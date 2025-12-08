use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::process::Command;

use crate::cli::CliInfo;
use crate::constants;
use crate::distro;
use crate::distro::PartInfo;
use crate::helper;
use crate::mount;
use crate::telemetry;
use anyhow::Result;
use log::debug;
use log::error;
use log::info;

enum Mountpoint {
    Mounted,
    NotMounted,
}

fn is_mountpoint(mountdir: &str) -> Result<Mountpoint> {
    let res = Command::new("mountpoint")
        .arg("-q")
        .arg(mountdir)
        .status()?;

    match res.success() {
        true => Ok(Mountpoint::Mounted),
        false => Ok(Mountpoint::NotMounted),
    }
}

fn has_lvm_partition(partitions: &[PartInfo]) -> bool {
    partitions.iter().any(|part| part.part_type == "8E00")
}

pub(crate) fn prepare_ade_environment(
    cli_info: &mut CliInfo,
    partitions: &[PartInfo],
) -> Result<bool> {
    let is_repair_vm = helper::is_repair_vm_imds()?;

    if is_repair_vm {
        match is_mountpoint(constants::INVESTIGATEROOT_DIR) {
            Ok(Mountpoint::Mounted) => {
                // With this validation we are running in a vm_repair which has automatically mounted the encrypted disk
                // ALAR requires to modify this setup
                modify_existing_ade_setup(partitions, cli_info)?;
                Ok(true)
            }
            Ok(Mountpoint::NotMounted) => {
                // We are running in a vm_repair but the encrypted disk is not mounted. This condition may happen after a repair vm got restarted
                mount_ade_manually(partitions, cli_info)?;
                Ok(true)
            }
            Err(e) => {
                error!("Error checking mountpoint: {e}");
                process::exit(1);
            }
        }
    } else {
        // This is the case when another VM is used to recover the encrypted disk
        println!("Not running in a repair VM context");
        if cli_info.ade_password.is_empty() {
            match read_pass_phrase_file() {
                Ok(_) => {
                    // BEK device is available and the password can be read from it
                    mount_ade_manually(partitions, cli_info)?;
                    Ok(true)
                }
                Err(e) => {
                    error!("Error reading the pass phrase file  {e} from the BEK disk");
                    error!("Please provide the password in base64 format to decrypt the disk.");
                    process::exit(1);
                }
            }
        } else {
            // The password is passed over to ALAR. We can use the password to mount the disk and proceed with the recovery process
            mount_ade_manually(partitions, cli_info)?;
            Ok(true)
        }
    }
}

/**
 The function modify_existing_ade_setup is used when ALAR is running in a repair VM context.
 This function relies on an existent BEK partition from which the password can be read.
*/
fn modify_existing_ade_setup(partitions: &[PartInfo], cli_info: &mut CliInfo) -> Result<()> {
    mount::umount(constants::INVESTIGATEROOT_DIR, true)?;
    if has_lvm_partition(partitions) {
        process::Command::new("vgchange")
            .arg("-an")
            .arg(constants::RESCUE_ROOTVG)
            .status()?;
    }
    process::Command::new("cryptsetup")
        .arg("close")
        .arg("osencrypt")
        .status()?;

    enable_encrypted_partition(cli_info, partitions)?;
    Ok(())
}

fn mount_ade_manually(partitions: &[PartInfo], cli_info: &mut CliInfo) -> Result<()> {
    info!("Mounting ADE encrypted disk manually");
    info!("Partitions: {:#?}", partitions);

    enable_encrypted_partition(cli_info, partitions)?;
    Ok(())
}

fn create_rescue_bek_dir() -> Result<()> {
    let command = format!("mkdir -p {}", constants::RESCUE_BEK);
    helper::run_cmd(&command).map_err(|open_error| {
        error!("Failed to create the BEK directory: {open_error}");
        open_error
    })?;

    Ok(())
}

fn create_rescue_bek_boot() -> Result<()> {
    let command = format!("mkdir -p {}", constants::RESCUE_BEK_BOOT);
    helper::run_cmd(&command).map_err(|open_error| {
        error!("Failed to create the BEK boot directory: {open_error}");
        open_error
    })?;

    Ok(())
}

fn find_root_partition_number(partitions: &[PartInfo]) -> i32 {
    let root_device = partitions
        .iter()
        .find(|part| part.fstype.contains("crypt?"));
    // unwrap is safe here because we know that there is a root partition
    root_device.unwrap().number
}

fn find_boot_partition_number(partitions: &[PartInfo]) -> i32 {
    let boot_partition = partitions
        .iter()
        .filter(|part| part.part_type != "EF00")
        .find(|part| part.fstype != "crypt?");
    // unwrap is safe here because we know that there is a boot partition
    boot_partition.unwrap().number
}

fn mount_bek_volume() -> Result<()> {
    create_rescue_bek_dir()?;
    let bek_volume = match helper::run_fun("blkid -t LABEL='BEK VOLUME' -o device") {
        Ok(device) => {
            debug!("BEK volume details: {device}");
            device
        }
        Err(e) => {
            error!("blkid raised an error : {e}");
            error!("Please set the password manually and run ALAR with the option :  --ade-password <password>");
            process::exit(1);
        }
    };
    if bek_volume.is_empty() {
        error!("There is no BEK VOLUME attached to the VM");
        error!("Please get the password manually and run ALAR with the option :  --ade-password <password>");
        process::exit(1);
    };

    mount::mount(bek_volume.trim(), constants::RESCUE_BEK, "", false)?;
    if !Path::new(constants::RESCUE_BEK_LINUX_PASS_PHRASE_FILE_NAME).exists() {
        error!("The pass phrase file doesn't exist. Please restart the VM to get the file LinuxPassPhraseFileName automatically created.");
        mount::umount(constants::RESCUE_BEK, false)?;
        process::exit(1);
    }
    Ok(())
}

fn umount_bek_volume() -> Result<()> {
    mount::umount(constants::RESCUE_BEK, false)?;
    Ok(())
}

fn read_pass_phrase_file() -> Result<String> {
    mount_bek_volume()?;
    let pass_phrase_file = fs::read_to_string(constants::RESCUE_BEK_LINUX_PASS_PHRASE_FILE_NAME)?;
    umount_bek_volume()?;
    Ok(pass_phrase_file)
}

fn mount_boot_partition(cli_info: &CliInfo, partitions: &[distro::PartInfo]) -> Result<()> {
    let boot_partition_number = find_boot_partition_number(partitions);
    let boot_partition_path = helper::get_recovery_disk_path(cli_info);
    create_rescue_bek_boot()?;
    mount::mount(
        &format!("{}{}", boot_partition_path, boot_partition_number),
        constants::RESCUE_BEK_BOOT,
        "",
        false,
    )?;
    Ok(())
}

fn umount_boot_partition() -> Result<()> {
    mount::umount(constants::RESCUE_BEK_BOOT, false)?;
    Ok(())
}

fn create_pass_phrase_file(phrase: &str) -> Result<()> {
    fs::write(constants::RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME, phrase)?;
    Ok(())
}

fn enable_encrypted_partition(
    cli_info: &mut CliInfo,
    partitions: &[distro::PartInfo],
) -> Result<()> {
    let partition_path = helper::get_recovery_disk_path(cli_info);
    let root_partiton_number = find_root_partition_number(partitions);

    let command: String = if cli_info.ade_password.is_empty() {
        // we verified earlier that the BEK does exists and is readable
        mount_bek_volume()?;
        mount_boot_partition(cli_info, partitions)?;
        format!(
            "cryptsetup luksOpen --key-file {} --header {}/luks/osluksheader {}{} rescueencrypt",
            constants::RESCUE_BEK_LINUX_PASS_PHRASE_FILE_NAME,
            constants::RESCUE_BEK_BOOT,
            partition_path,
            root_partiton_number
        )
    } else {
        create_pass_phrase_file(&cli_info.ade_password)?;
        mount_boot_partition(cli_info, partitions)?;
        format!(
            "cryptsetup luksOpen --key-file {} --header {}/luks/osluksheader {}{} rescueencrypt",
            constants::RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME,
            constants::RESCUE_BEK_BOOT,
            partition_path,
            root_partiton_number
        )
    };

    match process::Command::new("sh").arg("-c").arg(&command).status() {
        Ok(status) => {
            debug!("luksopen status: {}", &status);
            if status.success() {
                debug!("luksopen success");
            } else {
                debug!("luksopen failed");
                if cli_info.ade_password.is_empty() {
                    umount_bek_volume()?;
                }
                umount_boot_partition()?;
                close_rescueencrypt()?;
                telemetry::send_envelope(&telemetry::create_exception_envelope(telemetry::SeverityLevel::Error,
                    "ALAR EXCEPTION",
                     "Enabeling the encrypted device isn't possible.",
                     "enable_encrypted_partition() -> cryptsetup luksOpen raised an error",
                     cli_info,
                     &distro::Distro::default(),
                )).ok();
                error!("Error: Enabeling the encrypted device isn't possible. Please verify that the passphrase is correct. ALAR needs to stop.");
                process::exit(1);
            }
        }
        Err(e) => {
            umount_bek_volume()?;
            umount_boot_partition()?;
            fs::remove_file(constants::RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME)?;
            error!("Error: Enabeling the encrypted device isn't possible. ALAR needs to stop. Error detail is: {e}");
            telemetry::send_envelope(&telemetry::create_exception_envelope(telemetry::SeverityLevel::Error,
                "ALAR EXCEPTION",
                 "Enabeling the encrypted device isn't possible.",
                 "enable_encrypted_partition() -> cryptsetup luksOpen raised an error",
                 cli_info,
                 &distro::Distro::default(),
            )).ok();
            process::exit(1);
        }
    }
    umount_boot_partition()?;
    if cli_info.ade_password.is_empty() {
        umount_bek_volume()?;
    } else {
        // for security reasons we have to clear the ADE password
        cli_info.clear_password();
        fs::remove_file(constants::RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME)?;
    }

    Ok(())
}

pub(crate) fn ade_importvg() -> Result<()> {
    debug!("Inside ade_importvg");

    // Does the recover VM use LVM as well?
    if Path::new("/dev/rootvg").is_dir() {
        info!("Importing the rescuevg");

        let vgimportclone = format!(
            "vgimportclone -n rescuevg {}; vgchange -ay rescuevg;vgscan --mknodes",
            constants::ADE_OSENCRYPT_PATH
        );

        helper::run_cmd(&vgimportclone).map_err(|open_error| {
            error!("Failed to import the VG: {open_error}");
            open_error
        })?;

        ade_rename_rootvg()?;
    } else {
        let command = "vgchange -ay rootvg;vgscan --mknodes";
        helper::run_cmd(command).map_err(|open_error| {
            error!("Failed to activate the rootvg VG : {open_error}");
            open_error
        })?;
    };

    Ok(())
}

pub(crate) fn ade_rename_rootvg() -> Result<()> {
    debug!("Renaming the rootvg to oldvg and the rescuevg to rootvg");
    let command = "vgrename rootvg oldvg; vgrename rescuevg rootvg";
    helper::run_cmd(command).map_err(|open_error| {
        error!("Failed to rename the ADE VG: {open_error}");
        open_error
    })?;

    Ok(())
}

pub(crate) fn ade_lvm_cleanup() -> Result<()> {
    let command = if Path::new("/dev/oldvg").is_dir() {
        "vgchange -an rootvg; cryptsetup close rescueencrypt;vgrename oldvg rootvg"
    } else {
        "vgchange -an rootvg; cryptsetup close rescueencrypt"
    };

    helper::run_cmd(command).map_err(|open_error| {
        error!("Failed to cleanup the ADE VG: {open_error}");
        open_error
    })?;

    Ok(())
}

pub(crate) fn close_rescueencrypt() -> Result<()> {
    // Get out of constants::RESCUE_ROOT, otherwise umount isn't possible for RESCUE_ROOT
    match env::set_current_dir("/") {
        Ok(_) => {}
        Err(e) => println!("Error in set current dir : {e}"),
    }

    //mount::umount(constants::RESCUE_ROOT, true)?;
    let command = "cryptsetup close rescueencrypt";
    helper::run_cmd(command).map_err(|open_error| {
        error!("Failed to close rescueencrypt: {open_error}");
        open_error
    })?;

    Ok(())
}

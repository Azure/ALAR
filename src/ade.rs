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
use anyhow::Result;
use log::debug;
use log::error;
use log::info;
/*
For an encrypted Ubuntu OS disk we get the following details when a recovery VM got created and the encrypted disk is automatically encrypted and mounted

sdc
└─sdc1        vfat   FAT32 BEK VOLUME      A0E8-7B7F                                42M     0% /mnt/azure_bek_disk
sdd
├─sdd1
│ └─osencrypt ext4   1.0   cloudimg-rootfs 2d9ead34-ea62-403d-925d-84f18aac1e0c   25.4G    11% /investigateroot
├─sdd2        ext2   1.0                   0ccce532-a3f6-4869-b4f4-8a2edf26b9b4    9.6M    91% /investigateroot/boot
*/

// The key you find in the key-vault to encrypt the disk is wrapped in a base64. So you need to decode it first via base64 -d 'the key string in base64'
/*
Some further details to get to the pass phrase via az CLI

az keyvault secret list --vault-name ubuntu22-ade
[
  {
    "attributes": {
      "created": "2024-02-22T13:22:19+00:00",
      "enabled": true,
      "expires": null,
      "notBefore": null,
      "recoverableDays": 90,
      "recoveryLevel": "Recoverable+Purgeable",
      "updated": "2024-02-22T13:22:19+00:00"
    },
    "contentType": "BEK",
    "id": "https://ubuntu22-ade.vault.azure.net/secrets/8dd9285a-6848-401f-a392-8194d0e91188",
    "managed": null,
    "name": "8dd9285a-6848-401f-a392-8194d0e91188",
    "tags": {
      "DiskEncryptionKeyEncryptionAlgorithm": "RSA-OAEP",
      "DiskEncryptionKeyFileName": "LinuxPassPhraseFileName",
      "MachineName": "ubuntu22-ade"
    }
  }
]

  az keyvault secret show --id https://ubuntu22-ade.vault.azure.net/secrets/8dd9285a-6848-401f-a392-8194d0e91188
{
  "attributes": {
    "created": "2024-02-22T13:22:19+00:00",
    "enabled": true,
    "expires": null,
    "notBefore": null,
    "recoverableDays": 90,
    "recoveryLevel": "Recoverable+Purgeable",
    "updated": "2024-02-22T13:22:19+00:00"
  },
  "contentType": "BEK",
  "id": "https://ubuntu22-ade.vault.azure.net/secrets/8dd9285a-6848-401f-a392-8194d0e91188/4aa928f575d843889cc4ca71018cf565",
  "kid": null,
  "managed": null,
  "name": "8dd9285a-6848-401f-a392-8194d0e91188",
  "tags": {
    "DiskEncryptionKeyEncryptionAlgorithm": "RSA-OAEP",
    "DiskEncryptionKeyFileName": "LinuxPassPhraseFileName",
    "MachineName": "ubuntu22-ade"
  },
  "value": "UVd5eWtvK0Vnb1BIdEFBNnZyMWhDWnlkOWNncFFjL1BEYjZ2b2xFd0xLNm1YU1BYb2w2Zlcvb0N5d3ZXMW5hMldtMExDUzlRZTAvbS8yaUVoaWo1Q29LOU1mQkR1a0hnSWpBNFE5MGJZbnBROGdFNVFjVzNnS0E4TTJYUmdCTlFmNTludEs5WUFRYlQ1Q1NUanIxQytnaVRhMnVKQ0NoNWl2Q1QwLzZBK1E9PQ=="
}

The value is the base64 encoded key which you need to decode to get the pass phrase
*/

// Ideas to check whetehr we have to deal with ADE automatically mounted by creating a repair-VM
// mountdir --> https://www.baeldung.com/linux/bash-is-directory-mounted
// Soof interest is to know whether there exists a mountpoint for /investigateroot
// i iti mounted we can be sure we are in an vmrepir context
// if not the question is whether the engineer needs ADE encryption, in this case a manually encryption process as decribed above is required.
// The password is passed over to ALAR. Option name could be 'password' for instance.
// IF the password is passed over we can use the password to mount the disk and proceed with the recovery process. Thismounting procedure shoudl be covered in this module.

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
fn modify_existing_ade_setup(
    partitions: &[PartInfo],
    cli_info: &mut CliInfo,
) -> Result<()> {
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

fn mount_ade_manually(
    partitions: &[PartInfo],
    cli_info: &mut CliInfo,
) -> Result<()> {
    info!("Mounting ADE encrypted disk manually");
    info!("Password: {}", cli_info.ade_password);
    info!("Partitions: {:#?}", partitions);

    enable_encrypted_partition(cli_info, partitions)?;
    Ok(())
}

fn create_rescue_bek_dir() -> Result<()> {
    Command::new("mkdir")
        .arg("-p")
        .arg(constants::RESCUE_BEK)
        .status()?;

    Ok(())
}

fn create_rescue_bek_boot() -> Result<()> {
    Command::new("mkdir")
        .arg("-p")
        .arg(constants::RESCUE_BEK_BOOT)
        .status()?;

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
    let bek_volume = match cmd_lib::run_fun!(blkid -t LABEL="BEK VOLUME" -o device) {
        Ok(device) => device,
        Err(e) => {
            error!("There is no BEK VOLUME attached to the VM: {e}");
            error!("Please get the password manually and run ALAR with the option :  --ade-password <password>");
            process::exit(1);
        }
    };
    mount::mount(bek_volume.trim(), constants::RESCUE_BEK, "", false)?;
    if !Path::new(constants::RESCUE_BEK_LINUX_PASS_PHRASE_FILE_NAME).exists() {
        error!("The pass phrase file doesn't exist. Please restart the VM to get the file LinuxPassPhraseFileName automatically created.");
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
    let mut command = "".to_string();

    if cli_info.ade_password.is_empty() {
        // we verified earlier that the BEK does exists and is readable
        mount_bek_volume()?;
        mount_boot_partition(cli_info, partitions)?;
        command = format!(
            "cryptsetup luksOpen --key-file {} --header {}/luks/osluksheader {}{} rescueencrypt",
            constants::RESCUE_BEK_LINUX_PASS_PHRASE_FILE_NAME,
            constants::RESCUE_BEK_BOOT,
            partition_path,
            root_partiton_number
        );
    } else {
        create_pass_phrase_file(&cli_info.ade_password)?;
        mount_boot_partition(cli_info, partitions)?;
        command = format!(
            "cryptsetup luksOpen --key-file {} --header {}/luks/osluksheader {}{} rescueencrypt",
            constants::RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME,
            constants::RESCUE_BEK_BOOT,
            partition_path,
            root_partiton_number
        );
    }

    match process::Command::new("sh").arg("-c").arg(&command).status() {
        Ok(status) => {
            debug!("luksopen status: {}", &status);
            if status.success() {
                debug!("luksopen success");
            } else {
                if cli_info.ade_password.is_empty() {
                    umount_bek_volume()?;
                }
                umount_boot_partition()?;
                error!("Error: Enabeling the encrypted device isn't possible. Please verify that the passphrase is correct. ALAR needs to stop.");
                process::exit(1);
            }
        }
        Err(e) => {
            umount_bek_volume()?;
            umount_boot_partition()?;
            fs::remove_file(constants::RESCUE_TMP_LINUX_PASS_PHRASE_FILE_NAME)?;
            error!("Error: Enabeling the encrypted device isn't possible. ALAR needs to stop. Error detail is: {e}");
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

    // Does the recover VM do use LVM as well?
    if Path::new("/dev/rootvg").is_dir() {
        info!("Importing the rescuevg");

        let vgimportclone = format!(
            "vgimportclone -n rescuevg {}; vgchange -ay rescuevg;vgscan --mknodes",
            constants::ADE_OSENCRYPT_PATH
        );

        process::Command::new("bash")
            .arg("-c")
            .arg(vgimportclone)
            .status()
            .map_err(|open_error| {
                error!("Failed to import the VG: {open_error}");
                open_error
            })?;

        ade_rename_rootvg()?;
    } else {
        let command = "vgchange -ay rootvg;vgscan --mknodes";

        process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .status()
            .map_err(|open_error| {
                error!("Failed the rootvg VG : {open_error}");
                open_error
            })?;
    };

    Ok(())
}

pub(crate) fn ade_rename_rootvg() -> Result<()> {
    debug!("Renaming the rootvg to oldvg and the rescuevg to rootvg");
    let command = "vgrename rootvg oldvg; vgrename rescuevg rootvg";

    process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .status()
        .map_err(|open_error| {
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

    process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .status()
        .map_err(|open_error| {
            error!("Failed to cleanup the ADE VG: {open_error}");
            open_error
        })?;
    Ok(())
}

pub(crate) fn close_rescueencrypt() -> Result<()> {
    let command = "cryptsetup close rescueencrypt";

    process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .status()
        .map_err(|open_error| {
            error!("Failed to close rescueencrypt: {open_error}");
            open_error
        })?;
    Ok(())
}

/*

Muss aufgesplittedwerden in eine Überprfung obe die Disk enrypted ist und ob ALAR innerhalb von 'vm repair' genutzt wird oder ob der Engineer das Passwort eingeben muss.
Eine andere Überlegung ist ob es möglich festzustellen ob wir von einer 'vm repair' ENV aufgerufen werden.
-->  "tagsList": [
  {
    "name": "repair_source",
    "value": "redhat86_rg/red86"
  }
],
die tagliste kann gnutzt werden um festzustellen ob wir von einer 'vm repair' ENV aufgerufen werden.
Die default query is : curl -s -H Metadata:true --noproxy "*" "http://169.254.169.254/metadata/instance?api-version=2021-02-01" | jq
muss fein getunt werden um zu ermitteln ob es eine 'repair_source' gibt

Für telemtery könten weiter Variablen bein Aufruf von ALAR genutzt werden z.B.: initiator=SELFHELP

Für die Ermittlung ob ein Passwort gestzt wure kann as über die Umbgebun ermittelt werden? Dann mus ich nicht clap befragen hierfür




 */

/*
if is_mounted {
    do_ade_steps(false);
    true
} else {
    println!("One of the partitions got identified as encrypted. If this is correct please provide the password to decrypt the disk.");
    process::exit(1);
}
*/

use crate::cli::CliInfo;
use crate::constants;
use crate::distro;
use crate::helper;
use anyhow::Result;
use log::debug;
use log::error;
use log::info;
use log::log_enabled;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::{fs, process};

pub(crate) fn mkdir_assert() -> Result<()> {
    fs::create_dir_all(constants::ASSERT_PATH).map_err(|open_error| {
        error!("Error while creating the assert directory: {open_error}");
        open_error
    })?;
    Ok(())
}

pub(crate) fn mkdir_rescue_root() -> Result<()> {
    fs::create_dir_all(constants::RESCUE_ROOT).map_err(|open_error| {
        error!("Error while creating the rescue-root directory: {open_error}");
        open_error
    })?;
    Ok(())
}

pub(crate) fn mount(source: &str, destination: &str, option: &str, is_relaxed: bool) -> Result<()> {
    // There is an issue on Ubuntu that the XFS filesystem is not enabled by default
    // We need to load the driver first
    process::Command::new("modprobe").arg("xfs").status().map_err(|open_error | {
        error!("Loading of the module xfs was not possible. This may result in mount issues! : {open_error}");
        open_error
    })?;

    let supported = match sys_mount::SupportedFilesystems::new() {
        Ok(supported) => supported,
        Err(open_error) => {
            error!("Failed to get supported file systems: Detail {open_error}");
            error!("This is a severe issue for ALAR. Aborting.");
            process::exit(1);
        }
    };

    sys_mount::Mount::builder()
        .fstype(&supported)
        .flags(sys_mount::MountFlags::empty())
        .data(option)
        .mount(source, destination)
        .map_err(|mount_error| {
            error!("Failed to mount {source} on {destination}: {mount_error}");
            if !is_relaxed {
                error!("This is a severe issue for ALAR. Aborting.");
                process::exit(1);
            }
            mount_error
        })?;

    Ok(())
}

pub(crate) fn umount(destination: &str, recursive: bool) -> Result<()> {
    if recursive {
        process::Command::new("umount")
            .arg("-R")
            .arg(destination)
            .status()
            .map_err(|umount_error| {
                error!("Failed to unmount {destination}: {umount_error}");
                error!("This could cause severe issues.");
                umount_error
            })?;
        Ok(())
    } else {
        sys_mount::unmount(destination, sys_mount::UnmountFlags::DETACH).map_err(
            |umount_error| {
                error!("Failed to unmount {destination}: {umount_error}");
                error!("This shouldn't cause a severe issue for ALAR.");
                umount_error
            },
        )?;
        Ok(())
    }
}

pub(crate) fn fsck_partition(partition_path: &str) -> Result<()> {
    let mut exit_code = Some(0i32);
    let partition_filesystem =
        if let Ok(pfs) = distro::Distro::get_partition_filesystem(partition_path) {
            pfs
        } else {
            error!("Failed to get the partition filesystem. ALAR is not able to proceed further!");
            process::exit(1);
        };

    match partition_filesystem.as_str() {
        "xfs" => {
            info!("fsck for XFS on {partition_path}");

            // In case the filesystem has valuable metadata changes in a log which needs to
            // be replayed mount the filesystem to replay the log, and unmount it before
            // re-running xfs_repair
            mount(partition_path, constants::ASSERT_PATH, "nouuid", false)?;
            umount(constants::ASSERT_PATH, false)?;

            if let Ok(stat) = process::Command::new("xfs_repair")
                .arg(partition_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                exit_code = stat.code();
            }

            /*
               Because of RedHat9 a second validation needs to be performed
               as xfs_repair on Ubuntu isn't able to cope with the newer XFS v5 format which is used on RedHat9
               --> Found unsupported filesystem features
            */

            if let Ok(value) = process::Command::new("xfs_repair")
                .args([partition_path])
                .output()
            {
                // unwrap should be safe here as we get a result returned
                // xfs_repair is throwing an error, thus we need to use stderr
                let result_value = String::from_utf8(value.stderr).unwrap();
                if result_value.contains("Found unsupported filesystem features") {
                    exit_code = Some(0);
                }
            }
        }
        "fat16" => {
            info!("fsck for fat16/vfat");
            if let Ok(stat) = process::Command::new("fsck.vfat")
                .args(["-p", partition_path])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                exit_code = stat.code();
            }
        }
        _ => {
            info!("fsck for {partition_filesystem}");
            if let Ok(stat) = process::Command::new(format!("fsck.{partition_filesystem}"))
                .args(["-p", partition_path])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                exit_code = stat.code();
            }
        }
    }

    match exit_code {
        // error 4 is returned by fsck.ext4 only
        Some(_code @ 4) => {
            error!(
                "Partition {} can not be repaired in auto mode",
                &partition_path
            );
            error!("Stopping ALAR");
            process::exit(1);
        }
        // xfs_repair -n returns 1 if the fs is corrupted.
        // Also fsck may raise this error but we ignore it as even a normal recover is raising it. FALSE-NEGATIVE
        Some(_code @ 1) if partition_filesystem == "xfs" => {
            error!("A general error occured while trying to recover the device {partition_path}.");
            error!("Stopping ALAR");
            process::exit(1);
        }
        None => {
            panic!(
                "fsck operation terminated by signal error. ALAR is not able to proceed further!"
            );
        }

        // Any other error state is not of interest for us
        _ => {}
    }

    info!("File system check finished");

    Ok(())
}

pub(crate) fn rmdir(path: &str) -> Result<()> {
    fs::remove_dir_all(path)?;
    Ok(())
}

pub(crate) fn importvg(cli_info: &crate::cli::CliInfo, partition_number: i32) -> Result<()> {
    debug!("Inside importvg.");
    /*
       Save the old mounts
       We need to restore the old mounts later
    */
    let command = "lsblk -ln -o NAME,MOUNTPOINT | grep -e boot | sed -r 's/[[:space:]]+/:/'";
    let old_mounts = process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()?
        .stdout;

    let mut items: HashMap<&str, &str> = HashMap::new();
    let old_mount_lines = String::from_utf8(old_mounts)?;

    debug!("importvg :: Old mounts: {}", old_mount_lines);

    old_mount_lines.lines().for_each(|line| {
        let mut parts = line.split(':');
        let value = parts.next().unwrap();
        let key = parts.next().unwrap();
        items.insert(key, value);
    });

    //helper::run_cmd("pvscan; vgscan --mknodes; udevadm trigger")?;
    helper::run_cmd("pvscan;")?;
    let voulme_groups = helper::run_fun("vgs --noheadings -o vg_name;")?;

    // If we have found the rescuevg to be available then we can skip the import
    if voulme_groups.contains("rescuevg") {
        return Ok(());
    }

    // Either this is a RedHat based distro where the rootvg is also in use
    // or the rootvg became activated while adding the broken disk to the recover VM
    // some extra stuff is to be performed
    if Path::new("/dev/rootvg").is_dir() {
        // Let us figure out whether there are more than two rootvg's
        // If there are more than two rootvg's we need to do the import
        let result_string = helper::run_fun(r"pvscan  2>&1 | grep -v 'WARNING\|duplicate\|Total'")?;
        let mut rootvg_count = 0;

        result_string.lines().for_each(|line| {
            if line.contains("rootvg") {
                rootvg_count += 1;
            }
        });

        debug!("Number of rootvg's found: {rootvg_count}");

        if rootvg_count == 1 {
            debug!("Only one rootvg found. Skipping the import.");
            Ok(())
        } else {
        debug!("The rootvg is in use. We need to rename the rootvg to oldvg and the rescuevg to rootvg");
            let disk_path = format!(
                "{}{}",
                helper::get_recovery_disk_path(cli_info),
                partition_number
            );

            helper::run_cmd(&format!(
                "vgimportclone -n rescuevg {disk_path}; vgscan --mknodes"
            ))?;

            helper::run_cmd("vgrename rootvg oldvg; vgrename rescuevg rootvg; vgchange -ay")?;

            if items.contains_key("/boot/efi") {
                match umount("/boot/efi", false) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error umount /boot/efi : {e}");
                    }
                }
            }

            if items.contains_key("/boot") {
                match umount("/boot", false) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error umount /boot : {e}");
                    }
                }
            }

            if let Some(device_boot) = items.get("/boot") {
                mount(&format!("/dev/{device_boot}"), "/boot", "", false)?;
            }

            if let Some(device_efi) = items.get("/boot/efi") {
                mount(&format!("/dev/{device_efi}"), "/boot/efi", "", false)?;
            }

            Ok(())
        }
    } else {
        // No import is necessary
        Ok(())
    }
}

pub(crate) fn rename_oldvg() {
    debug!("Inside rename_oldvg");
    if helper::run_cmd("vgrename oldvg rootvg").is_err() {
        error!("Failed to rename oldvg to rootvg");
    }
}

pub(crate) fn rescan_host() -> Result<()> {
    debug!("Inside rescan_host");

    let old_mounts = helper::run_fun(
        "lsblk -ln -o NAME,MOUNTPOINT | grep -e boot | sed -r 's/[[:space:]]+/:/'",
    )?;

    debug!("Rescanning the host");
    match fs::write("/sys/class/scsi_host/host1/scan", b"- - -") {
        Ok(_) => {}
        Err(e) => {
            error!("Error writing to /sys/class/scsi_host/host1/scan: {e}");
            error!("It might be necessary to rescan the scsi host manually");
        }
    }

    match helper::run_cmd("udevadm trigger") {
        Ok(_) => {
            println!("udevadm trigger was successful")
        }
        Err(e) => {
            error!("rescan_host :: udevadm triger raised an err or wasn't able to be executed. Some error got thrown: {e}");
        }
    }

    let mut items: HashMap<&str, &str> = HashMap::new();
    old_mounts.lines().for_each(|line| {
        let mut parts = line.split(':');
        let value = parts.next().unwrap();
        let key = parts.next().unwrap();
        items.insert(key, value);
    });

    // The rescan has the side effect that the boot and efi partitions get automatically mounted
    // But as the UUIDs are the same they are mounted to the recover disk
    // This is why we need to unmount them and remount them to the correct location

    items.iter().for_each(|(key, value)| {
        println!("Key: {}, Value: {}", key, value);
    });

    if items.contains_key("/boot/efi") {
        umount("/boot/efi", false)?;
    }

    if items.contains_key("/boot") {
        umount("/boot", false)?;
    }

    if let Some(device_boot) = items.get("/boot") {
        println!("Device boot: {}", device_boot);
        mount(&format!("/dev/{device_boot}"), "/boot", "", false)?;
    }

    if let Some(device_efi) = items.get("/boot/efi") {
        println!("Device efi: {}", device_efi);
        mount(&format!("/dev/{device_efi}"), "/boot/efi", "", false)?;
    }

    if log_enabled!(log::Level::Debug) {
        debug!("At the end of rescan_host. What about the mounts?");
        debug!("{}", helper::run_fun("lsblk -f")?);
    }
    Ok(())
}

pub(crate) fn disable_broken_disk(cli_info: &CliInfo) -> Result<()> {
    debug!("Inside disable_broken_disk");
    let recover_disk = helper::get_recovery_disk_path(cli_info).replace("/dev/", "");
    helper::run_cmd("vgchange -an rootvg")?;

    fs::write(format!("/sys/block/{}/device/delete", recover_disk), b"1")?;
    Ok(())
}
pub(crate) fn bind_mount(source: &str, destination: &str) -> Result<()> {
    let supported_fs = sys_mount::SupportedFilesystems::new()?;

    sys_mount::Mount::builder()
        .fstype(&supported_fs)
        .flags(sys_mount::MountFlags::BIND)
        .mount(source, destination)?;
    debug!("Bind mount {source} to {destination} was successful");
    Ok(())
}

use std::fs;
use crate::cli;
use crate::constants;
use crate::distro;
use crate::distro::LogicalVolumesType;
use crate::helper;
use crate::mount;
use anyhow::Result;
use log::debug;

pub(crate) fn prepare_chroot(distro: &distro::Distro, cli: &cli::CliInfo) -> Result<()> {
    mount_required_partitions(distro, cli)?;
    mkdir_support_filesystems()?;
    mount_support_filesystems()?;
    Ok(())
}
fn mount_required_partitions(distro: &distro::Distro, cli: &cli::CliInfo) -> Result<()> {
    //let rescue_disk_path = get_rescue_disk_path(distro, cli);

    let os_partition = distro
        .partitions
        .iter()
        .find(|partition| partition.contains_os)
        .unwrap();
    let efi_partition = distro
        .partitions
        .iter()
        .find(|partition| partition.part_type == "EF00")
        .unwrap();
    let boot_partition = distro
        .partitions
        .iter()
        .find(|partition| !partition.contains_os && partition.part_type != "EF00");


    // A closure is required to build the correct path for the rescue disk
    let cl_get_rescue_disk_path = || -> String {
        if distro.is_ade {
            constants::ADE_OSENCRYPT_PATH.to_string()
        } else {
            format!("{}{}",helper::get_recovery_disk_path(cli), os_partition.number)
        }
    };

    debug!("rescue_disk_path : {}", cl_get_rescue_disk_path());
    debug!("os_partition : {:?}", os_partition);
    debug!("efi_partition : {:?}", efi_partition);
    debug!("boot_partition : {:?}", boot_partition);

    // Create the rescue root directory
    mount::mkdir_rescue_root()?;

    // Mount each lv if we have them available
    // This does mount ADE and non ADE partitions/lvs
    if let LogicalVolumesType::Some(lv_set) = &os_partition.logical_volumes {
        // First mount the rootlv, otherwise we get mount errors if we continue with the wrong order
        lv_set
            .iter()
            .filter(|root_lv| root_lv.name == "rootvg-rootlv")
            .for_each(|root_lv| {
                match mount::mount(
                    &format!("{}{}", "/dev/mapper/", root_lv.name),
                    constants::RESCUE_ROOT,
                    "",
                    false,
                ) {
                    Ok(()) => {}
                    Err(e) => {
                        let _ = helper::cleanup(distro, cli);
                        panic!(
                            "Unable to mount the logical volume : {} Error is: {}",
                            root_lv.name, e
                        );
                    }
                }
            });
        lv_set
            .iter()
            .filter(|volume| {
                volume.name != "rootvg-rootlv" && volume.name != "rootvg-tmplv" 
            })
            .for_each(|lv| {
                match mount::mount(
                    &format!("{}{}", "/dev/mapper/", lv.name),
                    &format!(
                        "{}{}",
                        constants::RESCUE_ROOT,
                        lv.name
                            .strip_prefix("rootvg-")
                            .unwrap()
                            .strip_suffix("lv")
                            .unwrap()
                    ),
                    "",
                    false,
                ) {
                    Ok(()) => {}
                    Err(e) => {
                        panic!(
                            "Unable to mount the logical volume : {} Error is: {}",
                            lv.name, e
                        );
                    }
                }
            });
    } else {
        // RAW disks gets mounted here
        mount::mount(
            &cl_get_rescue_disk_path(),
            constants::RESCUE_ROOT,
            "",
            false,
        )?;
    }

    // Even if we have an ADE encrpted disk the boot partition and the efi partition are not encrypted
    let rescue_disk_path = helper::get_recovery_disk_path(cli);

    // The order is again important. First /boot then /boot/efi
    // Verify also if we have a boot partition, Ubuntu doesn't have one for example
    if boot_partition.is_some() {
        if let Some(boot_partition) = boot_partition {
            mount::mount(
                &format!("{}{}", rescue_disk_path, boot_partition.number),
                constants::RESCUE_ROOT_BOOT,
                "",
                false,
            )?;
        }
    }

    mount::mount(
        &format!("{}{}", rescue_disk_path, efi_partition.number),
        constants::RESCUE_ROOT_BOOT_EFI,
        "",
        false,
    )?;

    Ok(())
}

fn mount_support_filesystems() -> Result<()> {
    for fs in constants::SUPPORT_FILESYSTEMS.split(' ') {
        mount::bind_mount(
            format!("/{fs}/").as_str(),
            format!("{}{fs}", constants::RESCUE_ROOT).as_str(),
        )?;
    }

    Ok(())
}

fn mkdir_support_filesystems() -> Result<()> {
    for fs in constants::SUPPORT_FILESYSTEMS.split(' ') {
        fs::create_dir_all(format!("{}/{}", constants::RESCUE_ROOT, fs))?;
    }
    Ok(())
}

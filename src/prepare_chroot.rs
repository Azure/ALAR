use crate::cli;
use crate::constants;
use crate::distro;
use crate::distro::LogicalVolumesType;
use crate::distro::PartInfo;
use crate::helper;
use crate::mount;
use crate::telemetry;
use anyhow::Result;
use log::debug;
use log::error;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;

pub(crate) fn prepare_chroot(distro: &distro::Distro, cli: &cli::CliInfo) -> Result<()> {
    let mut partition_details: HashMap<&str, &PartInfo> = HashMap::new();

    mount_required_partitions(distro, cli, &mut partition_details)?;
    mkdir_support_filesystems()?;
    mount_support_filesystems()?;
    set_environment(distro, cli, partition_details);
    Ok(())
}

fn convert_bool(state: bool) -> String {
    if state {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

fn get_distro_kind(distro: &distro::Distro) -> distro::DistroKind {
    match distro.distro_name_version.name.as_str() {
        s if s.contains("Ubuntu") => distro::DistroKind {
            distro_type: distro::DistroType::Ubuntu,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("Debian") => distro::DistroKind {
            distro_type: distro::DistroType::Debian,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("Red Hat") => distro::DistroKind {
            distro_type: distro::DistroType::RedHat,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("Oracle Linux") => distro::DistroKind {
            distro_type: distro::DistroType::RedHat,
            distro_subtype: distro::DistroSubType::OracleLinux,
        },
        s if s.contains("SLES") => distro::DistroKind {
            distro_type: distro::DistroType::Suse,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("Azure Linux") => distro::DistroKind {
            distro_type: distro::DistroType::AzureLinux,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("Linux Mariner") => distro::DistroKind {
            distro_type: distro::DistroType::AzureLinux,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("AlmaLinux") => distro::DistroKind {
            distro_type: distro::DistroType::RedHat,
            distro_subtype: distro::DistroSubType::AlmaLinux,
        },
        s if s.contains("Rocky Linux") => distro::DistroKind {
            distro_type: distro::DistroType::RedHat,
            distro_subtype: distro::DistroSubType::RockyLinux,
        },
        s if s.contains("CentOS") => distro::DistroKind {
            distro_type: distro::DistroType::RedHat,
            distro_subtype: distro::DistroSubType::CentOS,
        },
        _ => distro::DistroKind {
            distro_type: distro::DistroType::Undefined,
            distro_subtype: distro::DistroSubType::None,
        },
    }
}

// A helper function to get the corrected recovery disk path
// depending if we have an NVMe controller or not
fn get_corrected_recover_path(cli_info: &cli::CliInfo) -> String {
    if helper::is_nvme_controller().unwrap_or(false) {
        format!("{}p", helper::get_recovery_disk_path(cli_info))
    } else {
        helper::get_recovery_disk_path(cli_info)
    }
}

pub fn set_environment(
    distro: &distro::Distro,
    cli_info: &cli::CliInfo,
    partitions: HashMap<&str, &PartInfo>,
) {
    let distroname = &distro.distro_name_version.name;
    let distroversion = &distro.distro_name_version.version_id;
    let distrokind = get_distro_kind(distro);
    debug!("Distro kind: {:?}", distrokind);

    // some default values which can be always of help
    env::set_var("DISTRONAME", format!("'{}'", distroname.as_str()));
    env::set_var("DISTROVERSION", distroversion.as_str());
    env::set_var("isLVM", convert_bool(distro.is_lvm));
    env::set_var(
        "RECOVER_DISK_PATH",
        helper::get_recovery_disk_path(cli_info),
    );
    env::set_var(
        "OS_PARTITION",
        partitions.get("os").unwrap().number.to_string(),
    );
    if partitions.contains_key("boot") {
        env::set_var(
            "BOOT_PARTITION",
            partitions.get("boot").unwrap().number.to_string(),
        );
        env::set_var(
            "boot_part_path",
            format!(
                "{}{}",
                get_corrected_recover_path(cli_info),
                partitions.get("boot").unwrap().number
            ),
        );
    }
    if partitions.contains_key("efi") {
        env::set_var(
            "EFI_PARTITION",
            partitions.get("efi").unwrap().number.to_string(),
        );
        env::set_var(
            "efi_part_path",
            format!(
                "{}{}",
                get_corrected_recover_path(cli_info),
                partitions.get("efi").unwrap().number
            ),
        );
    }

    // Remove this variable because of security reasons
    env::remove_var("SUDO_COMMAND");

    debug!("Distro name: {distroname}");
    debug!("Distro version: {distroversion}");

    match distrokind {
        dkind if dkind.distro_type == distro::DistroType::RedHat => {
            debug!("Type {} detected", dkind.distro_type);
            env::set_var("isRedHat", convert_bool(true));
            let distrosubtype = dkind.distro_subtype;
            debug!("Subtype: {distrosubtype}");
            env::set_var("DISTROSUBTYPE", format!("{}", distrosubtype));
        }
        dkind if dkind.distro_type == distro::DistroType::Ubuntu => {
            debug!("Type {} detected", dkind.distro_type);
            env::set_var("isUbuntu", convert_bool(true));
            let distrosubtype = dkind.distro_subtype;
            debug!("Subtype: {distrosubtype}");
            env::set_var("DISTROSUBTYPE", format!("{}", distrosubtype));
        }
        dkind if dkind.distro_type == distro::DistroType::Suse => {
            debug!("Type {} detected", dkind.distro_type);
            env::set_var("isSuse", convert_bool(true));
            let distrosubtype = dkind.distro_subtype;
            debug!("Subtype: {distrosubtype}");
            env::set_var("DISTROSUBTYPE", format!("{}", distrosubtype));
        }
        dkind if dkind.distro_type == distro::DistroType::AzureLinux => {
            debug!("Type {} detected", dkind.distro_type);
            env::set_var("isAzureLinux", convert_bool(true));
            let distrosubtype = dkind.distro_subtype;
            debug!("Subtype: {distrosubtype}");
            env::set_var("DISTROSUBTYPE", format!("{}", distrosubtype));
        }
        dkind if dkind.distro_type == distro::DistroType::Debian => {
            debug!("Type {} detected", dkind.distro_type);
            env::set_var("isDebian", convert_bool(true));
            let distrosubtype = dkind.distro_subtype;
            debug!("Subtype: {distrosubtype}");
            env::set_var("DISTROSUBTYPE", format!("{}", distrosubtype));
        }
        _ => {
            env::set_var("DISTROTYPE", "UNKNOWN");
            env::set_var("DISTROSUBTYPE", "UNKNOWN");
        }
    }
}

fn mount_required_partitions<'a>(
    distro: &'a distro::Distro,
    cli: &cli::CliInfo,
    partitions: &mut HashMap<&str, &'a PartInfo>,
) -> Result<()> {
    let os_part = distro
        .partitions
        .iter()
        .find(|partition| partition.contains_os)
        .unwrap(); // unwrap is safe as we have always an OS partition
    partitions.insert("os", os_part);

    let efi_part = distro
        .partitions
        .iter()
        .find(|partition| partition.part_type == "EF00");
    if let Some(efi_part) = efi_part {
        partitions.insert("efi", efi_part);
    }

    let boot_part = distro
        .partitions
        .iter()
        .find(|partition| !partition.contains_os && partition.part_type != "EF00");
    if let Some(boot_part) = boot_part {
        partitions.insert("boot", boot_part);
    }

    // A closure is required to build the correct path for the rescue disk
    let cl_get_rescue_disk_path = || -> String {
        if distro.is_ade {
            constants::ADE_OSENCRYPT_PATH.to_string()
        } else {
            match helper::is_nvme_controller() {
                Ok(_is_nvme @ true) => {
                        debug!("Detected NVMe controller for recovery disk.");
                        format!( "{}p{}", helper::get_recovery_disk_path(cli), partitions.get("os").unwrap().number)
                    }
               Ok(_is_nvme @ false) => {
                        debug!("Detected SCSI controller for recovery disk.");
                        format!( "{}{}", helper::get_recovery_disk_path(cli), partitions.get("os").unwrap().number)
                    }
                
                Err(e) => {
                    error!("Error detecting NVMe controller: {e}");
                    process::exit(1);
                }
            }
        }
    };

    debug!("rescue_disk_path : {}", cl_get_rescue_disk_path());
    debug!("os_partition : {:?}", partitions.get("os"));
    debug!("efi_partition : {:?}", partitions.get("efi"));
    debug!("boot_partition : {:?}", partitions.get("boot"));

    // Create the rescue root directory
    mount::mkdir_rescue_root()?;

    // Mount each lv if we have them available
    // This does mount ADE and non ADE partitions/lvs
    if let LogicalVolumesType::Some(lv_set) = &partitions.get("os").unwrap().logical_volumes {
        // First mount the rootlv, otherwise we get mount errors if we continue with the wrong order
        lv_set
            .iter()
            .filter(|root_lv| root_lv.name == "rootvg-rootlv")
            .for_each(|root_lv| {
                let options = if root_lv.fstype == "xfs" {
                    "nouuid"
                } else {
                    ""
                };
                match mount::mount(
                    &format!("{}{}", "/dev/mapper/", root_lv.name),
                    constants::RESCUE_ROOT,
                    options,
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
            .filter(|volume| volume.name != "rootvg-rootlv" && volume.name != "rootvg-tmplv")
            .for_each(|lv| {
                let options = if lv.fstype == "xfs" { "nouuid" } else { "" };
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
                    options,
                    false,
                ) {
                    Ok(()) => {}
                    Err(e) => {
                        telemetry::send_envelope(&telemetry::create_exception_envelope(telemetry::SeverityLevel::Error,
                            "ALAR EXCEPTION",
                             &format!("Unable to mount the logical volume : {}", lv.name),
                             "prepare_chroot() -> mount() raised an error",
                             cli,
                             distro,
                        )).ok();
                        panic!(
                            "Unable to mount the logical volume : {} Error is: {}",
                            lv.name, e
                        );
                    }
                }
            });
    } else {
        // RAW disks gets mounted here
        // mind the XFS double UUID issue
        let command = format!("lsblk -nf -o FSTYPE {}", cl_get_rescue_disk_path());
        let filesystem = helper::run_fun(&command)?;
        if filesystem.trim() == "xfs" {
            mount::mount(
                &cl_get_rescue_disk_path(),
                constants::RESCUE_ROOT,
                "nouuid",
                false,
            )?;
        } else {
            mount::mount(
                &cl_get_rescue_disk_path(),
                constants::RESCUE_ROOT,
                "",
                false,
            )?;
        }
    }

    // Even if we have an ADE encrpted disk the boot partition and the efi partition are not encrypted
    let rescue_disk_path = helper::get_recovery_disk_path(cli);

    //If we have a NVME controller, we need to add 'p' before the partition number
    let rescue_disk_path = if helper::is_nvme_controller().unwrap_or(false) {
        format!("{}p", rescue_disk_path)
    } else {
        rescue_disk_path
    };

    // The order is again important. First /boot then /boot/efi
    // Verify also if we have a boot partition, Ubuntu doesn't have one for example
    if partitions.get("boot").is_some() {
        // mind the XFS double UUID issue
        if let Some(boot_partition) = partitions.get("boot") {
            if partitions.get("boot").unwrap().fstype == "xfs" {
                mount::mount(
                    &format!("{}{}", rescue_disk_path, boot_partition.number),
                    constants::RESCUE_ROOT_BOOT,
                    "nouuid",
                    false,
                )?;
            } else {
                mount::mount(
                    &format!("{}{}", rescue_disk_path, boot_partition.number),
                    constants::RESCUE_ROOT_BOOT,
                    "",
                    false,
                )?;
            }
        }
    }

    // Also be carefull with the efi partition, not all distros have one
    if partitions.get("efi").is_some() {
        if let Some(efi_partition) = partitions.get("efi") {
            mount::mount(
                &format!("{}{}", rescue_disk_path, efi_partition.number),
                constants::RESCUE_ROOT_BOOT_EFI,
                "",
                false,
            )?;
        }
    }

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

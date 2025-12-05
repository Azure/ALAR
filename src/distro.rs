use crate::ade;
use crate::cli;
use crate::cli::CliInfo;
use crate::constants;
use crate::helper;
use crate::mount;
use crate::telemetry;
use anyhow::Result;
use log::debug;
use log::error;
use log::info;
use std::fmt::Display;
use std::{
    fs,
    path::Path,
    process::{self},
};

#[derive(Debug, Default)]
pub(crate) struct PartInfo {
    pub(crate) number: i32,
    pub(crate) part_type: String,
    pub(crate) fstype: String,
    pub(crate) contains_os: bool,
    pub(crate) logical_volumes: LogicalVolumesType,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogicalVolume {
    pub(crate) name: String,
    pub(crate) fstype: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum LogicalVolumesType {
    Some(Vec<LogicalVolume>),
    #[default]
    None,
}

#[derive(Debug, Default)]
pub(crate) struct DistroNameVersion {
    pub(crate) name: String,
    pub(crate) version_id: String,
}

#[derive(Debug, Default)]
pub(crate) struct Distro {
    pub(crate) partitions: Vec<PartInfo>,
    pub(crate) distro_name_version: DistroNameVersion,
    pub(crate) cli_info: CliInfo,
    pub(crate) is_ade: bool,
    pub(crate) is_lvm: bool,
    pub(crate) architecture: Architecture,
}

#[derive(Debug, PartialEq, Default)]
pub enum DistroType {
    Debian,
    Suse,
    RedHat,
    Ubuntu,
    AzureLinux,
    #[default]
    Undefined,
}

#[derive(Debug, Default, Copy, Clone)]
pub(crate) enum Architecture {
    #[default]
    X86_64,
    Aarch64,
}

impl Display for Architecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Architecture::X86_64 => write!(f, "x86_64"),
            Architecture::Aarch64 => write!(f, "aarch64"),
        }
    }
}

impl Display for DistroType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistroType::Debian => write!(f, "Debian"),
            DistroType::Suse => write!(f, "Suse"),
            DistroType::RedHat => write!(f, "RedHat"),
            DistroType::Ubuntu => write!(f, "Ubuntu"),
            DistroType::AzureLinux => write!(f, "AzureLinux"),
            DistroType::Undefined => write!(f, "Undefined"),
        }
    }
}

#[derive(Debug, PartialEq, Default)]
pub enum DistroSubType {
    CentOS,
    AlmaLinux,
    RockyLinux,
    OracleLinux,
    #[default]
    None,
}

impl Display for DistroSubType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistroSubType::CentOS => write!(f, "CentOS"),
            DistroSubType::AlmaLinux => write!(f, "AlmaLinux"),
            DistroSubType::RockyLinux => write!(f, "RockyLinux"),
            DistroSubType::OracleLinux => write!(f, "OracleLinux"),
            DistroSubType::None => write!(f, "None"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct DistroKind {
    pub(crate) distro_type: DistroType,
    pub(crate) distro_subtype: DistroSubType,
}

impl PartInfo {
    fn activate_is_os(&mut self) {
        self.contains_os = true;
    }
}

impl Distro {
    fn get_all_partitions(disk_path: &str) -> String {
        let command = format!("sgdisk {disk_path} -p | tail -n-5 | grep -E \"^ *[1,2,3,4,5,6]\" | grep -v EF02 | sed 's/[ ]\\+/ /g;s/^[ \t]*//' ");
        match helper::run_fun(&command) {
            Ok(partitions) => partitions,
            Err(e) => {
                error!("Error getting disk info for disk {}. Something went wrong : {}. ALAR is not able to proceed. Exiting.", disk_path, e);
                process::exit(1);
            }
        }
    }

    fn get_all_recovery_partitions(cli_info: &CliInfo) -> String {
        let recover_disk = helper::get_recovery_disk_path(cli_info);
        Self::get_all_partitions(&recover_disk)
    }

    /// Check whether LVM is in use on the repair VM
    fn is_lvm_on_repair_disk() -> bool {
        match helper::run_fun("findmnt | grep mapper | grep usr") {
            Ok(output) => !output.trim().is_empty(),
            Err(_) => false,
        }
    }

    // The logic in this function is intended to verify whther the repair VM is a RHEL 7.x or 8.x version
    // Only they can be used. With newer versions the functionality is broken to repair a LVM on a LVM based repair VM
    fn is_repairvm_with_lvm_allowed() -> bool {
        if Self::is_lvm_on_repair_disk() {
            match helper::get_repair_os_version() {
                Ok(version) => {
                    let local_os_version = version.trim();
                    debug!("Repair VM OS version detected as: {}", local_os_version);
                    if local_os_version.starts_with("7.") || local_os_version.starts_with("8.") {
                        // allowed
                        info!("LVM detected on the recovery VM disk. ALAR is able to handle LVM based recovery disks.");
                        true
                    } else {
                        error!("LVM detected on the recovery VM disk. However, the repair VM OS version is {}. Only RHEL 7.x and 8.x are supported for LVM based recovery disks. Exiting.", local_os_version);
                        false
                    }
                }
                Err(e) => {
                    error!("Error getting repair VM OS version: {}. Exiting.", e);
                    // We are cautious here and don't allow LVM based recovery disks if we can't determine the OS version
                    false
                }
            }
        } else {
            // no LVM in use on repair disk
            true
        }
    }

    pub(crate) fn get_partition_filesystem(partition_path: &str) -> Result<String> {
        let command = format!("file -sL {}", partition_path);
        let command_output_string = helper::run_fun(&command)?.to_lowercase();

        match command_output_string.as_str() {
            s if s.contains("xfs") => Ok("xfs".to_string()),
            s if s.contains("ext4") => Ok("ext4".to_string()),
            s if s.contains("ext2") => Ok("ext2".to_string()),
            s if s.contains("ext3") => Ok("ext3".to_string()),
            s if s.contains("lvm2") => Ok("LVM2_member".to_string()),
            s if s.contains("btrfs") => Ok("btrfs".to_string()),
            s if s.contains("zfs") => Ok("zfs".to_string()),
            s if s.contains("crypt") => Ok("crypt".to_string()),
            s if s.contains("fat") => Ok("vfat".to_string()),
            _ => Ok("".to_string()),
        }
    }

    fn get_partition_details(cli_info: &CliInfo) -> Vec<PartInfo> {
        let mut parts: Vec<PartInfo> = Vec::new();
        let disk_info = Self::get_all_recovery_partitions(cli_info);

        for line in disk_info.lines() {
            let v: Vec<&str> = line.trim().split(' ').collect();
            let _number = v[0].to_string().parse::<i32>().unwrap();
            let _part_type = v[5].to_string();

            let partition_path = if helper::is_nvme_controller().unwrap_or(false) {
                format!("{}p{}", helper::get_recovery_disk_path(cli_info), _number)
            } else {
                format!("{}{}", helper::get_recovery_disk_path(cli_info), _number)
            };

            let mut partition_fstype = if let Ok(pfs) =
                Self::get_partition_filesystem(&partition_path)
            {
                pfs
            } else {
                error!("Not able to determine the partition filesystem. ALAR is not able to proceed. Exiting.");
                telemetry::send_envelope(&telemetry::create_exception_envelope(
                    telemetry::SeverityLevel::Error,
                    "ALAR EXCEPTION",
                    "Not able to determine the partition filesystem.",
                    "Distro::get_partition_details() -> get_partition_filesystem() returned error",
                    cli_info,
                    &Distro::default(),
                ))
                .ok();
                process::exit(1);
            };

            // An empty filesystem info is a hint that the partition is usual encrypted if ADE is in use
            if partition_fstype.is_empty() {
                partition_fstype = "crypt?".to_string();
            } else {
                partition_fstype = partition_fstype.trim().to_string();
            }

            parts.push(PartInfo {
                number: _number,
                part_type: _part_type,
                fstype: partition_fstype,
                contains_os: false,
                logical_volumes: LogicalVolumesType::None,
            });
        }
        parts
    }

    fn build_logical_volume_details(
        part: &mut [PartInfo],
        cli_info: &CliInfo,
        distro: &mut Distro,
    ) {
        let mut lv: Vec<LogicalVolume> = Vec::new();

        part.iter_mut()
            .filter(|lvm| lvm.part_type == "8E00")
            .for_each(|part| {
                let lvm_partition = if helper::is_nvme_controller().unwrap_or(false) {
                    format!(
                        "{}p{}",
                        helper::get_recovery_disk_path(cli_info),
                        part.number
                    )
                } else {
                    format!(
                        "{}{}",
                        helper::get_recovery_disk_path(cli_info),
                        part.number
                    )
                };

                match mount::importvg(cli_info, part.number) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error importing VG: {e}");
                        process::exit(1);
                    }
                }

                if log::log_enabled!(log::Level::Debug) {
                    let lvscan = helper::run_fun("lvscan").unwrap();
                    debug!("lvscan after running importvg ");
                    let _ = &lvscan.lines().for_each(|line| debug!("{:#?}", line));
                }

                let lv_detail = helper::run_fun(&format!(
                    "lsblk -ln {} -o NAME,FSTYPE | sed '1d'",
                    lvm_partition
                ));

                let lv_detail_string =
                    lv_detail.expect("lsblk shouldn't raise an error when getting fs information");
                debug!(
                    "build_logical_volume_details: lv_detail_string: {:#?}",
                    &lv_detail_string
                );

                let recovery_disk_name = helper::get_recovery_disk_path(cli_info).split_off(5);
                for line in lv_detail_string.lines() {
                    if line.contains(&recovery_disk_name) {
                        continue;
                    }
                    let mut v: Vec<&str> = line.trim().split(' ').collect();
                    v.retain(|&x| !x.is_empty());

                    lv.push(LogicalVolume {
                        name: v[0].to_string(),
                        fstype: v[1].to_string(),
                    });
                }
                part.logical_volumes = LogicalVolumesType::Some(lv.clone());
                distro.is_lvm = true;
            });
    }

    /**
     * what_distro_name_version requires to mount and umount partitions. This is required to figure out what distro make and version we have to cope with.
     * After those details are collected the partitions are unmounted, so we can get them mounted later again during the recovery process.
     */
    fn what_distro_name_version(
        partitions: &mut Vec<PartInfo>,
        cli_info: &CliInfo,
        distro: &mut Distro,
    ) -> Option<DistroNameVersion> {
        let recovery_disk_path = helper::get_recovery_disk_path(cli_info);

        debug!("recovery_disk_path: {}", recovery_disk_path);
        debug!("what_distro: Partitions to be processed: {:#?}", partitions);

        if mount::mkdir_assert().is_err() {
            error!("Error creating assert dir. ALAR is not able to proceed. Exiting.");
            process::exit(1);
        }

        // cycling through each of the partitions to figure out what sort of partition we do have
        for partition in partitions {
            // EFI part no need to check
            if partition.part_type == "EF00" {
                continue;
            }

            if partition.part_type == "8E00" && partition.fstype == "LVM2_member" {
                // Due to issues with RHEL above version 9.x we need to check whether the repair VM is allowed to use LVM based recovery disks
                if !Self::is_repairvm_with_lvm_allowed() {
                    telemetry::send_envelope(&telemetry::create_exception_envelope(
                    telemetry::SeverityLevel::Error,
                    "ALAR EXCEPTION",
                      "LVM based recovery disks are not supported on repair VMs with OS version >= 9.x.",
                        "Distro::is_repairvm_allowed_to_use_lvm() -> get_repair_os_version() returned >= 9.x",
                        cli_info,
                        distro,
                )).ok();
                    process::exit(1);
                }

                debug!("Found LVM partition. Executing read_distro_name_version_from_lv");
                return Self::read_distro_name_version_from_lv(partition, distro.is_ade, cli_info);
            }

            // Above we handle any kind of LVM partition including an encrypted one.
            // Below we handle the rest of the non-LVM partitions including one which resides on an encrypted device.

            fn error_condition_mount(e: anyhow::Error) {
                error!("Error mounting partition: {e}");
                process::exit(1);
            }

            fn error_condition_umount(e: anyhow::Error) {
                error!("Error umounting partition: {e}, this may cause side effects");
                process::exit(1);
            }

            fn nouuid_option(fstype: &str) -> &str {
                if fstype == "xfs" {
                    "nouuid"
                } else {
                    ""
                }
            }

            let mount_path = match helper::is_nvme_controller() {
                Ok(_is_nvme @ true) => {
                    debug!("Detected NVMe controller for recovery disk.");
                    format!("{}p{}", &recovery_disk_path, partition.number)
                }
                Ok(_is_nvme @ false) => {
                    debug!("Detected SCSI controller for recovery disk.");
                    format!("{}{}", &recovery_disk_path, partition.number)
                }

                Err(e) => {
                    error!("Error detecting NVMe controller: {e}");
                    process::exit(1);
                }
            };

            debug!(
                "Mounting partition number {} to {}",
                partition.number, &mount_path
            );

            match partition.fstype.as_str() {
                fs if fs == "xfs" || fs == "ext4" => {
                    match mount::fsck_partition(&mount_path) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error fscking partition: {e}");
                            process::exit(1);
                        }
                    }

                    match mount::mount(
                        &mount_path,
                        constants::ASSERT_PATH,
                        nouuid_option(fs),
                        false,
                    ) {
                        Ok(_) => {}
                        Err(e) => error_condition_mount(e),
                    }
                }
                // If the partition is marked as 'crypt?' the partition path needs to be corrected
                "crypt?" => {
                    let partition_path = constants::ADE_OSENCRYPT_PATH;
                    let fstype =
                        Self::get_partition_filesystem(partition_path).unwrap_or("xfs".to_string());
                    debug!("Filesystem type for the encrypted partition is: {}", fstype);

                    match mount::fsck_partition(partition_path) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error fscking partition: {e}");
                            process::exit(1);
                        }
                    }

                    match mount::mount(
                        partition_path,
                        constants::ASSERT_PATH,
                        nouuid_option(&fstype),
                        false,
                    ) {
                        Ok(_) => {}
                        Err(e) => error_condition_mount(e),
                    }
                }
                _ => {
                    match mount::fsck_partition(&mount_path) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error fscking partition: {e}");
                            process::exit(1);
                        }
                    }

                    match mount::mount(&mount_path, constants::ASSERT_PATH, "", false) {
                        Ok(_) => {}
                        Err(e) => error_condition_mount(e),
                    }
                }
            }

            if Path::new(constants::OS_RELEASE).is_file() {
                // If we have found this file we can be sure this is the OS partition
                partition.activate_is_os();

                let mut _name = "".to_string();
                let mut _version_id = "".to_string();
                //unwrap is safe here because we checked if the file exists
                for line in fs::read_to_string(constants::OS_RELEASE).unwrap().lines() {
                    let detail = line.trim();
                    if detail.starts_with("NAME=") {
                        _name = detail
                            .strip_prefix("NAME=")
                            .unwrap()
                            .to_string()
                            .replace('"', "");
                    }
                    if detail.starts_with("VERSION_ID=") {
                        _version_id = detail
                            .strip_prefix("VERSION_ID=")
                            .unwrap()
                            .to_string()
                            .replace('"', "");
                    }
                }

                // What is the architecture of the system to be recovered?
                let file_bash_info = match helper::run_fun("file /tmp/assert/bin/bash") {
                    Ok(info) => info,
                    Err(e) => {
                        error!("Error getting bash info: {e}");
                        "".to_string()
                    }
                };

                if file_bash_info.contains("aarch64") {
                    distro.architecture = Architecture::Aarch64;
                } else {
                    distro.architecture = Architecture::X86_64;
                }

                match mount::umount(constants::ASSERT_PATH, false) {
                    Ok(_) => {}
                    Err(e) => error_condition_umount(e),
                }

                if mount::rmdir(constants::ASSERT_PATH).is_ok() {
                    info!("Removed assert path");
                } else {
                    error!("Error removing directory ASSER_PATH");
                }

                return Some(DistroNameVersion {
                    name: _name,
                    //version_id: _version_id.parse::<f32>().unwrap(),
                    version_id: _version_id,
                });
            }
            match mount::umount(constants::ASSERT_PATH, false) {
                Ok(_) => {}
                Err(e) => error_condition_umount(e),
            }
        }

        if mount::rmdir(constants::ASSERT_PATH).is_ok() {
            info!("Removed assert path");
        } else {
            error!("Error removing directory ASSER_PATH");
        }
        // If we reach this point we haven't found the OS partition
        // which could point out to operate on a data disk.
        None
    }

    fn ade_set_no_lvm_partiton_fs(partitions: &mut [PartInfo]) {
        // This will only affect the partitions which are marked as 'crypt?' as this is an indicator for an encrypted partition.
        partitions
            .iter_mut()
            .filter(|partition| partition.fstype == "crypt?")
            .for_each(|part| {
                part.fstype = Self::get_partition_filesystem(constants::ADE_OSENCRYPT_PATH)
                    .unwrap_or("xfs".to_string());
            });
    }

    fn read_distro_name_version_from_lv(
        partinfo: &mut PartInfo,
        is_ade: bool,
        cli_info: &CliInfo,
    ) -> Option<DistroNameVersion> {
        let volumes = &partinfo.logical_volumes;
        let mut _name = "".to_string();
        let mut _version_id = "".to_string();

        debug!(
            "read_distro_name_version_from_lv :: Detail of the patitions to be processed: {:#?}",
            partinfo
        );

        if let LogicalVolumesType::Some(lv) = volumes {
            if lv.is_empty() {
                error!("No rootlv found in LVM. This is a not supported LVM setup. ALAR is not able to proceed. Exiting.");
                telemetry::send_envelope(&telemetry::create_exception_envelope(telemetry::SeverityLevel::Error,
                    "ALAR EXCEPTION",
                     "No rootlv found in LVM.",
                     "Distro::read_distro_name_version_from_lv() -> LogicalVolumesType::Some returned empty vector",
                     cli_info,
                     &Distro::default(),
                )).ok();
                process::exit(1);
            }
            // Find the rootlv and mount it
            lv.iter()
                .filter(|volume| volume.name.contains("rootlv"))
                .for_each(|volume| {
                    let mount_option = if volume.fstype == "xfs" { "nouuid" } else { "" };

                    let partition_path = if is_ade {
                        constants::RESCUE_ADE_ROOTLV
                    } else {
                        constants::ROOTVG_ROOTLV
                    };

                    match mount::fsck_partition(partition_path) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error fscking rescuevg-rootlv: {e}");
                            process::exit(1);
                        }
                    }
                    if mount::mount(partition_path, constants::ASSERT_PATH, mount_option, false)
                        .is_err()
                    {
                        error!(
                            "Error mounting rescue-rootlv. ALAR is not able to proceed. Exiting."
                        );
                        process::exit(1);
                    }
                });
            // Find the usrlv and mount it
            lv.iter()
                .filter(|volume| volume.name.contains("usrlv"))
                .for_each(|volume| {
                    let mount_option = if volume.fstype == "xfs" { "nouuid" } else { "" };

                    let partition_path = if is_ade {
                        constants::RESCUE_ADE_USRLV
                    } else {
                        constants::ROOTVG_USRLV
                    };

                    match mount::fsck_partition(partition_path) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error fscking rescuevg-usrlv: {e}");
                            process::exit(1);
                        }
                    }

                    if mount::mount(
                        partition_path,
                        constants::ASSERT_PATH_USR,
                        mount_option,
                        true,
                    )
                    .is_err()
                    {
                        error!(
                            "Error mounting rescue-usrlv. ALAR is not able to proceed. Exiting."
                        );
                        process::exit(1);
                    }
                });

            if let Ok(file_content) = fs::read_to_string(constants::OS_RELEASE) {
                for line in file_content.lines() {
                    let detail = line.trim();
                    if detail.starts_with("NAME=") {
                        _name = detail
                            .strip_prefix("NAME=")
                            .unwrap()
                            .to_string()
                            .replace('"', "");
                    }
                    if detail.starts_with("VERSION_ID=") {
                        _version_id = detail
                            .strip_prefix("VERSION_ID=")
                            .unwrap()
                            .to_string()
                            .replace('"', "");
                    }
                }
                partinfo.activate_is_os();
            } else {
                error!("Error reading os-release file. ALAR is not able to proceed. Exiting.");
                process::exit(1);
            }

            if mount::umount(constants::ASSERT_PATH, true).is_err() {
                error!("Error umounting rescue-rootlv. This may cause side effects. ALAR is not able to proceed. Exiting.");
                process::exit(1);
            }
            return Some(DistroNameVersion {
                name: _name,
                version_id: _version_id,
            });
        }
        None
    }

    fn is_fs_crypt_detected(partitions: &[PartInfo]) -> bool {
        partitions.iter().any(|part| part.fstype == "crypt?")
    }

    fn enable_ade(cli_info: &mut CliInfo, partition_details: &mut [PartInfo], distro: &mut Distro) {
        match ade::prepare_ade_environment(cli_info, partition_details).is_err() {
            true => {
                error!("Error preparing ADE environment. ALAR is not able to proceed. Exiting.");
                telemetry::send_envelope(&telemetry::create_exception_envelope(
                    telemetry::SeverityLevel::Error,
                    "ALAR EXCEPTION",
                    "Error preparing ADE environment.",
                    "Distro::enable_ade() -> ade::prepare_ade_environment() returned error",
                    cli_info,
                    distro,
                ))
                .ok();
                process::exit(1);
            }
            false => {
                // No error arm

                distro.set_is_ade(true);
                // if the crypt partition contains a LVM signature we need to import the volumegroup
                partition_details
                    .iter()
                    .filter(|x| x.fstype == "crypt?")
                    .for_each(|part| match part.part_type.as_str() {
                        "8E00" => match ade::ade_importvg() {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Error importing ADE VG: {e}");
                                process::exit(1);
                            }
                        },
                        "8300" => {}
                        _ => {
                            error!("Unknown partition type. ALAR is not able to proceed. Exiting.");
                            process::exit(1);
                        }
                    });
            }
        }
    }

    fn ade_prepare_lv(partition_details: &mut [PartInfo], distro: &mut Distro) {
        info!(
            "ADE is enabled. Collecting LV details from the ADE disk if an LVM signature is found."
        );
        let crypt_partition: &mut PartInfo = partition_details
            .iter_mut()
            .find(|part| part.fstype == "crypt?")
            .unwrap();

        // if the partition is not a LVM partition we don't need to proceed
        if crypt_partition.part_type != "8E00" {
            info!("No LVM partition found on the ADE disk.");
            return;
        } else {
            crypt_partition.fstype = "LVM2_member".to_string();
            distro.is_lvm = true;
        }

        let mut lv: Vec<LogicalVolume> = Vec::new();
        let mut lv_detail_string: String = String::with_capacity(64);
        match helper::run_fun(&format!(
            "lsblk -ln {} -o NAME,FSTYPE | sed '1d'",
            constants::ADE_OSENCRYPT_PATH
        )) {
            Ok(value) => {
                lv_detail_string = value;
            }
            Err(e) => {
                error!("Error getting LV details from ADE disk: {e}");
                process::exit(1);
            }
        };

        debug!(
            " ade_prepare_lv :: lv_detail_string: {:?}",
            &lv_detail_string
        );

        for line in lv_detail_string.lines() {
            let mut v: Vec<&str> = line.trim().split(' ').collect();
            v.retain(|&x| !x.is_empty());

            lv.push(LogicalVolume {
                name: v[0].to_string(),
                fstype: v[1].to_string(),
            });
        }
        crypt_partition.logical_volumes = LogicalVolumesType::Some(lv);

        debug!(
            "LV partition collected on the ADE eneabled disk/partition: {:#?}",
            &partition_details
        );
    }

    fn set_is_ade(&mut self, is_ade: bool) {
        self.is_ade = is_ade;
    }

    pub fn new(cli_info: &mut cli::CliInfo) -> Distro {
        let mut distro = Distro::default();
        let mut partition_details = Self::get_partition_details(cli_info);
        debug!(
            "Partition details of the disk to be recovered: {:?}",
            &partition_details
        );

        // at this point is is still not determined whether, if the fs_type is crypt, the disk needs to manually decrypted
        if Self::is_fs_crypt_detected(&partition_details) {
            /*
               The ADE disk gets decrypted and if we find an LVM signature we need to import the VG.
               Also, the LV on it get determined.
            */
            Self::enable_ade(cli_info, &mut partition_details, &mut distro);
            Self::ade_prepare_lv(&mut partition_details, &mut distro);
        } else {
            /*
               No encrypted disk got detected.
               It is also required to determine the LVs on the disk.
            */
            Self::build_logical_volume_details(&mut partition_details, cli_info, &mut distro);
        }

        let distro_name = match Self::what_distro_name_version(
            &mut partition_details,
            cli_info,
            &mut distro,
        ) {
            Some(distro_name) => distro_name,
            None => {
                error!("No OS partition found.");
                error!("Please make sure the disk isn't a Data-disk.");
                error!("If you are sure the attached disk is an OS-Disk please report this at: https://github.com/Azure/ALAR/issues.");
                error!("ALAR isn't able to proceed. Exiting.");
                telemetry::send_envelope(&telemetry::create_exception_envelope(
                    telemetry::SeverityLevel::Error,
                    "ALAR EXCEPTION",
                    "No OS partition found during distro detection.",
                    "Distro::new() -> what_distro_name_version() returned None",
                    cli_info,
                    &distro,
                ))
                .ok();
                process::exit(1);
            }
        };
        // Correct the filesystem for a non LVM ADE disk
        Self::ade_set_no_lvm_partiton_fs(&mut partition_details);

        distro.partitions = partition_details;
        distro.distro_name_version = distro_name;
        distro.cli_info = cli_info.clone();

        distro
    }
}

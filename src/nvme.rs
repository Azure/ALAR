// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the terms found in the LICENSE file in the root of this source tree.

use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use chrono::{DateTime, Duration, Utc};
use log::debug;
use regex::Regex;
use serde::Deserialize;
use anyhow::{Context, Result, ensure};
use anyhow::anyhow;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceVmCheck {
    overall_status: String,
    target_controller_type: String,
    vm_resource_id: String,
    checks: EligibilityChecks,
    nvme_eligibility: NvmeEligibility,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EligibilityChecks {
    ade_check: AdeCheck,
}

#[derive(Deserialize)]
struct AdeCheck {
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NvmeEligibility {
    schema_version: u8,
    source_vm_id: String,
    checked_at: String,
    os_disk: DiskEligibility,
    data_disks: Vec<DiskEligibility>,
    data_disk_count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiskEligibility {
    id: String,
    ade_enabled: bool,
}

pub(crate) fn is_requested(actions: &str) -> bool {
    actions.split(',').any(|action| action.trim().eq_ignore_ascii_case("nvme"))
}

pub(crate) fn ensure_not_ade(actions: &str, encrypted_or_unknown: bool) -> Result<()> {
    ensure!(
        !is_requested(actions) || !encrypted_or_unknown,
        "NVMe preparation is unsupported with ADE or unidentified encrypted partitions. \
         Unlocking a disk does not remove ADE."
    );
    Ok(())
}

fn is_compute_resource_id(value: &str, resource_type: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 9
        && parts[0].is_empty()
        && parts[1].eq_ignore_ascii_case("subscriptions")
        && !parts[2].is_empty()
        && parts[3].eq_ignore_ascii_case("resourceGroups")
        && !parts[4].is_empty()
        && parts[5].eq_ignore_ascii_case("providers")
        && parts[6].eq_ignore_ascii_case("Microsoft.Compute")
        && parts[7].eq_ignore_ascii_case(resource_type)
        && !parts[8].is_empty()
}

// The trusted repair workflow supplies this report for the original VM, not the rescue VM.
pub(crate) fn validate_eligibility_report(contents: &str, now: DateTime<Utc>) -> Result<()> {
    let report: SourceVmCheck = serde_json::from_str(contents).map_err(|error| {
        anyhow!("Invalid or incomplete NVMe eligibility JSON at line {}, column {}",
                error.line(), error.column())
    })?;
    let eligibility = &report.nvme_eligibility;
    ensure!(
        report.overall_status == "passed"
            && report.target_controller_type == "NVMe"
            && report.checks.ade_check.status == "passed"
            && eligibility.schema_version == 1,
        "The source-VM report does not establish NVMe ADE eligibility."
    );
    ensure!(
        is_compute_resource_id(&report.vm_resource_id, "virtualMachines")
            && report.vm_resource_id.eq_ignore_ascii_case(&eligibility.source_vm_id),
        "The eligibility report has an invalid or inconsistent source VM identity."
    );
    let checked_at = DateTime::parse_from_rfc3339(&eligibility.checked_at)
        .context("The eligibility report has an invalid check timestamp")?;
    let age = now.signed_duration_since(checked_at.with_timezone(&Utc));
    ensure!(
        age >= Duration::seconds(-60) && age <= Duration::minutes(15),
        "The eligibility report is expired or future-dated; repeat the source-VM check immediately before ALAR."
    );
    ensure!(
        eligibility.data_disk_count == eligibility.data_disks.len(),
        "The eligibility report has an incomplete data-disk inventory."
    );
    let mut ids = HashSet::new();
    for disk in std::iter::once(&eligibility.os_disk).chain(&eligibility.data_disks) {
        ensure!(
            !disk.ade_enabled && is_compute_resource_id(&disk.id, "disks"),
            "ADE is enabled or a managed-disk identity is invalid; NVMe is refused."
        );
        ensure!(ids.insert(disk.id.to_ascii_lowercase()), "Duplicate eligibility disk identity.");
    }
    Ok(())
}

pub(crate) fn validate_source_eligibility(actions: &str) -> Result<()> {
    if !is_requested(actions) {
        return Ok(());
    }
    let path = env::var("ALAR_NVME_ELIGIBILITY_FILE").context(
        "NVMe requires ALAR_NVME_ELIGIBILITY_FILE: a fresh source-VM check covering OS and data disks"
    )?;
    let file = File::open(&path).context("Cannot open the NVMe eligibility report")?;
    let metadata = file.metadata().context("Cannot inspect the NVMe eligibility report")?;
    ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o022 == 0,
        "The NVMe eligibility report must be a root-owned regular file, not writable by group or others."
    );
    let mut contents = String::new();
    file.take(65_537).read_to_string(&mut contents)
        .context("Cannot read the NVMe eligibility report")?;
    ensure!(contents.len() <= 65_536, "The NVMe eligibility report exceeds 64 KiB.");
    validate_eligibility_report(&contents, Utc::now())
}

#[derive(Debug)]
pub(crate) struct NvmeController {
    pub name: String,
    pub model: String,
    pub disks: Vec<String>,
}


// A global variable (protected by OnceLock), required to identify the nvme disk to be recovered
static NVME_DISK_PATH_CELL : OnceLock<Result<String>> = OnceLock::new();

fn read_nvme_controllers() -> Result<Vec<NvmeController>> {
    let class_nvme = Path::new("/sys/class/nvme/nvme*").try_exists();
    let mut controllers = Vec::new();
    
    // Check if the parent directory exists
    match class_nvme {
        Ok(_) => { } ,
        Err(e) => {
            println!("Failed to check existence of /sys/class/nvme: {}", e);
            return Ok(controllers);
        }
    }
    
    // Iterate through entries in the parent directory (controllers like nvme0, nvme1)
    for entry in fs::read_dir("/sys/class/nvme")? {
        let entry = entry?;
        let path = entry.path();
        
        // Only process directories (e.g., nvme0, nvme1, etc.)
        if path.is_dir() {
            let controller_name = entry.file_name().to_string_lossy().to_string();
            
            // Read the model file from the controller directory
            let model_file = path.join("model");
            let model = if model_file.exists() {
                match fs::read_to_string(&model_file) {
                    Ok(content) => content.trim().to_string(),
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", model_file.display(), e);
                        String::from("Unknown")
                    }
                }
            } else {
                String::from("Unknown")
            };
            
            // Find all disk directories (subdirectories starting with 'nvme')
            let mut disks = Vec::new();
            if let Ok(controller_entries) = fs::read_dir(&path) {
                for disk_entry in controller_entries.filter_map(Result::ok) {
                    let disk_name = disk_entry.file_name().to_string_lossy().to_string();
                    // Check if it's a directory and starts with 'nvme'
                    if disk_entry.path().is_dir() && disk_name.starts_with("nvme") {
                        disks.push(disk_name);
                    }
                }
            }
            
            // Sort disks for consistent ordering
            disks.sort();
            
            controllers.push(NvmeController {
                name: controller_name,
                model,
                disks,
            });
        }
    }
    
    // Sort controllers by name
    controllers.sort_by(|a, b| a.name.cmp(&b.name));
    
    Ok(controllers)
}

fn get_nvme_mounts() -> Result<String> {
    let mount_details = fs::read_to_string("/proc/mounts")?;
    let nmve_regex = r"/dev/nvme.*/boot/efi.*";
    let nvme_regex_compiled = Regex::new(nmve_regex).unwrap();
    let mut result = String::new();
    for cap in nvme_regex_compiled.captures_iter(mount_details.as_str()) {
        if let Some(matched) = cap.get(0) {
            result = matched.as_str().to_string();
        }
    }
    Ok(result.split(" ").next().unwrap_or("").to_string())
}

pub(crate) fn get_recovery_nvme_disk_path() -> &'static anyhow::Result<String> {
    // We use OnceLock to ensure that the NVMe disk path is computed only once and cached for subsequent calls.
    // Otherwise we have runtime problems even in a single-threaded context, because after the recovery disk got mounted it does interfere with the logic to determine 
    // what namespaces belong to the local disk and which one belongs to the recovery disk. 
    NVME_DISK_PATH_CELL.get_or_init(|| {

    let mut controllers = read_nvme_controllers()?;
    controllers.retain(|controller| 
         !controller.model.contains("Microsoft NVMe Direct Disk") 
    );
    debug!("Found NVMe controllers: {:?}", controllers);

    let mut nvme_disk_mount = get_nvme_mounts()?;
    debug!("Found NVMe disk mount: {}", nvme_disk_mount);
    nvme_disk_mount = nvme_disk_mount.split_off(5 ); // Remove '/dev/' prefix
    let _ = nvme_disk_mount.split_off(7 ); // Remove partition suffix
    
    //We have two controllers available on a VM. One for the remote disks and one for the local disk
    //The first we had stripped off already above.Though we still operate on a Vector of controllers. 
    controllers[0].disks.retain(|disk| disk != &nvme_disk_mount); // remove the OS disk

    if controllers[0].disks.len() > 1 {
        return Err(anyhow!("More than one recovery disk found: {:?}\nPlease use the option '--custom-recover-disk' to pass over the right recovery disk", controllers[0].disks));
    }    
       Ok(format!("/dev/{}", controllers[0].disks[0]))
}
    )
}

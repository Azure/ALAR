// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the terms found in the LICENSE file in the root of this source tree.

use std::fs;
use std::path::Path;
use log::debug;
use regex::Regex;
use anyhow::Result;
use anyhow::anyhow;
use std::sync::OnceLock;

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

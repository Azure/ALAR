use crate::{
    ade,
    cli::CliInfo,
    constants,
    distro::{Distro, LogicalVolumesType},
    mount,
};
use anyhow::{anyhow, Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{process, process::Command};

// There are issue with readlink or readpath. Somehow the pathes can't be resolved correctly
// The following functions are a workaround to get the correct path and the detemine the partition numbers
// based on those detalis we can get the partition path
pub(crate) fn realpath(path: &str) -> Result<String> {
    let device = Command::new("readlink")
        .arg("-fe")
        .arg(path)
        .output()?
        .stdout;
    if device.is_empty() {
        return Err(anyhow!("Failed to get the real path of {}", path));
    }
    Ok(String::from_utf8(device)?.trim().to_string())
}

pub(crate) fn get_recovery_disk_path(cli_info: &CliInfo) -> String {
    let mut path_info = String::new();
    let error_condition = |e| {
        eprintln!("Error getting recover disk info. Something went wrong. ALAR is not able to proceed. Exiting.");
        eprint!("Error detail: {}", e);
        process::exit(1);
    };

    if !cli_info.custom_recover_disk.is_empty() {
        match realpath(&cli_info.custom_recover_disk) {
            Ok(path) => {
                path_info = path;
            }
            Err(e) => error_condition(e),
        }
    } else {
        match realpath(constants::RESCUE_DISK) {
            Ok(path) => {
                path_info = path;
            }
            Err(e) => error_condition(e),
        }
    };
    path_info
}

pub(crate) fn is_repair_vm_imds() -> Result<bool> {
    #[derive(Serialize, Deserialize, Debug)]
    struct Tags {
        name: String,
    }
    let mut is_repair_vm = false;
    let client = reqwest::blocking::Client::new();

    let data = client
        .get("http://169.254.169.254/metadata/instance/compute/?api-version=2021-02-01")
        .header("Metadata", "true")
        .send()?
        .text()?;
    let data: Value = serde_json::from_str(&data)?;
    let data = data["tagsList"]
        .as_array()
        .ok_or(anyhow!("Array extraction not possible"))?;

    for tags in data {
        if serde_json::from_value::<Tags>(tags.to_owned())?
            .name
            .contains("repair_source")
        {
            is_repair_vm = true;
        }
    }

    Ok(is_repair_vm)
}

pub(crate) fn cleanup(distro: Distro, cli_info: &CliInfo) -> Result<()> {
    if distro.is_ade {
        debug!("Running ADE cleanup");
        if distro
            .partitions
            .iter()
            .filter(|lvm| matches!(lvm.logical_volumes, LogicalVolumesType::Some(_)))
            .count()
            > 0
        {
            info!("LVM clean up at the end of the recovery process.");
            match ade::ade_lvm_cleanup() {
                Ok(_) => {}
                Err(e) => {
                    error!("Clean up phase :: ade_cleanup raised an error : {e}");
                }
            };
        } else {
            info!("Clean up at the end of the recovery process.");
            ade::close_rescueencrypt()?;
        }
    } else {
        distro
            .partitions
            .iter()
            .filter(|lvm| matches!(lvm.logical_volumes, LogicalVolumesType::Some(_)))
            .for_each(|_| {
                info!("Cleaning up at the end of the recovery process.");
                match mount::rename_rootvg() {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Clean up phase :: rename_rootvg raised an error: {e}");
                    }
                }

                match mount::disable_broken_disk(cli_info) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Clean up phase :: disable_broken_disk raised and error : {e}");
                    }
                }
                mount::rename_oldvg();

                match mount::rescan_host() {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Clean up phase :: rescan_host raised an error : {e}");
                    }
                }
            });
    }
    Ok(())
}

pub(crate) fn run_fun(command: &str) -> Result<String> {
    debug!("Running command: {}", command);
    let output = Command::new("bash").arg("-c").arg(command).output()?.stdout;
    Ok(String::from_utf8(output)?)
}

pub(crate) fn run_cmd(command: &str) -> Result<()> {
    let output = Command::new("sh").arg("-c").arg(command).output()?;
    if !output.status.success() {
        return Err(anyhow!("Unable to run command {}", command));
    }
    Ok(())
}

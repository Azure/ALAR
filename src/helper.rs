use crate::{
    ade,
    cli::{self, CliInfo},
    constants,
    distro::{Distro, LogicalVolumesType},
    mount, telemetry,
};
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{self, Command},
    time::Duration,
};

// There are issue with readlink or readpath. Somehow the pathes can't be resolved correctly
// The following functions are a workaround to get the correct path and to determine the partition numbers
// based on those details we can get from the partition path.
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
        error!("Error getting recover disk info. Something went wrong. ALAR is not able to proceed. Exiting.");
        error!("Error detail: {}", e);

        match telemetry::send_envelope(&telemetry::create_exception_envelope(
            telemetry::SeverityLevel::Error,
            "ALAR EXCEPTION",
            "Error getting recovery disk partition information.",
            &format!(
                "Distro::get_all_recovery_partitions() -> helper::run_fun() returned error: {}",
                e
            ),
            cli_info,
            &Distro::default(),
        )) {
            Ok(_) => {}
            Err(e) => debug!("Failed to send telemetry data: {}", e),
        }
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
        .timeout(Duration::from_secs(4))
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

pub(crate) fn cleanup(distro: &Distro, cli_info: &CliInfo) -> Result<()> {
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
    debug!("Running function: {}", command);
    let output = Command::new("bash").arg("-c").arg(command).output()?.stdout;
    Ok(String::from_utf8(output)?)
}

pub(crate) fn run_cmd(command: &str) -> Result<()> {
    debug!("Running command: {}", command);
    let output = Command::new("sh").arg("-c").arg(command).output()?;
    if !output.status.success() {
        return Err(anyhow!("Unable to run command {}", command));
    }
    Ok(())
}

pub(crate) fn is_root_user() -> Result<bool> {
    let id_value = run_fun("id -u")?;
    Ok(id_value.trim() == "0")
}

pub(crate) fn download_action_scripts_or(cli_info: &cli::CliInfo) -> Result<()> {
    if cli_info.download_action_scripts {
        download_action_scripts()
    } else if !cli_info.local_action_directory.is_empty() {
        load_local_action_scripts(&cli_info.local_action_directory)
    } else {
        //No remote actions nor local actions are requested. We will use the builtin actions
        write_builtin_action_scripts()?;
        Ok(())
    }
}

fn download_action_scripts() -> Result<()> {
    // At first clean
    if Path::new(constants::ACTION_IMPL_DIR).exists() {
        if let Err(err) = fs::remove_dir_all(constants::ACTION_IMPL_DIR) {
            println!(
                "Directory {} can not be removed : '{}'",
                constants::ACTION_IMPL_DIR,
                err
            );
        }
    }
    debug!("Downloading the action scripts from the remote repository");
    let command = format!("curl -o /tmp/alar2.tar.gz -L {}", constants::TARBALL);
    run_cmd(&command).context("Archive alar2.tar.gz not downloaded")?;
    debug!("Downloaded the action scripts from the remote repository");
    // Expand the action_implementation directory
    run_cmd(
        "tar --wildcards --strip-component=2 -xzf /tmp/alar2.tar.gz -C /tmp *action_implementation",
    )?;

    Ok(())
}

fn load_local_action_scripts(directory_source: &str) -> Result<()> {
    if !Path::new(directory_source).exists() {
        return Err(anyhow!("Directory {} does not exist", directory_source));
    }

    if Path::new(constants::ACTION_IMPL_DIR).exists() {
        fs::remove_dir_all(constants::ACTION_IMPL_DIR)
            .context("Directory ACTION_IMPL_DIR can not be removed")?;
    }
    let mut options = fs_extra::dir::CopyOptions::new();
    options.skip_exist = true;
    options.copy_inside = true;
    fs_extra::dir::copy(directory_source, constants::ACTION_IMPL_DIR, &options)
        .context("Copying the content of the script directory to '/tmp' failed")?;
    Ok(())
}

fn write_builtin_action_scripts() -> Result<()> {
    fs::create_dir_all(constants::ACTION_IMPL_DIR)
        .context("Directory ACTION_IMPL_DIR can not be created")?;

    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "auditd-impl.sh"),
        constants::AUDITD_IMPL_FILE,
    )
    .context("Writing auditd-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "efifix-impl.sh"),
        constants::EFIFIX_IMPL_FILE,
    )
    .context("Writing efifix-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "fstab-impl.sh"),
        constants::FSTAB_IMPL_FILE,
    )
    .context("Writing fstab-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "grub.awk"),
        constants::GRUB_AKW_FILE,
    )
    .context("Writing grub.awk failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "grubfix-impl.sh"),
        constants::GRUBFIX_IMPL_FILE,
    )
    .context("Writing grubfix-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "helpers.sh"),
        constants::HELPERS_SH_FILE,
    )
    .context("Writing helpers.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "initrd-impl.sh"),
        constants::INITRD_IMPL_FILE,
    )
    .context("Writing initrd-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "kernel-impl.sh"),
        constants::KERNEL_IMPL_FILE,
    )
    .context("Writing kernel-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "safe-exit.sh"),
        constants::SAFE_EXIT_FILE,
    )
    .context("Writing safe-exit.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "serialconsole-impl.sh"),
        constants::SERIALCONSOLE_IMPL_FILE,
    )
    .context("Writing serialconsole-impl.sh failed")?;
    fs::write(
        format!("{}/{}", constants::ACTION_IMPL_DIR, "test-impl.sh"),
        constants::TEST_IMPL_FILE,
    )
    .context("Writing test-impl.sh failed")?;

    Ok(())
}

pub(crate) fn get_repair_os_name() -> Result<String> {
    let os_release = fs::read_to_string("/etc/os-release")
        .context("Unable to read /etc/os-release to determine the OS name")?;
    for line in os_release.lines() {
        if line.starts_with("PRETTY_NAME=") {
            let os_name_version = line.trim_start_matches("PRETTY_NAME=").trim_matches('"');
            return Ok(os_name_version.to_string());
        }
    }
    Err(anyhow!(
        "Unable to determine the OS name a from /etc/os-release"
    ))
}

pub(crate) fn get_repair_os_version() -> Result<String> {
    let os_release = fs::read_to_string("/etc/os-release")
        .context("Unable to read /etc/os-release to determine the OS version")?;
    for line in os_release.lines() {
        if line.starts_with("VERSION_ID=") {
            let os_version = line.trim_start_matches("VERSION_ID=").trim_matches('"');
            return Ok(os_version.to_string());
        }
    }
    Err(anyhow!(
        "Unable to determine the OS version from /etc/os-release"
    ))
}

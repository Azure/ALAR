use crate::{constants, distro, helper,};
use anyhow::{Result,};
use log::{debug};
use std::io::Write;
use std::{env, fs, io, process};

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
        s if s.contains("SUSE") => distro::DistroKind {
            distro_type: distro::DistroType::Suse,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("azurelinux") => distro::DistroKind {
            distro_type: distro::DistroType::AzureLinux,
            distro_subtype: distro::DistroSubType::None,
        },
        s if s.contains("mariner") => distro::DistroKind {
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

pub fn set_environment(distro: &distro::Distro) {
    let distroname = &distro.distro_name_version.name;
    let distroversion = &distro.distro_name_version.version_id;
    let distrokind = get_distro_kind(distro);
    debug!("Distro kind: {:?}", distrokind);

    // some default values which can be always of help
    env::set_var("DISTRONAME", distroname.as_str());
    env::set_var("DISTROVERSION", distroversion.as_str());
    env::set_var("isLVM", convert_bool(distro.is_lvm));

    // Remove this variable becasue of security reasons
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
        _ => {
            env::set_var("DISTROTYPE", "UNKNOWN");
            env::set_var("DISTROSUBTYPE", "UNKNOWN");
        }

    }
}

// TODO requires validation on any supported Linux distro (endorsed distros)
// If TMUX isn't available install it. If installation isn't possible make the user aware that tmux is required if the action 'chroot-cli' is selected
pub(crate) fn is_tmux_installed() -> Result<bool> {
    match helper::run_fun("tmux -V") {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub(crate) fn execute_chroot_cli() -> Result<()> {
    debug!("Executing chroot-cli");

    match env::set_current_dir(constants::RESCUE_ROOT) {
        Ok(_) => {}
        Err(e) => println!("Error in set current dir : {e}"),
    }

    // create a TMUX session which is used while one works directly in the chroot environment
    process::Command::new("tmux")
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg("rescue")
        .arg("chroot")
        .arg(constants::RESCUE_ROOT)
        .arg("/bin/bash")
        .spawn()?
        .wait()?;

    // Need to prepare the environment to make chroot a bit more safer
    process::Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg("rescue")
        .arg(format!(". {}/safe-exit.sh", constants::ACTION_IMPL_DIR))
        .arg("Enter")
        .spawn()?
        .wait()?;

    process::Command::new("tmux")
        .arg("attach")
        .arg("-t")
        .arg("rescue")
        .spawn()?
        .wait()?;

    // Get out of constants::RESCUE_ROOT, otherwise umount isn't possible for RESCUE_ROOT
    match env::set_current_dir("/") {
        Ok(_) => {}
        Err(e) => println!("Error in set current dir : {e}"),
    }

    Ok(())
}

pub(crate) fn run_repair_script(action_name: &str) -> Result<()> {
    match env::set_current_dir(constants::RESCUE_ROOT) {
        Ok(_) => {}
        Err(e) => println!("Error in set current dir : {e}"),
    }

    let file_name = format!("{}/{}-impl.sh", constants::ACTION_IMPL_DIR, action_name);
    let command = format!("chmod 500 {}", file_name);
    helper::run_cmd(&command)?;

    debug!("Running repair script for action: {action_name}");
    let output = process::Command::new("chroot")
        .arg(constants::RESCUE_ROOT)
        .arg("/bin/bash")
        .arg("-c")
        .arg(format!(
            "{}/{}-impl.sh",
            constants::ACTION_IMPL_DIR,
            action_name
        ))
        .output()?;

    println!("Output generated by the selected action: ");
    io::stdout().write_all(&output.stdout).unwrap();

    // Get out of constants::RESCUE_ROOT, otherwise umount isn't possible for RESCUE_ROOT
    match env::set_current_dir("/") {
        Ok(_) => {}
        Err(e) => println!("Error in set current dir : {e}"),
    }

    Ok(())
}

pub(crate) fn is_action_available(action_name: &str) -> io::Result<bool> {
    let action_name = action_name.to_lowercase();
    if action_name == constants::CHROOT_CLI {
        return Ok(true);
    }

    let dircontent = fs::read_dir(constants::ACTION_IMPL_DIR)?;
    let mut actions: Vec<String> = Vec::new();
    for item in dircontent {
        let detail = format!("{}", item?.path().display());
        actions.push(detail);
    }

    Ok(actions
        .iter()
        .any(|a| a.ends_with(&format!("{action_name}-impl.sh"))))
}

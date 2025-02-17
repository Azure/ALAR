use crate::{cli, constants, distro, helper};
use anyhow::Result;
use log::debug;
use std::io::Write;
use std::{env, fs, io, process};

// TODO requires validation on any supported Linux distro (endorsed distros)
// If TMUX isn't available install it. If installation isn't possible make the user aware that tmux is required if the action 'chroot-cli' is selected
pub(crate) fn is_tmux_installed() -> Result<bool> {
    match helper::run_fun("tmux -V") {
        Ok(value) => { if value.is_empty() { Ok(false) } else { Ok(true) } },
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

pub(crate) fn is_action_available(action_name: &str) -> Result<bool> {
    let action_name = action_name.to_lowercase();
    if action_name == constants::CHROOT_CLI {
        return Ok(true);
    }

    let dircontent = fs::read_dir(constants::ACTION_IMPL_DIR)?;
    for item in dircontent {
        let dir_item = item?.path().display().to_string();
        let dir_item = dir_item.strip_prefix(&format!("{}/", constants::ACTION_IMPL_DIR)).unwrap_or_default().to_string();
        if dir_item == format!("{action_name}-impl.sh") {
            return Ok(true);
        }
    }
    Ok(false)
}

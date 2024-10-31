mod action;
mod ade;
mod cli;
mod constants;
mod distro;
mod helper;
mod mount;
mod prepare_chroot;
use anyhow::Result;
use log::{debug, error, info, log_enabled, Level};
use std::{env, process};

fn main() -> Result<()> {
    //Initialize the logger
    env_logger::init();

    // First verify we have the right amount of information to operate
    let mut cli_info = cli::cli()?;

    // are we root?
    if !helper::is_root_user()? {
        error!("ALAR must be executed as root. Exiting.");
        process::exit(1);
    }

    let arguments: Vec<_> = env::args().collect();
    if log_enabled!(Level::Debug) {
        debug!("Arguments passed to ALAR: ");
        arguments.iter().for_each(|arg| debug!("{arg}"));
    }

    // Create a new distro object
    // The distro object will be used to determine the distro of the VM we are trying to recover
    let distro = distro::Distro::new(&mut cli_info);
    info!("Distro details collected : {:#?}", distro);

    /*
    After we have collected all the required information we can start the actuall recover process.
    If we have finished the recovery process it is important to reame the VG 'oldvg' back to 'rootvg'.
    Otherwise the recovery VM might not boot up correctly

    */

    // download_action_scripts_or will download the action scripts from GIT if explicitly requested or utilize a custom script if available,
    // otherwise the builtin ones will be used.
    match helper::download_action_scripts_or(&cli_info) {
        Ok(_) => {}
        Err(e) => {
            error!("An issue with the action scripts happend: {}", e);
            helper::cleanup(&distro, &cli_info)?;
            process::exit(1);
        }
    }

    // Let us verify whether the action to be executed is available
    for action in cli_info.actions.split(',') {
        if !action::is_action_available(action)? {
            error!("The action {action} is not available. Exiting.");
            helper::cleanup(&distro, &cli_info)?;
            process::exit(1);
        }
    }

    // Prepare and setup the environment for the recovery process
    if prepare_chroot::prepare_chroot(&distro, &cli_info).is_err() {
        error!("Failed to prepare the chroot environment. Exiting.");
        mount::umount(constants::RESCUE_ROOT, true)?;
        helper::cleanup(&distro, &cli_info)?;
        process::exit(1);
    }
    action::set_environment(&distro);

    // Run the repair scripts
    if cli_info.actions.contains(constants::CHROOT_CLI) {

        match action::is_tmux_installed() {
            Ok(true) => {
                action::execute_chroot_cli()?;
            }
            Ok(false) => {
                error!("tmux is not installed. Please install it manually. tmux is required if action 'chroot-cli' is selected");
                mount::umount(constants::RESCUE_ROOT, true)?;
                helper::cleanup(&distro, &cli_info)?;
                process::exit(1);
            }
            Err(e) => {
                error!("An issue with the action scripts happend: {}", e);
                mount::umount(constants::RESCUE_ROOT, true)?;
                helper::cleanup(&distro, &cli_info)?;
                process::exit(1);
            }
        }
    } else {
        for action_name in cli_info.actions.split(',') {
            action::run_repair_script(action_name.trim())?;
        }
    }

    // Umount and cleanup the resources
    mount::umount(constants::RESCUE_ROOT, true)?;
    helper::cleanup(&distro, &cli_info)?;

    Ok(())
}

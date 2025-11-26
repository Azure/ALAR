mod action;
mod ade;
mod cli;
mod constants;
mod distro;
mod global;
mod helper;
mod mount;
mod prepare_chroot;
mod telemetry;
mod nvme;
use anyhow::Result;
use env_logger::Env;
use log::{debug, error, info, log_enabled, Level};
use std::{env, process};

fn main() -> Result<()> {
    //Initialize the logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // First verify we have the right amount of information to operate
    let mut cli_info = cli::cli()?;

    // are we root?
    if !helper::is_root_user()? {
        error!("ALAR must be executed as root. Exiting.");
        process::exit(1);
    }

    if log_enabled!(Level::Debug) {
        let arguments: Vec<_> = env::args().collect();
        debug!("Arguments passed to ALAR: ");
        arguments.iter().for_each(|arg| debug!("{arg}"));
    }

    // Create a new distro object
    // The distro object will be used to determine the distro of the VM we are trying to recover
    let distro = distro::Distro::new(&mut cli_info);
    info!("Distro details collected : {:#?}", distro);

    // After we have collected all the required information we can start the actuall recover process.
    // If we have finished the recovery process it is important to rename the VG 'oldvg' back to 'rootvg'.
    // Otherwise the recovery VM might not boot up correctly


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
            telemetry::send_envelope(&telemetry::create_exception_envelope(
                telemetry::SeverityLevel::Warning,
                "ActionNotFound",
                &format!("The action {action} is not available"),
                "",
                &cli_info,
                &distro,
            ))?;
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
                error!("A tmux or action script error happened: {}", e);
                mount::umount(constants::RESCUE_ROOT, true)?;
                helper::cleanup(&distro, &cli_info)?;
                process::exit(1);
            }
        }
    } else {
        for action_name in cli_info.actions.split(',') {
            debug!("Running action script: {}", action_name.trim());
            action::run_repair_script(action_name.trim())?;
        }
    }

    // Finally send telemetry information
    let trace_message = telemetry::create_trace_envelope(
        telemetry::SeverityLevel::Information,
        "Recovery action(s) completed",
        &cli_info,
        &distro,
    );
    telemetry::send_envelope(&trace_message)?;

    // Umount and cleanup the resources
    mount::umount(constants::RESCUE_ROOT, true)?;
    helper::cleanup(&distro, &cli_info)?;

    Ok(())
}

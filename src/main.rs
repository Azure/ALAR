mod ade;
mod cli;
mod constants;
mod distro;
mod helper;
mod mount;
use anyhow::Result;
use log::{debug, error, log_enabled, Level};
use std::{env, process};

fn main() -> Result<()> {
    //Initialize the logger
    env_logger::init();

    // First verify we have the right amount of information to operate
    let mut cli_info = cli::cli();

    // are we root?
    if !helper::is_root_user()? {
        error!("ALAR must be run as root. Exiting.");
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
    debug!("Distro details collected : {:#?}", distro);

    /*
    After we have collected all the required information we can start the actuall recover process.
    If we have finished the recovery process it is important to reame the VG 'oldvg' back to 'rootvg'.
    Otherwise the recovery VM might not boot up correctly

     */
    helper::cleanup(distro, &cli_info)?;

    Ok(())
}

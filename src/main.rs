mod action;
mod ade;
mod cli;
mod constants;
mod distro;
mod helper;
mod mount;
mod prepare_action;
mod redhat;
mod standalone;
mod suse;
mod ubuntu;
use std::process;

fn main() {
    // First verify we have the right amount of information to operate
    let cli_info = cli::cli();

    // are we root?
    let euid = unsafe { nc::geteuid() };
    if euid != 0 {
        println!("Please run alar as root");
        process::exit(1);
    }

    // Verify the distro we have to work with
    // the Distro struct does contain then all of the required information
    let distro = distro::Distro::new();
    eprintln!("{distro:?}");

    // Do we have a valid distro or not?
    if distro.kind == distro::DistroKind::Undefined {
        helper::log_error("Unrecognized Linux distribution. The ALAR tool isn't able to recover it\n
                 The OS distros supported are:\n
                 CentOS/Redhat 6.8 - 9.x\n
                 Ubuntu 16.4 LTS, 18 LTS, 20.04 LTS\n
                 Suse 12 and 15\n
                 Debain 9, 10, 11\n
                 ALAR will stop!\n
                 If your OS is in the above list please report this issue at https://github.com/Azure/ALAR/issues"
        );
        process::exit(1);
    }

    // Prepare and mount the partitions. Take into account what distro we have to deal with
    match mount::mkdir_rescue_root() {
        Ok(_) => {}
        Err(e) => panic!("The rescue-root dir can't be created. This is not recoverable! : {e} "),
    }

    // Step 2 of prepare and mount. Mount the right dirs depending on the distro determined
    prepare_action::distro_mount(&distro);

    // Get the actions
    if let Err(e) = standalone::download_action_scripts(&cli::cli()) {
        prepare_action::distro_umount(&distro);
        panic!("action scripts are not able to be copied or downloadable : '{e}'");
    }

    // Verify we have an implementation available for the action to be executed
    // Define a variable for the error condition that may happen
    let mut is_action_error = false;
    for action_name in cli_info.actions.split(',') {
        match action::is_action_available(action_name) {
            // Do the action
            Ok(_is @ true) => match action::run_repair_script(&distro, action_name) {
                Ok(_) => is_action_error = false,
                Err(e) => {
                    helper::log_error(
                        format!("Action {} raised an error: '{}'", &action_name, e).as_str(),
                    );
                    is_action_error = true;
                }
            },
            Ok(_is @ false) => {
                helper::log_error(format!("Action '{action_name}' is not available").as_str());
                is_action_error = true;
            }
            Err(e) => {
                helper::log_error(
                    format!("There was an error raised while verifying the action: '{e}'").as_str(),
                );
                is_action_error = true;
            }
        }
    }

    // Umount everything again

    prepare_action::distro_umount(&distro);

    // Inform the calling process about the success
    if is_action_error {
        process::exit(1);
    } else {
        process::exit(0);
    }
}

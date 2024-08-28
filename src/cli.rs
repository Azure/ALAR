
use std::os::unix;

use crate::helper;
use clap::{App, Arg};
use log::debug;

// The Initiator type is used to determine the context in which ALAR is running
// This information is required to be used later in a telemetry module TODO
#[derive(Debug, Default, Clone)]
pub(crate) enum Initiator {
    RecoverVm,
    SelfHelp,
    #[default]
    Cli,
}
#[derive(Default, Debug, Clone)]
pub(crate) struct CliInfo {
    pub(crate) action_directory: String,
    pub(crate) actions: String,
    pub(crate) initiator: Initiator,
    pub(crate) custom_recover_disk: String,
    pub(crate) ade_password: String,
}
impl CliInfo {
    pub(crate) fn new() -> CliInfo {
        CliInfo::default()
    }

    pub(crate) fn clear_password(&mut self) {
        debug!("Clearing ADE password");
        self.ade_password.clear();
        self.ade_password = "XXXXXXXX".to_string();
    }
}

pub(crate) fn cli() -> CliInfo {
    let about = "
ALAR tries to assist with non boot able scenarios by running
one or more different actions in order to get a VM in a running state that allows
the administrator to further recover the VM after it is up, running and accessible again.
";
    let matches = App::new("Azure Linux Auto Recover")
        .version(clap::crate_version!())
        .author("Marcus Lachmanez , malachma@microsoft.com")
        .about(about)
        .arg(
            Arg::with_name("directory")
                .short('d')
                .long("directory")
                .takes_value(true)
                .help("The directory in which custom actions are defined"),
        )
        .arg(
            Arg::with_name("ACTION")
                .help("A required paramter that defines the action to be executed. Multiple actions can be seperated by a comma")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("selfhelp-initiator")
                .long("selfhelp-initiator")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("custom_recover_disk")
                .long("custom-recover-disk")
                .takes_value(true)
                .help("The path to the custom recovery disk"),
        )
        .arg(
            Arg::with_name("ade_password")
                .long("ade-password")
                .takes_value(true)
                .help("The password to decrypt the ADE encrypted disk"),
        )
        .get_matches();
    let mut cli_info = CliInfo::new();

    // we should be safe here to rely on clap and its verification, though let us fail back to a default value to avoid panics
    cli_info.actions = matches.value_of("ACTION").unwrap_or("fstab").to_string();

    // Here the default is intentionally set to an empty string as a default value.
    cli_info.action_directory = matches.value_of("directory").unwrap_or("").to_string();

    // We also set a defalt value for an empty string.
    cli_info.custom_recover_disk = matches
        .value_of("custom_recover_disk")
        .unwrap_or("")
        .to_string();

    cli_info.ade_password = matches.value_of("ade_password").unwrap_or("").to_string();

    cli_info.initiator = if matches.contains_id("selfhelp-initiator") {
        Initiator::SelfHelp
    } else {
        let ppid = unix::process::parent_id();
        let pprocess_name = if let Ok(ppid) = helper::run_fun(&format!("cat /proc/{ppid}/comm")) {
            ppid
        } else {
            "no name found".to_string()
        };
        debug!("pprocess_name is {pprocess_name}");

        match helper::is_repair_vm_imds() {
            Ok(true) => {
                if pprocess_name.contains("run-alar.sh") {
                    Initiator::RecoverVm
                } else {
                    Initiator::Cli
                }
            }
            Ok(false) => Initiator::Cli,
            Err(_) => Initiator::Cli,
        }
    };

    debug!("cli_info is {cli_info:#?}");
    cli_info
}

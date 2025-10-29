use crate::helper;
use anyhow::Result;
use clap::{ArgAction, Parser};
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
    pub(crate) local_action_directory: String,
    pub(crate) actions: String,
    pub(crate) initiator: Initiator,
    pub(crate) custom_recover_disk: String,
    pub(crate) ade_password: String,
    pub(crate) download_action_scripts: bool,
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

// Azure Linux Auto Recover
#[derive(Debug, Parser)]
#[command(
    name = "Azure Linux Auto Recover",
    version = clap::crate_version!(),
    author = "Marcus Lachmanez , malachma@microsoft.com",
    about = r#"
ALAR assists in recovering virtual machines from non-bootable states by executing one or more predefined actions.
Once the VM is restored to a bootable and accessible state, administrators can continue with further recovery or maintenance operations.
"#
)]
struct Cli {
    /// A required parameter that defines the action to be executed. Multiple actions can be separated by a comma
    #[arg(index = 1, value_name = "ACTION")]
    action: String,

    /// The directory in which custom actions are defined
    #[arg(short = 'd', long = "directory", value_name = "DIR")]
    directory: Option<String>,

    /// Use this flag to download the action scripts from GIT instead of the builtin ones
    #[arg(long = "download-action-scripts", action = ArgAction::SetTrue)]
    download_action_scripts: bool,


    /// Selfhelp initiator flag
    #[arg(long = "selfhelp-initiator", alias = "SELFHELP", action = ArgAction::SetTrue)]
    selfhelp_initiator: bool,


    /// The path to the custom recovery disk
    #[arg(long = "custom-recover-disk", value_name = "PATH")]
    custom_recover_disk: Option<String>,

    /// The password to decrypt the ADE encrypted disk (base64-encoded)
    #[arg(long = "ade-password", value_name = "PASSWORD")]
    ade_password: Option<String>,
}

pub(crate) fn cli() -> Result<CliInfo> {
    let args = Cli::parse();

    let mut cli_info = CliInfo::new();

    // we should be safe here to rely on clap and its verification, though let us fail back to a default value to avoid panics
    cli_info.actions = if args.action.trim().is_empty() {
        "fstab".to_string()
    } else {
        args.action
    };

    // Here the default is intentionally set to an empty string as a default value.
    cli_info.local_action_directory = args.directory.unwrap_or_default();

    // We also set a default value for an empty string.
    cli_info.custom_recover_disk = args.custom_recover_disk.unwrap_or_default();

    // If the encryption key is passed over manually we can be sure it is copied out of the key-vault
    // /the key-vault value is base64 encoded as well. Thus we need to decode it first to be able to use it to decrypt the disk.
    let decoded_bytes = simple_base64::decode(args.ade_password.as_deref().unwrap_or(""))?;
    cli_info.ade_password = String::from_utf8(decoded_bytes)?;

    cli_info.download_action_scripts = args.download_action_scripts;

    // selfhelp-initiator and initiator serve the same purpose, initiator is the parameter passed over from the Portal SelfHelp framework
    cli_info.initiator = if args.selfhelp_initiator {
        Initiator::SelfHelp
    } else {
        let pstree_text = helper::run_fun("pstree | grep run-command-ext")?;
        debug!("pstree information: {}", &pstree_text);

        match helper::is_repair_vm_imds() {
            Ok(true) => {
                if pstree_text.contains("run-command-ext") {
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
    Ok(cli_info)
}

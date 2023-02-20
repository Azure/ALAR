use crate::{constants, distro, helper};
use distro::DistroKind;
use std::io::Write;
use std::{env, fs, io, process};

pub(crate) fn run_repair_script(distro: &distro::Distro, action_name: &str) -> io::Result<()> {
    helper::log_info("----- Start action -----");

    match env::set_current_dir(constants::RESCUE_ROOT) {
        Ok(_) => {}
        Err(e) => println!("Error in set current dir : {e}"),
    }

    // Set the environment correct
    let convert_bool = |state: bool| -> String {
        if state {
            "true".to_string()
        } else {
            "false".to_string()
        }
    };
    match distro.kind {
        DistroKind::Debian | DistroKind::Ubuntu => {
            env::set_var("isUbuntu", "true");
            env::set_var("isADE", convert_bool(distro.is_ade));
            env::set_var("root_part_path", distro.rescue_root.root_part_path.as_str());
            env::set_var("efi_part_path", helper::get_efi_part_path(distro).as_str());
            env::set_var("boot_part_path", distro.boot_part.boot_part_path.as_str());
            env::remove_var("isSuse");
            env::remove_var("isRedHat");
            env::remove_var("isRedHat6");
        }
        DistroKind::Suse => {
            env::set_var("isSuse", "true");
            env::set_var("root_part_path", distro.rescue_root.root_part_path.as_str());
            env::set_var("efi_part_path", helper::get_efi_part_path(distro).as_str());
            env::set_var("boot_part_path", distro.boot_part.boot_part_path.as_str());
            env::remove_var("isUbuntu");
            env::remove_var("isRedHat");
            env::remove_var("isRedHat6");
        }
        DistroKind::RedHatCentOS => {
            env::set_var("isRedHat", "true");
            env::set_var("isADE", convert_bool(distro.is_ade));
            env::set_var("root_part_path", distro.rescue_root.root_part_path.as_str());
            env::set_var("efi_part_path", helper::get_efi_part_path(distro).as_str());
            env::set_var("boot_part_path", distro.boot_part.boot_part_path.as_str());
            match distro.is_lvm {
                true => env::set_var("isLVM", "true"),
                false => env::set_var("isLVM", "false"),
            }
            env::set_var("lvm_root_part", distro.lvm_details.lvm_root_part.as_str());
            env::set_var("lvm_usr_part", distro.lvm_details.lvm_usr_part.as_str());
            env::set_var("lvm_var_part", distro.lvm_details.lvm_var_part.as_str());
            env::remove_var("isUbuntu");
            env::remove_var("isSuse");
            env::remove_var("isRedHat6");
        }
        DistroKind::RedHatCentOS6 => {
            env::set_var("isRedHat", "true");
            env::set_var("isADE", convert_bool(distro.is_ade));
            env::set_var("isRedHat6", "true");
            env::set_var("root_part_path", distro.rescue_root.root_part_path.as_str());
            env::set_var("efi_part_path", helper::get_efi_part_path(distro).as_str());
            env::set_var("boot_part_path", distro.boot_part.boot_part_path.as_str());
            match distro.is_lvm {
                true => env::set_var("isLVM", "true"),
                false => env::set_var("isLVM", "false"),
            }
            env::set_var("lvm_root_part", distro.lvm_details.lvm_root_part.as_str());
            env::set_var("lvm_usr_part", distro.lvm_details.lvm_usr_part.as_str());
            env::set_var("lvm_var_part", distro.lvm_details.lvm_var_part.as_str());
            env::remove_var("isUbuntu");
            env::remove_var("isSuse");
        }
        DistroKind::Undefined => {} // Nothing to do
    }

    if action_name == constants::CHROOT_CLI {
        // create a TMUX session which is used while one works directly in the chroot environment 
        match process::Command::new("tmux")
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg("rescue")
            .arg("chroot")
            .arg(constants::RESCUE_ROOT)
            .arg("/bin/bash")
            .spawn()
            .expect("chroot can not be started")
            .wait()
        {
            Ok(_) => (),
            Err(err) => return Err(err),
        }
        
        // Need to prepare the environment to make chroot a bit more safer
        match process::Command::new("tmux")
            .arg("send-keys")
            .arg("-t")
            .arg("rescue")
            .arg(format!(". {}/safe-exit.sh", constants::ACTION_IMPL_DIR))
            .arg("Enter")
            .spawn()
            .expect("tmux script not run able")
            .wait()
        {
            Ok(_) => (),
            Err(err) => return Err(err),
        }
        
        match process::Command::new("tmux")
            .arg("attach")
            .arg("-t")
            .arg("rescue")
            .spawn()
            .expect("tmux attach not possible")
            .wait()
        {
            Ok(_) => (),
            Err(err) => return Err(err),
        }

    } else {
        // This is the normal arm to be used. Here we run the selected action 
        // At first make the script executable
        let file_name = format!("{}/{}-impl.sh", constants::ACTION_IMPL_DIR, action_name);
        cmd_lib::run_cmd!(chmod 500 ${file_name})?;

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

        io::stdout().write_all(&output.stdout).unwrap();
    }
    helper::log_info("----- Action stopped -----");

    Ok(())
}

pub(crate) fn is_action_available(action_name: &str) -> io::Result<bool> {
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

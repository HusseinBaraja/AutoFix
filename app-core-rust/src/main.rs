#[cfg(test)]
mod accessibility;
mod background;
mod ipc;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[cfg(test)]
mod platform;
#[cfg(test)]
pub mod secrets;
mod settings;
mod storage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let supported_args = args.is_empty()
        || args.as_slice() == ["--background"]
        || args.as_slice() == ["--shutdown-all"];
    if !supported_args {
        eprintln!("usage: AF-BG-Engine.exe [--background|--shutdown-all]");
        std::process::exit(2);
    }

    if args.is_empty() {
        launch_settings_shell()?;
        return Ok(());
    }

    if args.as_slice() == ["--shutdown-all"] {
        background::shutdown_process_group_mode();
        return Ok(());
    }

    background::run_background_mode().map_err(Into::into)
}

fn launch_settings_shell() -> Result<(), Box<dyn std::error::Error>> {
    let shell =
        find_settings_shell().ok_or("Autofix.exe was not found. Build ui/settings-ui first.")?;
    let mut command = Command::new(&shell);
    command
        .current_dir(
            shell
                .parent()
                .ok_or("Autofix.exe has no parent directory.")?,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x0000_0008 | 0x0000_0200);
    command.spawn()?;
    Ok(())
}

fn find_settings_shell() -> Option<PathBuf> {
    settings_shell_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn settings_shell_candidates() -> Vec<PathBuf> {
    let Ok(current_exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let Some(exe_dir) = current_exe.parent() else {
        return Vec::new();
    };
    let binary_dir = if exe_dir.file_name().is_some_and(|name| name == "deps") {
        exe_dir.parent().unwrap_or(exe_dir)
    } else {
        exe_dir
    };
    let workspace_root = binary_dir
        .parent()
        .and_then(|target_dir| target_dir.parent())
        .map(PathBuf::from);

    let mut candidates = vec![binary_dir.join("Autofix.exe")];
    if let Some(root) = workspace_root {
        candidates.push(root.join("ui/settings-ui/bin/Debug/net8.0-windows/Autofix.exe"));
        candidates.push(root.join("ui/settings-ui/bin/Release/net8.0-windows/Autofix.exe"));
    }

    candidates
}

#[cfg(test)]
mod app_launch_tests {
    use super::*;

    #[test]
    fn settings_shell_candidates_prefer_installed_sibling_then_dev_builds() {
        let candidates = settings_shell_candidates();

        assert!(candidates
            .first()
            .is_some_and(|path| path.ends_with("Autofix.exe")));
        assert!(candidates
            .iter()
            .any(|path| path.ends_with("ui/settings-ui/bin/Debug/net8.0-windows/Autofix.exe")));
        assert!(candidates
            .iter()
            .any(|path| path.ends_with("ui/settings-ui/bin/Release/net8.0-windows/Autofix.exe")));
    }
}

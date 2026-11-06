use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use frametime_domain::{catalog::Step, cs2_config::OptionalCfgAsset};

use crate::{
    cli::{Branch, CleanupMode, Command, FpsStrategyValue, HardwareCommand},
    error::AppError,
};

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static ENGINE_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub(crate) fn install_cancellation_handler() {
    use windows::{Win32::System::Console::SetConsoleCtrlHandler, core::BOOL};

    unsafe extern "system" fn handler(control: u32) -> BOOL {
        use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
        // Only the engine loop polls the flag; every other command must stay interruptible.
        if (control == CTRL_BREAK_EVENT || control == CTRL_C_EVENT)
            && ENGINE_ACTIVE.load(Ordering::SeqCst)
        {
            CANCEL_REQUESTED.store(true, Ordering::SeqCst);
            BOOL(1)
        } else {
            BOOL(0)
        }
    }
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(handler), true);
    }
}

#[cfg(not(windows))]
pub(crate) const fn install_cancellation_handler() {}

#[cfg(windows)]
pub(crate) fn set_engine_active(active: bool) {
    ENGINE_ACTIVE.store(active, Ordering::SeqCst);
}

#[cfg(not(windows))]
pub(crate) const fn set_engine_active(_active: bool) {}

#[cfg(windows)]
pub(crate) fn cancellation_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::SeqCst)
}

#[cfg(not(windows))]
pub(crate) const fn cancellation_requested() -> bool {
    false
}

pub(crate) fn prompt_for_step(step: &Step) -> bool {
    if !io::stdin().is_terminal() {
        return false;
    }
    print!(
        "Apply P{}:{} {}? [y/N]: ",
        step.id.phase as u8, step.id.number, step.title
    );
    if io::stdout().flush().is_err() {
        return false;
    }
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).is_ok()
        && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

pub(crate) fn interactive_menu() -> Result<Command, AppError> {
    println!(
        "frametime.cfg\n1 Optimize\n2 Cleanup\n3 FPS cap\n4 Show log\n5 Reset progress\n6 Verify\n7 Restore\n8 Backup summary\nA Hardware assessment\nC Install packaged CS2 CFG\nS Boot Safe Mode\nP Phase 3\nD Full dry-run\n9 Exit"
    );
    let value = read_console_line("Choice: ")?;
    match value.trim().to_ascii_uppercase().as_str() {
        "1" => Ok(Command::Optimize { yes: false }),
        "2" => interactive_cleanup(),
        "3" => interactive_fps_cap(),
        "4" => Ok(Command::ShowLog),
        "5" => Ok(if confirm("Reset all workflow progress? [y/N]: ")? {
            Command::ResetProgress { yes: true }
        } else {
            Command::Exit
        }),
        "6" => Ok(Command::Verify),
        "7" => Ok(if confirm("Restore supported backup entries? [y/N]: ")? {
            Command::Restore { yes: true }
        } else {
            Command::Exit
        }),
        "8" => Ok(Command::BackupSummary),
        "A" => Ok(Command::Hardware {
            command: HardwareCommand::Doctor,
        }),
        "C" => interactive_cs2_cfg(),
        "S" => Ok(if confirm("Arm the verified Safe Mode handoff? [y/N]: ")? {
            Command::BootSafeMode { yes: true }
        } else {
            Command::Exit
        }),
        "P" => Ok(Command::Phase3 { yes: false }),
        "D" => Ok(Command::DryRun {
            branch: Branch::All,
        }),
        "9" => std::process::exit(0),
        _ => Err(AppError::Invalid("invalid menu choice".into())),
    }
}

fn interactive_cs2_cfg() -> Result<Command, AppError> {
    println!("Packaged CS2 CFG assets:");
    for (index, asset) in OptionalCfgAsset::ALL.iter().enumerate() {
        println!("{} {}", index + 1, asset.file_name());
    }
    let value = read_console_line("Asset number, or 0 to cancel: ")?;
    let Some(asset) = cfg_asset_menu_value(value.trim()) else {
        return if value.trim() == "0" {
            Ok(Command::Exit)
        } else {
            Err(AppError::Invalid("invalid CS2 CFG asset number".into()))
        };
    };
    Ok(
        if confirm("Back up and install this packaged CFG? [y/N]: ")? {
            Command::Cs2Cfg {
                assets: vec![asset],
                yes: true,
            }
        } else {
            Command::Exit
        },
    )
}

fn cfg_asset_menu_value(value: &str) -> Option<OptionalCfgAsset> {
    let index = value.parse::<usize>().ok()?.checked_sub(1)?;
    OptionalCfgAsset::ALL.get(index).copied()
}

fn interactive_cleanup() -> Result<Command, AppError> {
    let value = read_console_line("Cleanup mode: 1 Quick, 2 Full, 3 Driver, 4 Cancel: ")?;
    let mode = match value.trim() {
        "1" => CleanupMode::Quick,
        "2" => CleanupMode::Full,
        "3" => CleanupMode::Driver,
        "4" => return Ok(Command::Exit),
        _ => return Err(AppError::Invalid("invalid cleanup mode".into())),
    };
    let yes = confirm("Run this destructive cleanup mode? [y/N]: ")?;
    let acknowledge_irreversible = if yes && !matches!(mode, CleanupMode::Driver) {
        confirm("This cleanup deletes Temp contents irreversibly. Acknowledge? [y/N]: ")?
    } else {
        false
    };
    Ok(if yes {
        Command::Cleanup {
            mode,
            yes,
            acknowledge_irreversible,
        }
    } else {
        Command::Exit
    })
}

fn interactive_fps_cap() -> Result<Command, AppError> {
    let value = read_console_line("Path of a VProf capture file (empty to go back): ")?;
    let Some(path) = fps_cap_menu_path(&value) else {
        return Ok(Command::Exit);
    };
    println!("{}", fps_cap_persist_hint(&path.display().to_string()));
    Ok(Command::FpsCap {
        average_fps: None,
        vprof_text: None,
        vprof_file: Some(path),
        clipboard: false,
        strategy: FpsStrategyValue::Raw,
        measured_cap: 0,
        refresh_hz: 0,
        ceiling_margin_hz: 3,
        label: "Manual benchmark".into(),
        copy: false,
        no_persist: true,
    })
}

/// The menu only evaluates; persisting a cap stays an explicit command.
fn fps_cap_persist_hint(path: &str) -> String {
    format!(
        "This evaluation changes nothing and copies nothing. To persist a cap, run:\n  frametime fps-cap --vprof-file \"{path}\" --measured-cap <N>\n  frametime fps-cap --vprof-file \"{path}\" --strategy vrr --refresh-hz <HZ>"
    )
}

fn fps_cap_menu_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim().trim_matches('"').trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

pub(crate) fn confirm_interactive(message: &str) -> bool {
    io::stdin().is_terminal() && confirm(message).unwrap_or(false)
}

fn confirm(prompt: &str) -> Result<bool, AppError> {
    let value = read_console_line(prompt)?;
    Ok(matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "y" | "yes" | "j" | "ja"
    ))
}

fn read_console_line(prompt: &str) -> Result<String, AppError> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| AppError::failed(error.to_string()))?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| AppError::failed(error.to_string()))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_asset_menu_indexes_the_complete_closed_set() {
        assert_eq!(cfg_asset_menu_value("1"), Some(OptionalCfgAsset::NetStable));
        assert_eq!(
            cfg_asset_menu_value("15"),
            Some(OptionalCfgAsset::Competitive2026)
        );
        assert_eq!(cfg_asset_menu_value("0"), None);
        assert_eq!(cfg_asset_menu_value("16"), None);
        assert_eq!(cfg_asset_menu_value("not-a-number"), None);
    }

    #[test]
    fn fps_cap_menu_path_ignores_blank_input_and_surrounding_quotes() {
        assert_eq!(fps_cap_menu_path("  \n"), None);
        assert_eq!(fps_cap_menu_path("\"\""), None);
        assert_eq!(
            fps_cap_menu_path(" \"C:\\caps\\run.txt\" \n"),
            Some(PathBuf::from("C:\\caps\\run.txt"))
        );
    }

    #[test]
    fn fps_cap_persist_hint_names_both_explicit_commands() {
        let hint = fps_cap_persist_hint("run.txt");
        assert!(hint.contains("frametime fps-cap --vprof-file \"run.txt\" --measured-cap <N>"));
        assert!(hint.contains("--strategy vrr"));
    }
}

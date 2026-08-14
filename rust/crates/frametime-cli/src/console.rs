use std::io::{self, IsTerminal, Write};

use frametime_core::Step;
use frametime_windows::platform_is_supported;

use crate::{
    cli::{Branch, CleanupMode, Command, HardwareCommand},
    error::AppError,
};

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub(crate) fn install_cancellation_handler() {
    use windows::{Win32::System::Console::SetConsoleCtrlHandler, core::BOOL};

    unsafe extern "system" fn handler(control: u32) -> BOOL {
        use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
        if control == CTRL_BREAK_EVENT || control == CTRL_C_EVENT {
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
        step.phase as u8, step.number, step.title
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
        "frametime.cfg\n1 Optimize\n2 Cleanup\n3 FPS cap\n4 Show log\n5 Reset progress\n6 Verify\n7 Restore\n8 Backup summary\nA Hardware assessment\nS Boot Safe Mode\nP Phase 3\nD Full dry-run\n9 Exit"
    );
    print!("Choice: ");
    io::stdout()
        .flush()
        .map_err(|error| AppError::failed(error.to_string()))?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| AppError::failed(error.to_string()))?;
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

fn interactive_cleanup() -> Result<Command, AppError> {
    print!("Cleanup mode: 1 Quick, 2 Full, 3 Driver, 4 Cancel: ");
    io::stdout()
        .flush()
        .map_err(|error| AppError::failed(error.to_string()))?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| AppError::failed(error.to_string()))?;
    let mode = match value.trim() {
        "1" => CleanupMode::Quick,
        "2" => CleanupMode::Full,
        "3" => CleanupMode::Driver,
        "4" => return Ok(Command::Exit),
        _ => return Err(AppError::Invalid("invalid cleanup mode".into())),
    };
    let yes = confirm("Run this destructive cleanup mode? [y/N]: ")?;
    let acknowledge_irreversible = if yes && matches!(mode, CleanupMode::Full) {
        confirm("Full cleanup is irreversible. Acknowledge irreversible effects? [y/N]: ")?
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
    print!("Measured average FPS: ");
    io::stdout()
        .flush()
        .map_err(|error| AppError::failed(error.to_string()))?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| AppError::failed(error.to_string()))?;
    let average_fps = value
        .trim()
        .parse::<f64>()
        .map_err(|_| AppError::Invalid("average FPS must be a number".into()))?;
    Ok(Command::FpsCap {
        average_fps: Some(average_fps),
        vprof_text: None,
        vprof_file: None,
        clipboard: false,
        reduction: 0.09,
        minimum: 60,
        label: "Manual benchmark".into(),
        copy: platform_is_supported(),
        no_persist: !platform_is_supported(),
    })
}

fn confirm(prompt: &str) -> Result<bool, AppError> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|error| AppError::failed(error.to_string()))?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| AppError::failed(error.to_string()))?;
    Ok(matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "y" | "yes" | "j" | "ja"
    ))
}

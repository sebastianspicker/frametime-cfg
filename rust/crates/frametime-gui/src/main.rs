#[cfg(any(test, windows))]
mod diagnostics;
#[cfg(any(test, windows))]
mod model;

#[cfg(windows)]
mod app;

#[cfg(not(windows))]
fn main() {
    eprintln!("frametime-gui requires x64 Windows 10 or 11");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = frametime_windows::harden_process_dll_search() {
        eprintln!("frametime-gui startup refused: {error}");
        std::process::exit(1);
    }
    if (app::safe_mode_active() || std::env::var_os("SAFEBOOT_OPTION").is_some())
        && !model::gui_allows_phase_2_in_safe_mode()
    {
        eprintln!(
            "frametime-gui is unavailable in Safe Mode; return to a normal Windows session before using the GUI"
        );
        std::process::exit(2);
    }
    if std::env::args().any(|arg| arg == "--smoke-test") {
        println!("frametime-gui native surface available");
        return;
    }
    let package =
        model::PackageAuthentication::authenticate(frametime_windows::authenticate_current_package);
    if let Err(error) = app::run(package) {
        eprintln!("frametime-gui failed: {error}");
        std::process::exit(1);
    }
}

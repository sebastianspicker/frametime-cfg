mod app;
mod cli;
mod console;
mod error;
mod package_auth;
mod render;

fn main() -> std::process::ExitCode {
    #[cfg(windows)]
    if let Err(error) = frametime_windows::harden_process_dll_search() {
        eprintln!("frametime startup refused: {error}");
        return std::process::ExitCode::FAILURE;
    }
    app::main()
}

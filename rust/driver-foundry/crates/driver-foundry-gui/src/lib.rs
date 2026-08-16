//! Productive GUI for clean + install, sharing the same engines as the CLI.

mod app;
mod clean;
mod install;

pub use app::{run_gui, AppMode, DriverFoundryApp};
pub use clean::CleanUiState;
pub use install::{InstallPage, InstallUiState};

#[cfg(test)]
mod tests;

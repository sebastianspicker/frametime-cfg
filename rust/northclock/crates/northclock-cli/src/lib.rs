#![forbid(unsafe_code)]

mod definitions;
mod mapping;
mod output;

use clap::Parser;
use definitions::Cli;
use mapping::to_application_command;
use northclock_core::{ApplicationService, CommandEnvelope};
use northclock_platform_windows::WindowsPlatform;
use output::emit;
use std::ffi::OsString;

pub fn run<I, T>(arguments: I) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() { 2 } else { 0 };
            let _ = error.print();
            return code;
        }
    };

    let (command, deprecation) = match to_application_command(&cli) {
        Ok(mapped) => mapped,
        Err(error) => {
            let envelope = CommandEnvelope::failure("cli", None, error);
            emit(&envelope, cli.json);
            return envelope.exit_code();
        }
    };
    if let Some(message) = deprecation {
        eprintln!("warning: {message}");
    }
    let platform = WindowsPlatform::new();
    let service = match WindowsPlatform::local_app_data_dir() {
        Ok(root) => ApplicationService::with_storage(platform, northclock_core::Storage::new(root)),
        Err(error) => {
            eprintln!("warning: persistence disabled: {error}");
            ApplicationService::new(platform)
        }
    };
    let envelope = service.execute(command);
    emit(&envelope, cli.json);
    envelope.exit_code()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_usage_uses_exit_two() {
        assert_eq!(run(["northclock", "cpu"]), 2);
    }
}

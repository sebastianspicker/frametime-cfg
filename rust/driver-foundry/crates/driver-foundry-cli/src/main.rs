//! Driver Foundry — native GPU driver cleanup and custom installation for Windows.

mod clean_cmd;
mod cli;
mod install_cmd;

use clap::Parser;
use cli::{Cli, Commands};
use driver_foundry_common::{version_line, COMMAND_NAME, PRODUCT_NAME, PRODUCT_TAGLINE};
use std::process::ExitCode;

fn main() -> ExitCode {
    match Cli::parse().command {
        None => run_default(),
        Some(Commands::Clean(args)) => clean_cmd::run(*args),
        Some(Commands::Install(args)) => install_cmd::run(*args),
        Some(Commands::ListPackages { catalog }) => install_cmd::list_packages(catalog),
        Some(Commands::ListLanguages) => install_cmd::list_languages(),
        Some(Commands::Gui) => run_gui(),
    }
}

fn run_default() -> ExitCode {
    if std::env::var_os("DFOUNDRY_FORCE_CLI_HELP").is_some() {
        print_banner_help();
        return ExitCode::SUCCESS;
    }
    if std::env::var_os("DFOUNDRY_AUTO_GUI").is_some() {
        return run_gui();
    }
    print_banner_help();
    ExitCode::SUCCESS
}

fn print_banner_help() {
    print_banner();
    println!();
    println!("Usage: {COMMAND_NAME} <COMMAND>");
    println!();
    println!("Commands:");
    println!("  clean           Plan/execute GPU driver clean (dry-run default)");
    println!("  install         Package filter/strip + install dry-run / force-install");
    println!("  list-packages   List packages.v1.json component ids");
    println!("  list-languages  List shipped UI language packs");
    println!("  gui             Interactive clean + install GUI");
    println!();
    println!(
        "Try `{COMMAND_NAME} --help`, `{COMMAND_NAME} clean --help`, `{COMMAND_NAME} install --help`"
    );
    println!("{}", version_line());
}

pub(crate) fn print_banner() {
    println!("{PRODUCT_NAME} — {PRODUCT_TAGLINE}");
    println!("Native Rust application for Windows");
}

fn run_gui() -> ExitCode {
    print_banner();
    println!("domain: gui (eframe/egui — shares clean/install engines)");
    println!("{}", version_line());
    match driver_foundry_gui::run_gui() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: gui failed: {error}");
            eprintln!("hint: GUI requires a display. Engines remain available via the CLI.");
            ExitCode::from(1)
        }
    }
}

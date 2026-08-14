#![forbid(unsafe_code)]

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "northclock-gui", version)]
struct Arguments {
    /// Open a transparent always-on-top read-only measurement overlay.
    #[arg(long)]
    overlay: bool,
}

fn main() {
    let arguments = Arguments::parse();
    let result = if arguments.overlay {
        northclock_gui::run_overlay()
    } else {
        northclock_gui::run()
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

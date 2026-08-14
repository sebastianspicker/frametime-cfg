#![forbid(unsafe_code)]

use clap::Parser;
use northclock_core::{ApplicationCommand, ApplicationService};
use northclock_platform_windows::WindowsPlatform;

#[derive(Debug, Parser)]
#[command(
    name = "northclock-vram-worker",
    version,
    about = "Isolated D3D12 VRAM validation worker"
)]
struct Arguments {
    #[arg(long)]
    adapter: Option<String>,
    #[arg(long, default_value_t = 256 * 1024 * 1024)]
    bytes: u64,
    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,
}

fn main() {
    let arguments = Arguments::parse();
    let service = ApplicationService::new(WindowsPlatform::for_vram_worker());
    let envelope = service.execute(ApplicationCommand::VramTest {
        adapter: arguments.adapter,
        bytes: arguments.bytes,
        timeout_ms: arguments.timeout_ms,
    });
    match serde_json::to_string(&envelope) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("could not serialize worker result: {error}");
            std::process::exit(1);
        }
    }
    std::process::exit(i32::from(envelope.exit_code()));
}

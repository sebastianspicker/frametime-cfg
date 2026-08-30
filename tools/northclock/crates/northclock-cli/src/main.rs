#![forbid(unsafe_code)]

fn main() {
    let code = northclock_cli::run(std::env::args_os());
    std::process::exit(i32::from(code));
}

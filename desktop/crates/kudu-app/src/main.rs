// No console window for the release build on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod daemon;
mod ipc;
mod overlay;
mod paths;
mod platform;
mod settings;
mod window;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = if args.iter().any(|a| a == "--window") {
        paths::Paths::new().and_then(window::run)
    } else {
        daemon::run(&args)
    };
    if let Err(error) = result {
        log::error!("{error:#}");
        eprintln!("Kudu: {error:#}");
        std::process::exit(1);
    }
}

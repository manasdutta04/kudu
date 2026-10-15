// No console window for the release build on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod daemon;
mod ipc;
mod overlay;
mod paths;
mod platform;
mod settings;
mod window;

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let is_window = args.iter().any(|a| a == "--window");
    let result = paths::Paths::new().and_then(|paths| {
        init_logging(&paths, if is_window { "window" } else { "kudu" });
        if is_window {
            window::run(paths)
        } else {
            daemon::run(&args, paths)
        }
    });
    if let Err(error) = result {
        log::error!("{error:#}");
        eprintln!("Kudu: {error:#}");
        std::process::exit(1);
    }
}

/// Logs go to `kudu.log` in Kudu's local data folder (a GUI app on Windows has no console),
/// so problems on a user's PC can be diagnosed. They never contain dictated text.
fn init_logging(paths: &paths::Paths, process: &'static str) {
    // Keep the file small: start over once it passes 2 MB.
    if std::fs::metadata(&paths.log).is_ok_and(|m| m.len() > 2_000_000) {
        let _ = std::fs::remove_file(&paths.log);
    }
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.format(move |out, record| {
        writeln!(out, "{} [{process}] {} {}: {}", out.timestamp_seconds(), record.level(), record.target(), record.args())
    });
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&paths.log) {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    let _ = builder.try_init();

    // A crash anywhere is written to the log with where it happened, instead of vanishing.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        log::error!("Crash in thread '{thread}': {info}");
        default_hook(info);
    }));
    log::info!("Kudu {} started ({process})", env!("CARGO_PKG_VERSION"));
}

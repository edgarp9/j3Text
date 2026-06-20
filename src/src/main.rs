#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use j3text::{error::AppError, platform};

fn main() {
    if let Err(error) = platform::run() {
        report_startup_error(&error);
        std::process::exit(1);
    }
}

fn report_startup_error(error: &AppError) {
    eprintln!("{}", error.user_message());
    eprintln!("internal error: {error}");
    platform::report_fatal_startup_error(error);
}

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod windows_app;

#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = windows_app::run() {
        if std::env::args().any(|argument| argument == "--benchmark") {
            eprintln!("{error}");
            std::process::exit(1);
        } else {
            windows_app::show_fatal_error(&error);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This MVP currently uses the Windows-native fast path. A portable shell is planned.");
}

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod app;
mod cli;
mod config;
#[cfg(windows)]
mod handoff;
mod icons;
mod instance;
mod keys;
mod platform;
mod select;
mod session;
mod taskbar;
mod terminal_view;
mod text_field;
mod theme;
mod update;

fn main() {
    let options = match cli::Options::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            platform::show_startup_error(&format!("{error:#}"));
            std::process::exit(2);
        }
    };
    app::run(options);
}

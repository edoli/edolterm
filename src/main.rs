#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod app;
mod config;
mod keys;
mod platform;
mod session;
mod terminal_view;
mod text_field;
mod theme;

fn main() {
    app::run();
}

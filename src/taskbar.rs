//! Windows taskbar profile launchers, sharing an identity with installed shortcuts.
use crate::config::Profile;
use std::path::Path;

pub const APP_ID: &str = "dev.edoli.edolterm";

#[cfg(windows)]
mod windows;

pub fn update(profiles: &[Profile], directory: &Path) {
    #[cfg(windows)]
    windows::schedule(profiles.to_vec(), directory);
    #[cfg(not(windows))]
    let _ = (profiles, directory);
}

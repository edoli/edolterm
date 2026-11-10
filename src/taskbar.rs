//! Windows taskbar profile launchers, sharing an identity with installed shortcuts.
use crate::config::Profile;
use std::path::Path;

pub const APP_ID: &str = "dev.edoli.edolterm";

#[cfg(windows)]
mod windows;

pub fn update(
    profiles: &[Profile],
    directory: &Path,
) -> async_channel::Receiver<Vec<crate::icons::profiles::LoadedIcon>> {
    #[cfg(windows)]
    {
        windows::schedule(profiles.to_vec(), directory, true)
    }
    #[cfg(not(windows))]
    {
        let _ = (profiles, directory);
        async_channel::bounded(1).1
    }
}

#[cfg(all(feature = "ui-tests", debug_assertions, windows))]
pub fn load_icons(
    profiles: &[Profile],
    directory: &Path,
) -> async_channel::Receiver<Vec<crate::icons::profiles::LoadedIcon>> {
    windows::schedule(profiles.to_vec(), directory, false)
}

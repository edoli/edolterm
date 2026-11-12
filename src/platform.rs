//! OS integration is kept outside the terminal model and presentation.
use crate::config::Profile;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Opt-in startup diagnostics. No file is opened during normal launches.
pub fn trace_startup(stage: &str) {
    use std::{
        io::Write,
        sync::{Mutex, OnceLock},
        time::Instant,
    };
    struct Trace {
        start: Instant,
        file: Mutex<std::fs::File>,
    }
    static TRACE: OnceLock<Option<Trace>> = OnceLock::new();
    if let Some(trace) = TRACE.get_or_init(|| {
        let start = Instant::now();
        let path = std::env::var_os("EDOLTERM_STARTUP_TRACE")?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Trace {
            start,
            file: Mutex::new(file),
        })
    }) && let Ok(mut file) = trace.file.lock()
    {
        let _ = writeln!(
            file,
            "{}",
            serde_json::json!({
                "stage": stage, "elapsed_ms": trace.start.elapsed().as_secs_f64() * 1000.,
                "pid": std::process::id(),
            })
        );
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

pub fn show_startup_error(message: &str) {
    eprintln!("{message}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let title: Vec<u16> = "Edolterm".encode_utf16().chain(Some(0)).collect();
        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

pub fn find_program(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return path.is_file().then(|| path.into());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| {
            if cfg!(windows) && path.extension().is_none() {
                vec![dir.join(format!("{name}.exe")), dir.join(name)]
            } else {
                vec![dir.join(name)]
            }
        })
        .find(|path| path.is_file())
}

pub fn discover_profiles() -> Vec<Profile> {
    trace_startup("profile_search");
    let mut profiles = Vec::new();
    #[cfg(windows)]
    {
        let pwsh = find_program("pwsh").or_else(|| {
            std::env::var_os("ProgramFiles")
                .map(|p| PathBuf::from(p).join("PowerShell/7/pwsh.exe"))
                .filter(|p| p.is_file())
        });
        if let Some(p) = pwsh {
            profiles.push(Profile::new(
                "pwsh",
                "PowerShell",
                p.to_string_lossy(),
                &["-NoLogo"],
            ));
        }
        let system =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
                .join("System32");
        profiles.push(Profile::new(
            "powershell",
            "Windows PowerShell",
            system
                .join("WindowsPowerShell/v1.0/powershell.exe")
                .to_string_lossy(),
            &["-NoLogo"],
        ));
        profiles.push(Profile::new(
            "cmd",
            "Command Prompt",
            system.join("cmd.exe").to_string_lossy(),
            &[],
        ));
        let wsl = system.join("wsl.exe");
        trace_startup("wsl_list");
        if wsl.is_file()
            && let Ok(output) = quiet_command(&wsl).args(["--list", "--quiet"]).output()
            && output.status.success()
        {
            let text = decode_wsl_output(&output.stdout);
            for name in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
                profiles.push(Profile::new(
                    &format!("wsl:{name}"),
                    &format!("{name} · WSL"),
                    wsl.to_string_lossy(),
                    &["--distribution", name, "--cd", "~"],
                ));
            }
        }
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        profiles.push(Profile::new("default", "Shell", shell, &["-l"]));
        for name in ["bash", "zsh", "fish"] {
            if let Some(p) = find_program(name) {
                profiles.push(Profile::new(name, name, p.to_string_lossy(), &["-l"]));
            }
        }
    }
    profiles
}

/// Resolve only saved data. Do not inspect executable paths or enumerate shells.
pub fn configured_profiles(settings: &crate::config::Settings) -> Vec<Profile> {
    merge_profiles(settings.discovered_profiles.clone(), &settings.profiles)
}

/// Search and persist once when neither a saved search nor a user profile exists.
pub fn initialize_profiles(
    settings: &mut crate::config::Settings,
    directory: &Path,
) -> anyhow::Result<bool> {
    initialize_profiles_using(settings, directory, discover_profiles)
}

fn initialize_profiles_using(
    settings: &mut crate::config::Settings,
    directory: &Path,
    discover: impl FnOnce() -> Vec<Profile>,
) -> anyhow::Result<bool> {
    if !settings.discovered_profiles.is_empty() || !settings.profiles.is_empty() {
        return Ok(false);
    }
    // Keep the found profiles usable in this process if persistence fails;
    // the caller reports the error and a later launch can retry saving.
    settings.discovered_profiles = discover();
    settings.save(directory)?;
    Ok(true)
}

#[cfg(windows)]
fn decode_wsl_output(bytes: &[u8]) -> String {
    if bytes.contains(&0) || bytes.starts_with(&[0xff, 0xfe]) {
        String::from_utf16_lossy(
            &bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b))
                .filter(|c| *c != 0xfeff)
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

pub fn merge_profiles(mut detected: Vec<Profile>, custom: &[Profile]) -> Vec<Profile> {
    for profile in custom {
        if let Some(p) = detected.iter_mut().find(|p| p.id == profile.id) {
            *p = profile.clone();
        } else {
            detected.push(profile.clone());
        }
    }
    detected
}

pub fn open_file(path: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        quiet_command("notepad.exe").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        quiet_command("open").arg(path).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        quiet_command("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Windows Terminal's scrollUp/scrollDown defaults use the wheel-lines preference.
pub fn keyboard_scroll_lines(screen_rows: usize) -> i32 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETWHEELSCROLLLINES, SystemParametersInfoW,
        };
        let mut lines = 3u32;
        // SAFETY: SPI_GETWHEELSCROLLLINES writes one UINT into this valid pointer.
        unsafe {
            SystemParametersInfoW(
                SPI_GETWHEELSCROLLLINES,
                0,
                (&mut lines as *mut u32).cast(),
                0,
            );
        }
        if lines == u32::MAX {
            screen_rows.min(i32::MAX as usize) as i32
        } else {
            lines.min(i32::MAX as u32) as i32
        }
    }
    #[cfg(not(windows))]
    {
        let _ = screen_rows;
        3
    }
}

pub fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_discoveries_are_available_without_search_and_custom_profiles_win() {
        let builtin = Profile::new("pwsh", "Built-in", "pwsh.exe", &[]);
        let cached = Profile::new(
            "wsl:Ubuntu",
            "Ubuntu",
            "wsl.exe",
            &["--distribution", "Ubuntu"],
        );
        let custom = Profile::new("pwsh", "Custom", "custom.exe", &[]);
        let settings = crate::config::Settings {
            discovered_profiles: vec![builtin.clone(), cached.clone()],
            profiles: vec![custom.clone()],
            ..Default::default()
        };
        assert_eq!(configured_profiles(&settings), [custom, cached]);
    }

    #[test]
    fn first_run_search_is_saved_and_subsequent_runs_never_search_or_rewrite() {
        use crate::config::Settings;
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let root = temporary.join(format!("edolterm-profile-init-{}", std::process::id()));
        let mut settings = Settings {
            font_size: 17.5,
            ..Settings::default()
        };
        let calls = std::cell::Cell::new(0);
        assert!(
            initialize_profiles_using(&mut settings, &root, || {
                calls.set(calls.get() + 1);
                vec![Profile::new(
                    "cached",
                    "Cached shell",
                    "__missing_executable__.exe",
                    &[],
                )]
            })
            .unwrap()
        );
        assert_eq!(calls.get(), 1);
        let original = std::fs::read(root.join("settings.toml")).unwrap();
        let mut reloaded = Settings::load(&root).unwrap();
        assert_eq!(reloaded.font_size, 17.5);
        assert_eq!(configured_profiles(&reloaded), settings.discovered_profiles);
        assert!(
            !initialize_profiles_using(&mut reloaded, &root, || panic!(
                "cached startup searched again"
            ))
            .unwrap()
        );
        assert_eq!(original, std::fs::read(root.join("settings.toml")).unwrap());
        assert!(root.canonicalize().unwrap().starts_with(&temporary));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_user_profiles_skip_initial_search_even_without_discoveries() {
        let custom = Profile::new("custom", "Custom", "missing.exe", &[]);
        let mut settings = crate::config::Settings {
            profiles: vec![custom.clone()],
            ..Default::default()
        };
        assert!(
            !initialize_profiles_using(&mut settings, Path::new("unused"), || panic!(
                "custom-only startup searched"
            ))
            .unwrap()
        );
        assert_eq!(configured_profiles(&settings), [custom]);
    }
    #[test]
    fn custom_profiles_override_without_duplication() {
        let base = vec![Profile::new("shell", "Shell", "sh", &[])];
        let custom = vec![Profile::new("shell", "My shell", "other", &["a b"])];
        let merged = merge_profiles(base, &custom);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], custom[0]);
    }
    #[test]
    #[cfg(windows)]
    fn wsl_unicode_names() {
        let bytes: Vec<_> = "Ubuntu\r\n개발\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(
            decode_wsl_output(&bytes).lines().collect::<Vec<_>>(),
            ["Ubuntu", "개발"]
        );
    }
}

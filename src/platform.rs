//! OS integration is kept outside the terminal model and presentation.
use crate::config::Profile;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

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

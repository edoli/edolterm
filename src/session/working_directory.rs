//! Shell-reported paths are independent of the terminal grid and UI.
//! Observe OSC 9;9 and OSC 7 without consuming or modifying any PTY bytes.
use super::input_modes::InputModes;
use crate::config::Profile;
use alacritty_terminal::{
    event::{OnResize, WindowSize},
    tty::{ChildEvent, EventedPty, EventedReadWrite},
    vte::{Params, Parser, Perform},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use polling::{Event, PollMode, Poller};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone)]
pub(super) struct WorkingDirectory {
    path: Arc<Mutex<PathBuf>>,
    reported: Arc<AtomicBool>,
}

impl WorkingDirectory {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path: Arc::new(Mutex::new(path)),
            reported: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn get(&self) -> PathBuf {
        self.path.lock().unwrap().clone()
    }

    pub fn reported(&self) -> bool {
        self.reported.load(Ordering::Acquire)
    }
}

impl Perform for WorkingDirectory {
    fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
        let path = match params {
            // A semicolon is legal in a filename; VTE splits OSC parameters.
            [b"9", b"9", rest @ ..] if !rest.is_empty() => {
                let bytes = rest.join(&b';');
                let Ok(text) = std::str::from_utf8(&bytes) else {
                    return;
                };
                PathBuf::from(
                    text.strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(text),
                )
            }
            [b"7", rest @ ..] if !rest.is_empty() => {
                let bytes = rest.join(&b';');
                let Ok(text) = std::str::from_utf8(&bytes) else {
                    return;
                };
                // Remote ssh hosts must not change the local restore directory.
                // Validate before URL normalization, which can drop a file URI's
                // hostname when its path starts with a Windows drive letter.
                let Some((scheme, rest)) = text.split_once("://") else {
                    return;
                };
                if !scheme.eq_ignore_ascii_case("file") {
                    return;
                }
                let host = rest.split('/').next().unwrap_or_default();
                let local_host = std::env::var("COMPUTERNAME")
                    .or_else(|_| std::env::var("HOSTNAME"))
                    .unwrap_or_default();
                if !host.is_empty()
                    && !host.eq_ignore_ascii_case("localhost")
                    && !host.eq_ignore_ascii_case(&local_host)
                {
                    return;
                }
                let Ok(mut uri) = url::Url::parse(text) else {
                    return;
                };
                if uri.host_str().is_some_and(|host| !host.is_empty()) {
                    let _ = uri.set_host(None);
                }
                let Ok(path) = uri.to_file_path() else { return };
                path
            }
            _ => return,
        };
        // Never resolve a relative report against the app's own cwd. Retain the
        // last valid filesystem directory when a shell visits Registry:, etc.
        if path.is_absolute() && path.is_dir() {
            *self.path.lock().unwrap() = path;
            if !self.reported.swap(true, Ordering::AcqRel) {
                crate::platform::trace_startup("shell_prompt");
            }
        }
    }
}

/// Integrate only interactive PowerShell launches. Explicit command/file
/// profiles retain their arguments and lifecycle. Profiles on disk are untouched.
pub(super) fn shell_args(profile: &Profile, env: &mut HashMap<String, String>) -> Vec<String> {
    let program = profile
        .program
        .trim_matches('"')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(program.as_str(), "cmd.exe" | "cmd") {
        let prompt = env
            .get("PROMPT")
            .cloned()
            .or_else(|| std::env::var("PROMPT").ok())
            .unwrap_or_else(|| "$P$G".into());
        env.insert("PROMPT".into(), format!("$E]9;9;$P$E\\{prompt}"));
    }
    let mut args = profile.args.clone();
    if matches!(program.as_str(), "wsl.exe" | "wsl")
        && let Some(directory) = env.get("EDOLTERM_RESTORE_CWD")
    {
        // wsl.exe accepts an absolute Windows path for --cd. Override its home
        // directory flag before any explicit Linux command arguments.
        let options_end = args
            .iter()
            .position(|arg| matches!(arg.as_str(), "--" | "--exec" | "-e"))
            .unwrap_or(args.len());
        if let Some(index) = args[..options_end].iter().position(|arg| arg == "--cd") {
            if index + 1 < options_end {
                args[index + 1] = directory.clone();
            } else {
                args.insert(index + 1, directory.clone());
            }
        } else {
            args.splice(0..0, ["--cd".into(), directory.clone()]);
        }
    }
    if matches!(
        program.as_str(),
        "powershell.exe" | "powershell" | "pwsh.exe" | "pwsh"
    ) && args.iter().all(|arg| {
        matches!(
            arg.to_ascii_lowercase().as_str(),
            "-nologo" | "-noprofile" | "-noexit" | "-interactive" | "-nol" | "-nop" | "-noe"
        )
    }) {
        // PowerShell's logical location is not the process CWD. Wrap the prompt
        // after normal profile loading, preserving the user's existing prompt.
        let script = include_str!("../../assets/shell-integration/powershell.ps1");
        let encoded = STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        if !args
            .iter()
            .any(|a| a.eq_ignore_ascii_case("-noexit") || a.eq_ignore_ascii_case("-noe"))
        {
            args.push("-NoExit".into());
        }
        args.extend(["-EncodedCommand".into(), encoded]);
    }
    args
}

pub(super) struct TrackedPty<T> {
    inner: T,
    parser: Parser,
    output: OutputState,
}

struct OutputState {
    cwd: WorkingDirectory,
    input_modes: InputModes,
}

impl Perform for OutputState {
    fn osc_dispatch(&mut self, params: &[&[u8]], terminated: bool) {
        self.cwd.osc_dispatch(params, terminated);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        self.input_modes
            .csi_dispatch(params, intermediates, ignore, action);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.input_modes.esc_dispatch(intermediates, ignore, byte);
    }
}

impl<T> TrackedPty<T> {
    pub fn new(inner: T, cwd: WorkingDirectory, input_modes: InputModes) -> Self {
        Self {
            inner,
            parser: Parser::new(),
            output: OutputState { cwd, input_modes },
        }
    }
}

impl<T: EventedReadWrite> Read for TrackedPty<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.reader().read(bytes)?;
        self.parser.advance(&mut self.output, &bytes[..count]);
        Ok(count)
    }
}

impl<T: EventedReadWrite> Write for TrackedPty<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.writer().write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.writer().flush()
    }
}

impl<T: EventedReadWrite> EventedReadWrite for TrackedPty<T> {
    type Reader = Self;
    type Writer = Self;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: We own the PTY and preserve its registration lifetime.
        unsafe { self.inner.register(poll, event, mode) }
    }
    fn reregister(&mut self, poll: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.inner.reregister(poll, event, mode)
    }
    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.inner.deregister(poll)
    }
    fn reader(&mut self) -> &mut Self::Reader {
        self
    }
    fn writer(&mut self) -> &mut Self::Writer {
        self
    }
}

impl<T: EventedPty> EventedPty for TrackedPty<T> {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.inner.next_child_event()
    }
}
impl<T: OnResize> OnResize for TrackedPty<T> {
    fn on_resize(&mut self, size: WindowSize) {
        self.inner.on_resize(size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_explicit_directory_overrides_home_without_changing_command_arguments() {
        let directory = "C:\\한글 폴더\\it's & [1]";
        let mut env = HashMap::from([("EDOLTERM_RESTORE_CWD".into(), directory.into())]);
        let profile = Profile::new(
            "wsl",
            "WSL",
            "wsl.exe",
            &["--distribution", "Ubuntu", "--cd", "~"],
        );
        assert_eq!(
            shell_args(&profile, &mut env),
            ["--distribution", "Ubuntu", "--cd", directory]
        );
        assert_eq!(shell_args(&profile, &mut HashMap::new()), profile.args);
        let profile = Profile::new(
            "wsl",
            "WSL",
            "wsl.exe",
            &["--exec", "program", "--cd", "command-option"],
        );
        assert_eq!(
            shell_args(&profile, &mut env),
            [
                "--cd",
                directory,
                "--exec",
                "program",
                "--cd",
                "command-option"
            ]
        );
    }

    #[test]
    fn fragmented_osc_reports_and_uri_encoding_keep_the_last_valid_local_path() {
        let root = std::env::temp_dir().join(format!("edolterm-osc-{}", std::process::id()));
        let dir = root.join("한글 ; folder");
        std::fs::create_dir_all(&dir).unwrap();
        let mut cwd = WorkingDirectory::new(root.clone());
        let shared = cwd.clone();
        assert!(!shared.reported());
        let mut parser = Parser::new();
        let report = format!("\x1b]9;9;\"{}\"\x1b\\", dir.display());
        for byte in report.as_bytes() {
            parser.advance(&mut cwd, &[*byte]);
        }
        assert_eq!(cwd.get(), dir);
        assert!(shared.reported());
        parser.advance(&mut cwd, b"\x1b]9;9;relative\x07\x1b]9;9;Registry:\x07");
        assert_eq!(cwd.get(), dir);
        let uri = url::Url::from_directory_path(&root).unwrap();
        parser.advance(&mut cwd, format!("\x1b]7;{uri}\x07").as_bytes());
        // URL directory paths can end with a slash; compare filesystem identity.
        assert_eq!(
            cwd.get().canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        let mut remote = url::Url::from_directory_path(&dir).unwrap();
        remote.set_host(Some("unrelated-ssh-host.invalid")).unwrap();
        parser.advance(&mut cwd, format!("\x1b]7;{remote}\x1b\\").as_bytes());
        assert_eq!(
            cwd.get().canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn integration_keeps_explicit_commands_and_custom_prompts() {
        let explicit = Profile::new("p", "P", "pwsh.exe", &["-NoProfile", "-Command", "exit 7"]);
        assert_eq!(shell_args(&explicit, &mut HashMap::new()), explicit.args);
        let interactive = Profile::new(
            "p",
            "P",
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            &["-NoLogo"],
        );
        let args = shell_args(&interactive, &mut HashMap::new());
        assert_eq!(&args[..3], ["-NoLogo", "-NoExit", "-EncodedCommand"]);
        let mut env = HashMap::from([("PROMPT".into(), "custom $P$G".into())]);
        shell_args(&Profile::new("c", "C", "cmd.exe", &[]), &mut env);
        assert!(env["PROMPT"].ends_with("custom $P$G"));
    }
}

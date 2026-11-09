use crate::config::{Profile, Settings};
#[cfg(any(test, debug_assertions))]
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::{
    Term,
    event::{Event, EventListener, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg},
    grid::{Dimensions, Scroll},
    sync::FairMutex,
    term::{Config, Osc52},
    tty,
    vte::ansi::{CursorShape, CursorStyle},
};
use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[path = "session/input_modes.rs"]
mod input_modes;
#[path = "session/working_directory.rs"]
mod working_directory;

#[path = "session/startup.rs"]
pub mod startup;

#[cfg(windows)]
#[path = "session/windows_pty.rs"]
pub(crate) mod windows_pty;

#[derive(Clone)]
pub struct EventProxy {
    pub(crate) tx: async_channel::Sender<Event>,
    pub(crate) pending: Arc<AtomicBool>,
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::Wakeup) && self.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.tx.try_send(event);
    }
}

pub type Terminal = Term<EventProxy>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}
impl Default for GridSize {
    fn default() -> Self {
        Self {
            cols: 100,
            rows: 30,
            cell_width: 9,
            cell_height: 21,
        }
    }
}
impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}
impl From<GridSize> for WindowSize {
    fn from(s: GridSize) -> Self {
        Self {
            num_cols: s.cols as u16,
            num_lines: s.rows as u16,
            cell_width: s.cell_width,
            cell_height: s.cell_height,
        }
    }
}

pub struct Session {
    pub terminal: Arc<FairMutex<Terminal>>,
    cwd: working_directory::WorkingDirectory,
    input_modes: input_modes::InputModes,
    pub size: GridSize,
    pub pending: Arc<AtomicBool>,
    sender: EventLoopSender,
    pub completed: async_channel::Receiver<()>,
}

pub fn term_config(settings: &Settings) -> Config {
    Config {
        scrolling_history: settings.scrollback,
        default_cursor_style: CursorStyle {
            shape: CursorShape::Beam,
            blinking: true,
        },
        kitty_keyboard: false,
        osc52: if settings.allow_osc52 {
            Osc52::OnlyCopy
        } else {
            Osc52::Disabled
        },
        ..Config::default()
    }
}

impl Session {
    pub fn start(
        profile: Profile,
        cwd: Option<PathBuf>,
        settings: &Settings,
        window_id: u64,
    ) -> Result<(Self, async_channel::Receiver<Event>)> {
        let restore_cwd = cwd.filter(|path| path.is_dir());
        let cwd = restore_cwd
            .clone()
            .or_else(|| profile.cwd.clone())
            .unwrap_or_else(crate::platform::home_dir);
        if !cwd.is_dir() {
            anyhow::bail!("시작 폴더를 찾을 수 없습니다: {}", cwd.display());
        }
        let (tx, rx) = async_channel::unbounded();
        let pending = Arc::new(AtomicBool::new(false));
        let proxy = EventProxy {
            tx,
            pending: pending.clone(),
        };
        let size = GridSize::default();
        let terminal = Arc::new(FairMutex::new(Term::new(
            term_config(settings),
            &size,
            proxy.clone(),
        )));
        let mut env: std::collections::HashMap<_, _> = profile.env.clone().into_iter().collect();
        if let Some(path) = restore_cwd {
            env.insert(
                "EDOLTERM_RESTORE_CWD".into(),
                path.to_string_lossy().into_owned(),
            );
        }
        let args = working_directory::shell_args(&profile, &mut env);
        env.entry("TERM".into())
            .or_insert_with(|| "xterm-256color".into());
        env.entry("COLORTERM".into())
            .or_insert_with(|| "truecolor".into());
        env.entry("TERM_PROGRAM".into())
            .or_insert_with(|| "edolterm".into());
        env.entry("TERM_PROGRAM_VERSION".into())
            .or_insert_with(|| env!("CARGO_PKG_VERSION").into());
        // Alacritty's Windows launcher concatenates the executable separately from args.
        // Quote executable paths containing spaces; escape_args handles each argument.
        #[cfg(windows)]
        let program = if profile.program.contains(' ') && !profile.program.starts_with('"') {
            format!("\"{}\"", profile.program)
        } else {
            profile.program.clone()
        };
        #[cfg(not(windows))]
        let program = profile.program.clone();
        let options = tty::Options {
            shell: Some(tty::Shell::new(program, args)),
            working_directory: Some(cwd.clone()),
            drain_on_exit: true,
            env,
            #[cfg(windows)]
            escape_args: true,
        };
        #[cfg(windows)]
        let pty = windows_pty::new(&options, size.into(), window_id);
        #[cfg(not(windows))]
        let pty = tty::new(&options, size.into(), window_id);
        let pty =
            pty.with_context(|| format!("{} 실행 실패 ({})", profile.name, profile.program))?;
        let cwd = working_directory::WorkingDirectory::new(cwd);
        let input_modes = input_modes::InputModes::default();
        let pty = working_directory::TrackedPty::new(pty, cwd.clone(), input_modes.clone());
        let event_loop = EventLoop::new(terminal.clone(), proxy, pty, true, false)?;
        let sender = event_loop.channel();
        let io_thread = event_loop.spawn();
        let (done_tx, completed) = async_channel::bounded(1);
        // Join and drop the PTY on a reaper thread: ClosePseudoConsole can block.
        std::thread::Builder::new()
            .name("edolterm-pty-reaper".into())
            .spawn(move || {
                drop(io_thread.join());
                let _ = done_tx.try_send(());
            })?;
        Ok((
            Self {
                terminal,
                cwd,
                input_modes,
                size,
                sender,
                pending,
                completed,
            },
            rx,
        ))
    }

    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        let _ = self.sender.send(Msg::Input(bytes.into().into()));
    }

    pub fn working_directory(&self) -> PathBuf {
        self.cwd.get()
    }

    pub fn win32_input_mode(&self) -> bool {
        self.input_modes.win32()
    }

    pub fn input(&self, bytes: impl Into<Vec<u8>>) {
        {
            let mut terminal = self.terminal.lock();
            terminal.selection = None;
            terminal.scroll_display(Scroll::Bottom);
        }
        self.write(bytes);
    }

    pub fn resize(&mut self, size: GridSize) {
        if self.size == size {
            return;
        }
        self.terminal.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(size.into()));
        self.size = size;
    }

    pub fn shutdown(&self) {
        let _ = self.sender.send(Msg::Shutdown);
    }

    #[cfg(any(test, debug_assertions))]
    pub fn plain_text(&self) -> String {
        let term = self.terminal.lock();
        term.bounds_to_string(
            Point::new(Line(-(term.history_size() as i32)), Column(0)),
            Point::new(
                Line(term.screen_lines() as i32 - 1),
                Column(term.columns() - 1),
            ),
        )
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[cfg(windows)]
    fn changed_shell_directories_survive_session_save_and_restart() {
        use crate::config::{SavedSession, SavedTab};
        let root = std::env::temp_dir().join(format!("edolterm-cwd-{}", std::process::id()));
        let mut profiles = vec![
            Profile::new(
                "powershell",
                "Windows PowerShell",
                "powershell.exe",
                &["-NoLogo", "-NoProfile"],
            ),
            Profile::new("cmd", "CMD", "cmd.exe", &["/D"]),
        ];
        if let Some(pwsh) = crate::platform::find_program("pwsh") {
            profiles.push(Profile::new(
                "pwsh",
                "PowerShell",
                pwsh.to_string_lossy(),
                &["-NoLogo", "-NoProfile"],
            ));
        }
        let mut saved = SavedSession::default();
        for profile in &profiles {
            let dir = root.join(format!("{} 한글 '프로젝트; 경로", profile.id));
            std::fs::create_dir_all(&dir).unwrap();
            // Windows can expose TEMP through an 8.3 alias while the shell
            // reports its long path. Compare resolved filesystem paths.
            let expected_directory = dir.canonicalize().unwrap();
            let (session, events) =
                Session::start(profile.clone(), None, &Settings::default(), 0).unwrap();
            let command = if profile.id == "cmd" {
                format!("cd /d \"{}\"\r", dir.display())
            } else {
                format!(
                    "Set-Location -LiteralPath '{}'\r",
                    dir.to_string_lossy().replace('\'', "''")
                )
            };
            session.input(command.into_bytes());
            pump_until(&session, &events, || {
                session
                    .working_directory()
                    .canonicalize()
                    .is_ok_and(|actual| actual == expected_directory)
            });
            saved.tabs.push(SavedTab {
                profile_id: profile.id.clone(),
                cwd: session.working_directory(),
            });
            stop_and_wait(&session);
        }
        saved.save(&root).unwrap();
        let saved = SavedSession::load(&root).unwrap();
        assert_eq!(saved.tabs.len(), profiles.len());
        for (tab, profile) in saved.tabs.into_iter().zip(profiles) {
            let (mut session, events) = Session::start(
                profile.clone(),
                Some(tab.cwd.clone()),
                &Settings::default(),
                0,
            )
            .unwrap();
            // Read the actual restarted shell location, not just Session metadata.
            let command = if profile.id == "cmd" {
                "echo RESTORED_%CD%\r"
            } else {
                "[Console]::WriteLine(('RESTORED_' + $PWD.ProviderPath))\r"
            };
            session.resize(GridSize {
                cols: 300,
                ..GridSize::default()
            });
            session.input(command.as_bytes().to_vec());
            let expected = format!("RESTORED_{}", tab.cwd.display());
            pump_until(&session, &events, || {
                session.plain_text().contains(&expected)
            });
            stop_and_wait(&session);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn powershell_integration_preserves_custom_prompt_and_command_status() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let mut programs = vec![PathBuf::from("powershell.exe")];
        if let Some(pwsh) = crate::platform::find_program("pwsh") {
            programs.push(pwsh);
        }
        for program in programs {
            let script = format!(
                "function global:prompt {{ \"PROMPT_STATUS:$?,$global:LASTEXITCODE,$($global:Error.Count)> \" }}\n{}",
                include_str!("../assets/shell-integration/powershell.ps1")
            );
            let encoded = STANDARD.encode(
                script
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            let profile = Profile::new(
                "prompt",
                "Prompt",
                program.to_string_lossy(),
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NoExit",
                    "-EncodedCommand",
                    &encoded,
                ],
            );
            let (session, events) = Session::start(profile, None, &Settings::default(), 0).unwrap();
            session.input(b"cmd.exe /d /c exit 7\r".to_vec());
            pump_until(&session, &events, || {
                session.plain_text().contains("PROMPT_STATUS:False,7,0>")
            });
            session.input(b"$null = 1\r".to_vec());
            pump_until(&session, &events, || {
                session.plain_text().contains("PROMPT_STATUS:True,7,0>")
            });
            stop_and_wait(&session);
        }
    }

    #[cfg(windows)]
    fn pump_until(
        session: &Session,
        events: &async_channel::Receiver<Event>,
        ready: impl Fn() -> bool,
    ) {
        let start = Instant::now();
        loop {
            while let Ok(event) = events.try_recv() {
                match event {
                    Event::PtyWrite(text) => session.write(text.into_bytes()),
                    Event::Wakeup => session.pending.store(false, Ordering::Release),
                    _ => {}
                }
            }
            if ready() {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "cwd: {:?}, PTY output: {}",
                session.working_directory(),
                session.plain_text()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[cfg(windows)]
    fn stop_and_wait(session: &Session) {
        session.shutdown();
        let start = Instant::now();
        while session.completed.try_recv().is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "ConPTY did not close"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    #[cfg(windows)]
    fn conpty_preserves_background_colors_and_color_environment() {
        use alacritty_terminal::vte::ansi::{Color, Rgb};
        let script = r#"$e = [char]27; [Console]::Write("${e}[0m${e}[2J${e}[H${e}[48;2;38;38;38mT  ${e}[0m`r`n${e}[48;5;236mI  ${e}[0m`r`n${e}[100mN  ${e}[0m`r`n${e}]10;?${e}\${e}]11;?${e}\"); Start-Sleep -Milliseconds 300; $reply = ''; while ([Console]::KeyAvailable) { $reply += [Console]::ReadKey($true).KeyChar }; [Console]::Write("REPLY:$([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($reply)))`r`nENV:$env:TERM,$env:COLORTERM,$env:TERM_PROGRAM`r`n"); Start-Sleep -Seconds 20"#;
        let profile = Profile::new(
            "colors",
            "colors",
            "powershell.exe",
            &["-NoLogo", "-NoProfile", "-Command", script],
        );
        let (session, events) = Session::start(profile, None, &Settings::default(), 0).unwrap();
        let started = Instant::now();
        let mut color_requests = Vec::new();
        loop {
            while let Ok(event) = events.try_recv() {
                match event {
                    Event::PtyWrite(text) => session.write(text.into_bytes()),
                    Event::Wakeup => session.pending.store(false, Ordering::Release),
                    Event::ColorRequest(i, f) => {
                        color_requests.push(i);
                        let color = crate::theme::TerminalTheme::default()
                            .rgb(i, session.terminal.lock().renderable_content().colors);
                        session.write(f(color).into_bytes());
                    }
                    _ => {}
                }
            }
            let text = session.plain_text();
            if text.contains("ENV:") {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "PTY output: {text}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            session
                .plain_text()
                .contains("ENV:xterm-256color,truecolor,edolterm")
        );
        // System ConPTY versions may swallow OSC queries. Report this separately
        // from SGR preservation; lack of OSC support is not a cell color failure.
        eprintln!("OSC color queries received by terminal: {color_requests:?}");
        if let Some(reply) = session
            .plain_text()
            .lines()
            .find(|line| line.starts_with("REPLY:"))
        {
            eprintln!("OSC reply (base64): {}", &reply[6..]);
        }
        let term = session.terminal.lock();
        for (row, marker, gray) in [(0, 'T', 38), (1, 'I', 48), (2, 'N', 118)] {
            let expected = Rgb {
                r: gray,
                g: gray,
                b: gray,
            };
            assert_eq!(term.grid()[Point::new(Line(row), Column(0))].c, marker);
            for col in 0..3 {
                let cell = &term.grid()[Point::new(Line(row), Column(col))];
                let actual = match cell.bg {
                    Color::Spec(rgb) => rgb,
                    Color::Indexed(i) => crate::theme::TerminalTheme::default()
                        .rgb(i as usize, term.renderable_content().colors),
                    Color::Named(i) => crate::theme::TerminalTheme::default()
                        .rgb(i as usize, term.renderable_content().colors),
                };
                assert_eq!(actual, expected, "background for {marker} column {col}");
            }
        }
        drop(term);
        session.shutdown();
    }

    #[test]
    #[cfg(windows)]
    fn powershell_uses_utf8_after_legacy_console_encoding() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let mut programs = vec![PathBuf::from("powershell.exe")];
        if let Some(pwsh) = crate::platform::find_program("pwsh") {
            programs.push(pwsh);
        }
        for program in programs {
            let script = format!(
                "[Console]::InputEncoding = [Text.Encoding]::GetEncoding(437)\n\
                 [Console]::OutputEncoding = [Text.Encoding]::GetEncoding(437)\n\
                 $global:OutputEncoding = [Text.Encoding]::ASCII\n{}",
                include_str!("../assets/shell-integration/powershell.ps1")
            );
            let encoded = STANDARD.encode(
                script
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            let profile = Profile::new(
                "utf8",
                "UTF-8",
                program.to_string_lossy(),
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NoExit",
                    "-EncodedCommand",
                    &encoded,
                ],
            );
            let (session, events) = Session::start(profile, None, &Settings::default(), 0).unwrap();
            session.input(b"[Console]::WriteLine(('UTF8_' + [char]0xD55C + [char]0xAE00 + '_OK')); [Console]::WriteLine(('ENCODINGS_' + [Console]::InputEncoding.CodePage + ',' + [Console]::OutputEncoding.CodePage + ',' + $OutputEncoding.CodePage))\r".to_vec());
            pump_until(&session, &events, || {
                let output = session.plain_text();
                output.contains("UTF8_한글_OK") && output.contains("ENCODINGS_65001,65001,65001")
            });
            stop_and_wait(&session);
        }
    }

    #[test]
    #[cfg(windows)]
    fn conpty_unicode_input_resize_and_shutdown() {
        let profile = Profile::new("test", "test", "powershell.exe", &["-NoLogo", "-NoProfile"]);
        let (mut session, events) = Session::start(profile, None, &Settings::default(), 0).unwrap();
        session.resize(GridSize {
            cols: 73,
            rows: 19,
            ..GridSize::default()
        });
        session.input("[Console]::WriteLine(('PTY_' + [char]0xD55C + [char]0xAE00 + '_OK')); $host.UI.RawUI.WindowSize.Width\r".as_bytes().to_vec());
        let start = Instant::now();
        loop {
            while let Ok(event) = events.try_recv() {
                match event {
                    Event::PtyWrite(text) => session.write(text.into_bytes()),
                    Event::Wakeup => session.pending.store(false, Ordering::Release),
                    _ => {}
                }
            }
            let text = session.plain_text();
            if text.contains("PTY_한글_OK") && text.lines().any(|l| l.trim() == "73") {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "PTY output: {text}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        session.shutdown();
        let start = Instant::now();
        while session.completed.try_recv().is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "ConPTY did not close"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

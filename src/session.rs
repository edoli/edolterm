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
};
use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

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
    pub cwd: PathBuf,
    pub size: GridSize,
    pub pending: Arc<AtomicBool>,
    sender: EventLoopSender,
    pub completed: async_channel::Receiver<()>,
}

pub fn term_config(settings: &Settings) -> Config {
    Config {
        scrolling_history: settings.scrollback,
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
        let cwd = cwd
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
            shell: Some(tty::Shell::new(program, profile.args.clone())),
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
                        let color = crate::theme::Theme::new(false)
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
                    Color::Indexed(i) => crate::theme::Theme::new(false)
                        .rgb(i as usize, term.renderable_content().colors),
                    Color::Named(i) => crate::theme::Theme::new(false)
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

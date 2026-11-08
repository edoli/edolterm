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

#[derive(Clone)]
pub struct EventProxy {
    tx: async_channel::Sender<Event>,
    pending: Arc<AtomicBool>,
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
        let pty = tty::new(&options, size.into(), window_id)
            .with_context(|| format!("{} 실행 실패 ({})", profile.name, profile.program))?;
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

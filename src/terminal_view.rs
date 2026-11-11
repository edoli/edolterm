use crate::{
    config::{Profile, Settings},
    keys,
    session::{GridSize, Session, startup, term_config},
    text_field::utf8_offset,
    theme::Theme,
};
use alacritty_terminal::{
    event::Event,
    grid::{Dimensions, Scroll},
    index::{Boundary, Column, Direction, Line, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, cell::Flags, search::RegexSearch},
    vte::ansi::CursorShape,
};
use gpui::{prelude::*, *};
use std::{ops::Range, path::PathBuf, sync::atomic::Ordering, time::Duration};
use unicode_segmentation::UnicodeSegmentation;

mod file_drop;
mod layout;
mod links;
mod scroll;
use scroll::ScrollLines;

pub enum TerminalEvent {
    Metadata,
    Exited(bool),
}

struct SessionStarts(startup::Launcher);
impl Global for SessionStarts {}

enum PendingInput {
    Text(String),
    Paste(String),
    KeyDown(Keystroke),
    KeyUp(Keystroke),
}

pub struct TerminalView {
    pub session: Option<Session>,
    pub profile: Profile,
    pub title: String,
    pub error: Option<String>,
    pub exited: bool,
    pub bell: bool,
    pub focus: FocusHandle,
    pub settings: Settings,
    pub search_match: Option<std::ops::RangeInclusive<TermPoint>>,
    pub search_result: String,
    bounds: Bounds<Pixels>,
    cell_width: f32,
    cell_height: f32,
    grid_top: f32,
    preedit: String,
    preedit_selection: Range<usize>,
    cursor_bounds: Bounds<Pixels>,
    selecting: bool,
    link_mouse_down: bool,
    #[cfg(all(feature = "ui-tests", debug_assertions))]
    link_probe: Option<links::Probe>,
    scroll_lines: ScrollLines,
    last_mouse: Option<(usize, usize)>,
    cursor_on: bool,
    win32_enter_pressed: bool,
    blink: Option<Task<()>>,
    cwd: Option<PathBuf>,
    window_id: u64,
    grid_size: GridSize,
    startup: Option<startup::Pending>,
    startup_task: Option<Task<()>>,
    #[cfg(windows)]
    handoff: Option<crate::handoff::Incoming>,
    pending_input: Vec<PendingInput>,
    _receiver: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<TerminalEvent> for TerminalView {}

impl TerminalView {
    pub fn new(
        profile: Profile,
        cwd: Option<std::path::PathBuf>,
        settings: Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus(&focus, window, |this, _, cx| {
                this.bell = false;
                this.cursor_on = true;
                if let Some(s) = &this.session
                    && s.terminal.lock().mode().contains(TermMode::FOCUS_IN_OUT)
                {
                    s.write(b"\x1b[I".to_vec());
                }
                this.blink = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(600))
                            .await;
                        if this
                            .update(cx, |this, cx| {
                                this.cursor_on = !this.cursor_on;
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
                cx.emit(TerminalEvent::Metadata);
                cx.notify();
            }),
            cx.on_blur(&focus, window, |this, _, cx| {
                this.blink = None;
                this.cursor_on = true;
                if let Some(s) = &this.session
                    && s.terminal.lock().mode().contains(TermMode::FOCUS_IN_OUT)
                {
                    s.write(b"\x1b[O".to_vec());
                }
                cx.notify();
            }),
            cx.observe_window_activation(window, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |this, _, cx| {
                if this.settings.theme == "system" {
                    cx.notify();
                }
            }),
        ];
        Self {
            title: profile.name.clone(),
            profile,
            session: None,
            error: None,
            exited: false,
            bell: false,
            focus,
            settings,
            search_match: None,
            search_result: String::new(),
            bounds: Bounds::default(),
            cell_width: 9.,
            cell_height: 21.,
            grid_top: 6.,
            preedit: String::new(),
            preedit_selection: 0..0,
            cursor_bounds: Bounds::default(),
            selecting: false,
            link_mouse_down: false,
            #[cfg(all(feature = "ui-tests", debug_assertions))]
            link_probe: None,
            scroll_lines: ScrollLines::default(),
            last_mouse: None,
            cursor_on: true,
            win32_enter_pressed: false,
            blink: None,
            cwd,
            window_id: window.window_handle().window_id().as_u64(),
            grid_size: GridSize::default(),
            startup: None,
            startup_task: None,
            #[cfg(windows)]
            handoff: None,
            pending_input: Vec::new(),
            _receiver: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn start(&mut self, selected: bool, cx: &mut Context<Self>) {
        if self.exited || self.session.is_some() || self.error.is_some() {
            return;
        }
        if let Some(startup) = &self.startup {
            if selected {
                startup.prioritize();
            }
            return;
        }
        if cx.try_global::<SessionStarts>().is_none() {
            match startup::Launcher::new() {
                Ok(launcher) => cx.set_global(SessionStarts(launcher)),
                Err(error) => {
                    self.error = Some(format!("셸 시작 작업을 만들지 못했습니다: {error}"));
                    cx.notify();
                    return;
                }
            }
        }
        #[cfg(windows)]
        let incoming = self.handoff.take();
        #[cfg(windows)]
        let startup = if let Some(incoming) = incoming {
            let settings = self.settings.clone();
            cx.global::<SessionStarts>().0.submit(
                Box::new(move || Session::attach(incoming, &settings)),
                selected,
            )
        } else {
            cx.global::<SessionStarts>().0.start(
                self.profile.clone(),
                self.cwd.clone(),
                self.settings.clone(),
                self.window_id,
                selected,
            )
        };
        #[cfg(not(windows))]
        let startup = cx.global::<SessionStarts>().0.start(
            self.profile.clone(),
            self.cwd.clone(),
            self.settings.clone(),
            self.window_id,
            selected,
        );
        let result = startup.result.clone();
        self.startup = Some(startup);
        self.startup_task = Some(cx.spawn(async move |this, cx| {
            let result = result
                .recv()
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("셸 시작 작업이 종료되었습니다.")));
            let _ = this.update(cx, |this, cx| {
                this.startup.take();
                if this.exited {
                    return;
                }
                match result {
                    Ok((mut session, events)) => {
                        // Settings and viewport can change while the child is starting.
                        session
                            .terminal
                            .lock()
                            .set_options(term_config(&this.settings));
                        session.resize(this.grid_size);
                        this.session = Some(session);
                        for input in std::mem::take(&mut this.pending_input) {
                            match input {
                                PendingInput::Text(text) => this.input_text(text),
                                PendingInput::Paste(text) => this.input_paste(text),
                                PendingInput::KeyDown(key) => {
                                    this.input_key(&key);
                                }
                                PendingInput::KeyUp(key) => this.release_key(&key),
                            }
                        }
                        this._receiver = Some(cx.spawn(async move |this, cx| {
                            while let Ok(event) = events.recv().await {
                                let wakeup = matches!(event, Event::Wakeup);
                                if this.update(cx, |this, cx| this.event(event, cx)).is_err() {
                                    break;
                                }
                                if wakeup {
                                    cx.background_executor()
                                        .timer(Duration::from_millis(16))
                                        .await;
                                }
                            }
                        }));
                    }
                    Err(error) => {
                        this.error = Some(format!("{error:#}"));
                        this.pending_input.clear();
                    }
                }
                cx.emit(TerminalEvent::Metadata);
                cx.notify();
            });
        }));
    }

    #[cfg(windows)]
    pub fn receive_handoff(&mut self, incoming: crate::handoff::Incoming) {
        self.title = incoming
            .title
            .chars()
            .filter(|ch| !ch.is_control())
            .take(256)
            .collect();
        self.handoff = Some(incoming);
    }

    pub fn working_directory(&self) -> PathBuf {
        self.session
            .as_ref()
            .map(Session::working_directory)
            .or_else(|| self.cwd.clone())
            .or_else(|| self.profile.cwd.clone())
            .unwrap_or_else(crate::platform::home_dir)
    }

    #[cfg(all(feature = "ui-tests", debug_assertions))]
    pub(crate) fn startup_sequence(&self) -> Option<u64> {
        self.startup.as_ref().map(startup::Pending::sequence)
    }

    pub fn is_running(&self) -> bool {
        !self.exited && (self.session.is_some() || self.startup.is_some())
    }

    pub fn shutdown(&mut self) -> Vec<async_channel::Receiver<()>> {
        self.exited = true;
        self.startup_task.take();
        self.pending_input.clear();
        let mut completed = self
            .startup
            .take()
            .map(|startup| startup.cancel())
            .unwrap_or_default();
        if let Some(session) = &self.session {
            session.shutdown();
            completed.push(session.completed.clone());
        }
        completed
    }

    fn input_text(&mut self, text: String) {
        if self.exited || self.error.is_some() {
            return;
        }
        if let Some(session) = &self.session {
            session.input(text.into_bytes());
        } else {
            self.pending_input.push(PendingInput::Text(text));
        }
    }

    fn input_paste(&mut self, text: String) {
        if self.exited || self.error.is_some() {
            return;
        }
        if let Some(session) = &self.session {
            let bracketed = session
                .terminal
                .lock()
                .mode()
                .contains(TermMode::BRACKETED_PASTE);
            session.input(keys::paste(&text, bracketed));
        } else {
            self.pending_input.push(PendingInput::Paste(text));
        }
    }

    fn input_key(&mut self, key: &Keystroke) -> bool {
        let mode = self
            .session
            .as_ref()
            .map(|s| *s.terminal.lock().mode())
            .unwrap_or_default();
        let win32_enter =
            self.session.as_ref().is_some_and(Session::win32_input_mode) && key.key == "enter";
        let bytes = if win32_enter {
            keys::win32_enter(key, true)
        } else {
            keys::encode(key, mode)
        };
        let Some(bytes) = bytes else {
            return false;
        };
        if !self.exited && self.error.is_none() {
            if let Some(session) = &self.session {
                session.input(bytes);
                if win32_enter {
                    self.win32_enter_pressed = true;
                }
            } else {
                self.pending_input.push(PendingInput::KeyDown(key.clone()));
            }
        }
        true
    }

    fn release_key(&mut self, key: &Keystroke) {
        if key.key != "enter" {
            return;
        }
        if self.session.is_none() && !self.exited && self.error.is_none() {
            self.pending_input.push(PendingInput::KeyUp(key.clone()));
        } else if self.win32_enter_pressed {
            self.win32_enter_pressed = false;
            if let Some(session) = &self.session
                && !self.exited
                && session.win32_input_mode()
                && let Some(bytes) = keys::win32_enter(key, false)
            {
                session.write(bytes);
            }
        }
    }

    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        match event {
            Event::Wakeup => {
                if let Some(s) = &self.session {
                    s.pending.store(false, Ordering::Release);
                }
            }
            Event::Title(title) => {
                self.title = if title
                    .trim_matches('"')
                    .eq_ignore_ascii_case(self.profile.program.trim_matches('"'))
                {
                    self.profile.name.clone()
                } else {
                    title
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(256)
                        .collect()
                };
                cx.emit(TerminalEvent::Metadata);
            }
            Event::ResetTitle => {
                self.title = self.profile.name.clone();
                cx.emit(TerminalEvent::Metadata);
            }
            Event::PtyWrite(text) => {
                if let Some(s) = &self.session {
                    s.write(text.into_bytes());
                }
            }
            Event::ColorRequest(i, f) => {
                if let Some(s) = &self.session {
                    let color = theme
                        .terminal
                        .rgb(i, s.terminal.lock().renderable_content().colors);
                    s.write(f(color).into_bytes());
                }
            }
            Event::TextAreaSizeRequest(f) => {
                if let Some(s) = &self.session {
                    s.write(f(s.size.into()).into_bytes());
                }
            }
            Event::ClipboardStore(_, text) if self.settings.allow_osc52 => {
                cx.write_to_clipboard(ClipboardItem::new_string(text))
            }
            // Reading the host clipboard through OSC52 is deliberately not advertised.
            Event::ClipboardLoad(_, f) => {
                if let Some(s) = &self.session {
                    s.write(f("").into_bytes());
                }
            }
            Event::ChildExit(status) => {
                self.exited = true;
                let success = status.success();
                if !success {
                    self.error = Some(format!("프로세스가 종료되었습니다: {status}"));
                }
                cx.emit(TerminalEvent::Exited(success));
            }
            Event::Exit => {
                if !self.exited {
                    self.exited = true;
                    self.error
                        .get_or_insert_with(|| "터미널 연결이 종료되었습니다.".into());
                    cx.emit(TerminalEvent::Metadata);
                }
            }
            Event::Bell => {
                self.bell = true;
                cx.emit(TerminalEvent::Metadata);
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn apply_settings(&mut self, settings: &Settings, cx: &mut Context<Self>) {
        self.settings = settings.clone();
        if let Some(s) = &self.session {
            s.terminal.lock().set_options(term_config(settings));
        }
        cx.notify();
    }

    pub fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &self.session
            && let Some(text) = s.terminal.lock().selection_to_string()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let mut term = session.terminal.lock();
        let start = TermPoint::new(term.topmost_line(), Column(0));
        let mut end = start;
        // Include scrollback, but exclude unused rows after the last text cell.
        'rows: for row in (term.topmost_line().0..term.screen_lines() as i32).rev() {
            for col in (0..term.columns()).rev() {
                let point = TermPoint::new(Line(row), Column(col));
                let cell = &term.grid()[point];
                if cell.c != ' ' || cell.zerowidth().is_some_and(|chars| !chars.is_empty()) {
                    end = point;
                    break 'rows;
                }
            }
        }
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        term.selection = Some(selection);
        cx.notify();
    }
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
            self.input_paste(text);
            cx.notify();
        }
    }

    fn drop_files(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        let text = file_drop::paths_text(paths.paths(), &self.profile);
        if text.is_empty() {
            return;
        }
        self.input_paste(format!("{text} "));
        self.selecting = false;
        self.cursor_on = true;
        self.focus.focus(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    fn file_drag_move(
        &mut self,
        event: &DragMoveEvent<ExternalPaths>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selecting = false;
        // This GPUI revision translates FileDrop to mouse events only after
        // updating input modality. A drop after typing otherwise fails its
        // hover test. Feed the translated move through modality tracking once.
        if window.last_input_was_keyboard() {
            window.dispatch_event(PlatformInput::MouseMove(event.event.clone()), cx);
        }
    }

    pub fn search(&mut self, text: &str, backwards: bool, advance: bool, cx: &mut Context<Self>) {
        self.search_result.clear();
        if text.is_empty() {
            self.search_match = None;
            cx.notify();
            return;
        }
        let Some(s) = &self.session else {
            return;
        };
        let mut term = s.terminal.lock();
        let escaped: String = text
            .chars()
            .flat_map(|c| {
                if ".*+?()[]{}^$|\\".contains(c) {
                    vec!['\\', c]
                } else {
                    vec![c]
                }
            })
            .collect();
        let Ok(mut regex) = RegexSearch::new(&escaped) else {
            return;
        };
        let direction = if backwards {
            Direction::Left
        } else {
            Direction::Right
        };
        let origin = if advance {
            self.search_match.as_ref().map(|r| {
                if backwards {
                    r.start().sub(&*term, Boundary::None, 1)
                } else {
                    r.end().add(&*term, Boundary::None, 1)
                }
            })
        } else {
            None
        }
        .unwrap_or_else(|| TermPoint::new(Line(-(term.grid().display_offset() as i32)), Column(0)));
        self.search_match = term.search_next(
            &mut regex,
            origin,
            direction,
            if backwards { Side::Left } else { Side::Right },
            None,
        );
        if let Some(m) = &self.search_match {
            term.scroll_to_point(*m.start());
            self.search_result = "일치 항목".into();
        } else {
            self.search_result = "일치 항목 없음".into();
        }
        cx.notify();
    }

    fn key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.preedit.is_empty() {
            return;
        }
        if self.input_key(&e.keystroke) {
            self.cursor_on = true;
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn key_up(&mut self, e: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if e.keystroke.key == "enter" && (self.win32_enter_pressed || self.startup.is_some()) {
            self.release_key(&e.keystroke);
            cx.stop_propagation();
        }
    }

    fn grid_position(&self, p: Point<Pixels>) -> (usize, usize, Side) {
        let max = self.session.as_ref().map(|s| s.size).unwrap_or_default();
        let x = (f32::from(p.x - self.bounds.left()) - 8.).max(0.) / self.cell_width;
        let y = (f32::from(p.y - self.bounds.top()) - self.grid_top).max(0.) / self.cell_height;
        (
            (x as usize).min(max.cols - 1),
            (y as usize).min(max.rows - 1),
            if x.fract() >= 0.5 {
                Side::Right
            } else {
                Side::Left
            },
        )
    }
    fn report_mouse(&mut self, button: u8, pos: Point<Pixels>, release: bool, mods: Modifiers) {
        if let Some(s) = &self.session {
            let mode = *s.terminal.lock().mode();
            let (col, row, _) = self.grid_position(pos);
            if let Some(bytes) = keys::mouse(
                button,
                col,
                row,
                release,
                mode.contains(TermMode::SGR_MOUSE),
                mods,
            ) {
                #[cfg(all(feature = "ui-tests", debug_assertions))]
                if let Some(probe) = &mut self.link_probe {
                    probe.mouse_reports.push(bytes);
                    return;
                }
                s.write(bytes);
            }
        }
    }
    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.cursor_on = true;
        if e.button == MouseButton::Left {
            self.link_mouse_down = false;
            if links::activation(e.modifiers)
                && let Some(link) = self.link_at(e.position)
            {
                self.selecting = false;
                self.link_mouse_down = true;
                self.open_link(&link.uri, cx);
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        let Some(s) = &self.session else {
            return;
        };
        let mode = *s.terminal.lock().mode();
        let (col, row, side) = self.grid_position(e.position);
        if mode.intersects(TermMode::MOUSE_MODE) && !e.modifiers.shift {
            self.selecting = false;
            self.report_mouse(
                match e.button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    _ => 2,
                },
                e.position,
                false,
                e.modifiers,
            );
        } else if e.button == MouseButton::Left {
            let mut term = s.terminal.lock();
            let point = TermPoint::new(
                Line(row as i32 - term.grid().display_offset() as i32),
                Column(col),
            );
            let ty = if e.click_count >= 3 {
                SelectionType::Lines
            } else if e.click_count == 2 {
                SelectionType::Semantic
            } else if e.modifiers.alt {
                SelectionType::Block
            } else {
                SelectionType::Simple
            };
            term.selection = Some(Selection::new(ty, point, side));
            self.selecting = true;
        } else if e.button == MouseButton::Right {
            self.paste(cx);
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if cx.has_active_drag() {
            return;
        }
        if links::activation(e.modifiers) {
            cx.notify(); // Re-evaluate Ctrl-hover at the current pointer.
        }
        if self.link_mouse_down && e.pressed_button == Some(MouseButton::Left) {
            cx.stop_propagation();
            return;
        }
        let Some(s) = &self.session else {
            return;
        };
        let (col, row, side) = self.grid_position(e.position);
        if self.selecting && e.dragging() {
            let mut term = s.terminal.lock();
            if e.position.y < self.bounds.top() {
                term.scroll_display(Scroll::Delta(1));
            } else if e.position.y > self.bounds.bottom() {
                term.scroll_display(Scroll::Delta(-1));
            }
            let point = TermPoint::new(
                Line(row as i32 - term.grid().display_offset() as i32),
                Column(col),
            );
            if let Some(selection) = &mut term.selection {
                selection.update(point, side);
            }
            cx.notify();
        } else {
            let mode = *s.terminal.lock().mode();
            if self.last_mouse != Some((col, row))
                && !e.modifiers.shift
                && (mode.contains(TermMode::MOUSE_MOTION)
                    || mode.contains(TermMode::MOUSE_DRAG) && e.pressed_button.is_some())
            {
                let button = match e.pressed_button {
                    Some(MouseButton::Left) => 0,
                    Some(MouseButton::Middle) => 1,
                    Some(MouseButton::Right) => 2,
                    _ => 3,
                };
                self.report_mouse(32 + button, e.position, false, e.modifiers);
            }
        }
        self.last_mouse = Some((col, row));
    }
    fn mouse_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if e.button == MouseButton::Left && std::mem::take(&mut self.link_mouse_down) {
            self.selecting = false;
            cx.stop_propagation();
            return;
        }
        if cx.has_active_drag() {
            self.selecting = false;
            return;
        }
        if !self.selecting
            && let Some(s) = &self.session
            && s.terminal.lock().mode().intersects(TermMode::MOUSE_MODE)
            && !e.modifiers.shift
        {
            self.report_mouse(
                match e.button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    _ => 2,
                },
                e.position,
                true,
                e.modifiers,
            );
        }
        self.selecting = false;
    }
    fn open_link(&mut self, uri: &str, cx: &mut Context<Self>) {
        #[cfg(all(feature = "ui-tests", debug_assertions))]
        if let Some(probe) = &mut self.link_probe {
            probe.opened.push(uri.to_owned());
            return;
        }
        cx.open_url(uri);
    }
    fn scroll(&mut self, e: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = &self.session else {
            return;
        };
        cx.stop_propagation();
        let lines = self.scroll_lines.update(e, self.cell_height);
        if lines == 0 {
            return;
        }
        let mode = *s.terminal.lock().mode();
        if !e.modifiers.shift && mode.intersects(TermMode::MOUSE_MODE) {
            for _ in 0..lines.unsigned_abs().min(30) {
                self.report_mouse(
                    if lines > 0 { 64 } else { 65 },
                    e.position,
                    false,
                    e.modifiers,
                );
            }
        } else if !e.modifiers.shift
            && mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
        {
            let key = if lines > 0 { "\x1bOA" } else { "\x1bOB" };
            s.write(
                key.repeat(lines.unsigned_abs().min(30) as usize)
                    .into_bytes(),
            );
        } else {
            s.terminal.lock().scroll_display(Scroll::Delta(lines));
        }
        cx.notify();
    }

    fn prepare(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Frame {
        self.bounds = bounds;
        let old_cursor_bounds = self.cursor_bounds;
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let mut font = font(self.settings.font_family.clone());
        font.features = FontFeatures::disable_ligatures();
        font.fallbacks = Some(FontFallbacks::from_fonts(if cfg!(windows) {
            vec![
                "Consolas".into(),
                "Malgun Gothic".into(),
                "Segoe UI Emoji".into(),
            ]
        } else {
            vec!["monospace".into()]
        }));
        let size = px(self.settings.font_size);
        self.cell_width = (self.settings.font_size * self.settings.cell_width).max(1.);
        let vertical = layout::ViewportRows::fit(
            f32::from(bounds.size.height),
            self.settings.font_size,
            self.settings.line_height,
            window.scale_factor(),
        );
        self.cell_height = vertical.height;
        self.grid_top = vertical.top;
        let cols = ((f32::from(bounds.size.width) - 24.) / self.cell_width)
            .floor()
            .clamp(2., 4096.) as usize;
        let rows = vertical.count;
        self.grid_size = GridSize {
            cols,
            rows,
            cell_width: self.cell_width.ceil() as u16,
            cell_height: self.cell_height.ceil() as u16,
        };
        if let Some(s) = &mut self.session {
            s.resize(self.grid_size);
        }
        let mut frame = Frame {
            quads: Vec::new(),
            glyphs: Vec::new(),
            cursor: None,
            preedit: Vec::new(),
            scrollbar: None,
        };
        let Some(s) = &self.session else {
            return frame;
        };
        // Copy a compact snapshot while locked. Shaping and painting never hold the PTY lock.
        let (cells, cursor, offset, colors, selection, history) = {
            let term = s.terminal.lock();
            let content = term.renderable_content();
            let cells = content
                .display_iter
                .map(|c| (c.point, c.cell.clone()))
                .collect::<Vec<_>>();
            (
                cells,
                content.cursor,
                content.display_offset,
                *content.colors,
                content.selection,
                term.history_size(),
            )
        };
        let origin = bounds.origin + point(px(8.), px(self.grid_top));
        let hovered_link = links::activation(window.modifiers())
            .then(|| self.link_at(window.mouse_position()))
            .flatten();
        let focused = self.focus.is_focused(window) && window.is_window_active();
        let mut spans: Vec<TextSpan> = Vec::with_capacity(rows);
        for (p, cell) in cells {
            let row = p.line.0 + offset as i32;
            if row < 0 || row as usize >= rows {
                continue;
            }
            let pos = origin
                + point(
                    px(p.column.0 as f32 * self.cell_width),
                    px(row as f32 * self.cell_height),
                );
            let wide = cell.flags.contains(Flags::WIDE_CHAR);
            let mut fg = theme
                .terminal
                .color(cell.fg, &colors, cell.flags.contains(Flags::BOLD));
            let mut bg = theme.terminal.color(cell.bg, &colors, false);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if selection.is_some_and(|s| s.contains(p)) {
                bg = theme.terminal.selection;
            }
            if self.search_match.as_ref().is_some_and(|r| r.contains(&p)) {
                bg = theme.terminal.search_match.bg;
                fg = theme.terminal.search_match.fg;
            }
            if cell.flags.contains(Flags::DIM) {
                fg = fg.opacity(theme.terminal.dim_opacity);
            }
            let rect = Bounds::new(pos, gpui::size(px(self.cell_width), px(self.cell_height)));
            if bg != theme.terminal.bg {
                frame.quads.push(fill(rect, bg));
            }
            if cell.flags.intersects(
                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
            ) {
                continue;
            }
            let is_cursor = p == cursor.point
                && cursor.shape != CursorShape::Hidden
                && focused
                && self.cursor_on
                && self.preedit.is_empty();
            if is_cursor {
                let mut bounds = Bounds::new(
                    pos,
                    gpui::size(
                        px(self.cell_width * if wide { 2. } else { 1. }),
                        px(self.cell_height),
                    ),
                );
                match cursor.shape {
                    CursorShape::Beam => bounds.size.width = px(2.),
                    CursorShape::Underline => {
                        bounds.origin.y += px(self.cell_height - 2.);
                        bounds.size.height = px(2.);
                    }
                    _ => {
                        fg = theme.terminal.bg;
                    }
                }
                frame
                    .quads
                    .push(fill(bounds, theme.terminal.indexed(258, &colors)));
            }
            if p == cursor.point {
                self.cursor_bounds =
                    Bounds::new(pos, gpui::size(px(self.cell_width), px(self.cell_height)));
                frame.cursor = Some(self.cursor_bounds);
            }
            let mut text = cell.c.to_string();
            if let Some(chars) = cell.zerowidth() {
                text.extend(chars);
            }
            // Batch single-cell ASCII only. Wide/fallback glyphs and combining marks
            // retain their explicit cell origins, independent of font advances.
            let groupable = text.len() == 1 && cell.c.is_ascii() && !wide;
            let mut font = font.clone();
            if cell.flags.contains(Flags::BOLD) {
                font.weight = FontWeight::BOLD;
            }
            if cell.flags.contains(Flags::ITALIC) {
                font.style = FontStyle::Italic;
            }
            let run = TextRun {
                len: 0,
                font,
                color: fg,
                background_color: None,
                underline: (cell.flags.intersects(Flags::ALL_UNDERLINES)
                    || hovered_link
                        .as_ref()
                        .is_some_and(|link| link.cells.contains(&p)))
                .then_some(UnderlineStyle {
                    color: Some(fg),
                    thickness: px(1.),
                    wavy: cell.flags.contains(Flags::UNDERCURL),
                }),
                strikethrough: cell.flags.contains(Flags::STRIKEOUT).then_some(
                    StrikethroughStyle {
                        color: Some(fg),
                        thickness: px(1.),
                    },
                ),
            };
            if groupable
                && let Some(last) = spans.last_mut()
                && last.groupable
                && last.next == p
                && last.run == run
            {
                last.text.push_str(&text);
                last.next.column.0 += 1;
            } else {
                spans.push(TextSpan {
                    pos,
                    text,
                    run,
                    next: TermPoint::new(p.line, Column(p.column.0 + 1)),
                    groupable,
                });
            }
        }
        for mut span in spans {
            if span.run.underline.is_none() && span.run.strikethrough.is_none() {
                span.text.truncate(span.text.trim_end_matches(' ').len());
            }
            if span.text.is_empty() {
                continue;
            }
            span.run.len = span.text.len();
            let mut line =
                window
                    .text_system()
                    .shape_line(span.text.into(), size, &[span.run], None);
            if span.groupable {
                line = layout::align_ascii_cells(line, self.cell_width);
            }
            frame.glyphs.push((span.pos, line));
        }
        if !self.preedit.is_empty() {
            let run = TextRun {
                len: self.preedit.len(),
                font,
                color: theme.terminal.fg,
                background_color: Some(theme.panel),
                underline: Some(UnderlineStyle {
                    color: Some(theme.accent),
                    thickness: px(1.),
                    wavy: false,
                }),
                strikethrough: None,
            };
            let mut column = 0;
            for grapheme in self.preedit.graphemes(true) {
                let mut run = run.clone();
                run.len = grapheme.len();
                let line =
                    window
                        .text_system()
                        .shape_line(grapheme.to_owned().into(), size, &[run], None);
                let position =
                    self.cursor_bounds.origin + point(px(column as f32 * self.cell_width), px(0.));
                frame.preedit.push((position, line));
                column += unicode_width::UnicodeWidthStr::width(grapheme);
            }
        }
        if history > 0 {
            let height =
                (f32::from(bounds.size.height) * rows as f32 / (history + rows) as f32).max(24.);
            let y =
                (f32::from(bounds.size.height) - height) * (1. - offset as f32 / history as f32);
            frame.scrollbar = Some(fill(
                Bounds::new(
                    point(bounds.right() - px(7.), bounds.top() + px(y)),
                    gpui::size(px(3.), px(height)),
                ),
                theme.scrollbar,
            ));
        }
        if self.cursor_bounds != old_cursor_bounds && self.focus.is_focused(window) {
            window.invalidate_character_coordinates();
        }
        let _ = cx;
        frame
    }
}

struct TextSpan {
    pos: Point<Pixels>,
    text: String,
    run: TextRun,
    next: TermPoint,
    groupable: bool,
}

struct Frame {
    quads: Vec<PaintQuad>,
    glyphs: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<Bounds<Pixels>>,
    preedit: Vec<(Point<Pixels>, ShapedLine)>,
    scrollbar: Option<PaintQuad>,
}

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = utf8_offset(&self.preedit, r.start)..utf8_offset(&self.preedit, r.end);
        *actual = Some(
            self.preedit[..r.start].encode_utf16().count()
                ..self.preedit[..r.end].encode_utf16().count(),
        );
        Some(self.preedit[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.preedit_selection.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.preedit.is_empty()).then_some(0..self.preedit.encode_utf16().count())
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preedit.clear();
        self.preedit_selection = 0..0;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit.clear();
        self.preedit_selection = 0..0;
        self.input_text(text.to_owned());
        self.cursor_on = true;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = r
            .map(|r| utf8_offset(&self.preedit, r.start)..utf8_offset(&self.preedit, r.end))
            .unwrap_or(0..self.preedit.len());
        self.preedit.replace_range(r, text);
        let len = self.preedit.encode_utf16().count();
        self.preedit_selection = selected
            .map(|s| s.start.min(len)..s.end.min(len))
            .unwrap_or(len..len);
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let n = utf8_offset(&self.preedit, r.start);
        let width =
            unicode_width::UnicodeWidthStr::width(&self.preedit[..n]) as f32 * self.cell_width;
        Some(Bounds::new(
            self.cursor_bounds.origin + point(px(width), px(0.)),
            self.cursor_bounds.size,
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.preedit_selection.end)
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let a = cx.entity();
        let b = a.clone();
        let error = self.error.clone();
        let over_link = links::activation(window.modifiers())
            && self.link_at(window.mouse_position()).is_some();
        div()
            .id("terminal")
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .cursor_text()
            .when(over_link, |view| view.cursor_pointer())
            .on_hover(cx.listener(|_, _, _, cx| cx.notify()))
            .overflow_hidden()
            .bg(theme.terminal.bg)
            .on_key_down(cx.listener(Self::key))
            .on_key_up(cx.listener(Self::key_up))
            .on_modifiers_changed(cx.listener(|_, _, _, cx| cx.notify()))
            .on_drag_move(cx.listener(Self::file_drag_move))
            .on_drop(cx.listener(Self::drop_files))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll(cx.listener(Self::scroll))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        a.update(cx, |this, cx| this.prepare(bounds, window, cx))
                    },
                    move |bounds, frame, window, cx| {
                        let this = b.read(cx);
                        let focus = this.focus.clone();
                        let height = this.cell_height;
                        if focus.is_focused(window) {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, b.clone()),
                                cx,
                            );
                        }
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for quad in frame.quads {
                                window.paint_quad(quad);
                            }
                            for (pos, line) in frame.glyphs {
                                let _ =
                                    line.paint(pos, px(height), TextAlign::Left, None, window, cx);
                            }
                            for (pos, line) in frame.preedit {
                                let _ =
                                    line.paint(pos, px(height), TextAlign::Left, None, window, cx);
                            }
                            if let Some(quad) = frame.scrollbar {
                                window.paint_quad(quad);
                            }
                        });
                    },
                )
                .size_full(),
            )
            .when(
                self.session.is_none() && self.error.is_none() && !self.exited,
                |view| {
                    view.child(
                        div()
                            .absolute()
                            .left(px(12.))
                            .top(px(8.))
                            .text_color(theme.terminal.fg)
                            .text_size(px(13.))
                            .child("셸 시작 중…"),
                    )
                },
            )
            .when_some(error, |view, error| {
                view.child(
                    div()
                        .absolute()
                        .left(px(12.))
                        .right(px(12.))
                        .bottom(px(12.))
                        .p_3()
                        .rounded_md()
                        .bg(theme.error.bg)
                        .text_color(theme.error.fg)
                        .text_size(px(13.))
                        .child(error),
                )
            })
    }
}

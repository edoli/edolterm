//! A captured, process-wide tab drag. Keep the live TerminalView (and PTY) intact.
use super::*;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{ClientToScreen, ScreenToClient},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, ReleaseCapture, SetCapture, VK_ESCAPE, VK_LBUTTON,
        },
        WindowsAndMessaging::{
            GW_HWNDNEXT, GetCursorPos, GetTopWindow, GetWindow, GetWindowRect, HWND_TOP,
            IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SetWindowPos,
            ShowWindow,
        },
    },
};

#[derive(Default)]
struct Controller {
    session: Option<Drag>,
    active: bool,
    finish: Option<bool>,
}
impl Global for Controller {}

struct SettingsTab {
    draft: Settings,
    page: SettingsPage,
    numbers: Option<[String; 4]>,
    scroll: Point<Pixels>,
    shortcuts_scroll: Point<Pixels>,
}

impl SettingsTab {
    fn capture(app: &TerminalApp, cx: &App) -> Self {
        Self {
            draft: app.draft.clone(),
            page: app.settings_page,
            numbers: app.preferences.as_ref().map(|prefs| {
                [
                    &prefs.size,
                    &prefs.line_height,
                    &prefs.cell_width,
                    &prefs.history,
                ]
                .map(|input| input.read(cx).input.read(cx).text.clone())
            }),
            scroll: app.settings_scroll.offset(),
            shortcuts_scroll: app.shortcuts_scroll.offset(),
        }
    }

    fn restore(self, app: &mut TerminalApp, window: &mut Window, cx: &mut Context<TerminalApp>) {
        app.draft = self.draft;
        app.settings_page = self.page;
        app.preferences = None;
        if let Some(values) = self.numbers {
            app.ensure_preferences(window, cx);
            let prefs = app.preferences.as_ref().unwrap();
            for (input, value) in [
                &prefs.size,
                &prefs.line_height,
                &prefs.cell_width,
                &prefs.history,
            ]
            .into_iter()
            .zip(values)
            {
                input
                    .read(cx)
                    .input
                    .clone()
                    .update(cx, |input, cx| input.set_text(value, window, cx));
            }
        }
        app.settings_scroll.set_offset(self.scroll);
        app.shortcuts_scroll.set_offset(self.shortcuts_scroll);
    }
}

pub(super) struct Drag {
    origin: windows::Entry,
    current: windows::Entry,
    id: u64,
    original_id: u64,
    original_order: Vec<u64>,
    original_scroll: Point<Pixels>,
    grab_fraction: f32,
    grab_y: f32,
    geometry: SavedSession,
    floating: Option<windows::Entry>,
    detached: bool,
    touched: Vec<windows::Entry>,
}

pub(super) fn active(cx: &App) -> bool {
    cx.try_global::<Controller>()
        .is_some_and(|drag| drag.active)
}

pub(super) fn request_finish(cancel: bool, cx: &mut App) {
    let state = cx.default_global::<Controller>();
    // Escape takes precedence over a release queued in the same frame.
    state.finish = Some(cancel || state.finish == Some(true));
}

#[cfg(all(feature = "ui-tests", debug_assertions))]
pub(super) fn begin_test(app: &TerminalApp, window: &Window, cx: &mut App) -> Drag {
    let drag = Drag::new(app, window, cx).expect("smoke drag has crossed the threshold");
    cx.default_global::<Controller>().active = true;
    drag
}

#[cfg(all(feature = "ui-tests", debug_assertions))]
pub(super) fn finish_test(drag: Drag, cancel: bool, cx: &mut App) {
    drag.finish(cancel, cx);
    cx.set_global(Controller::default());
}

pub(super) fn begin_native(app: &TerminalApp, window: &Window, cx: &mut Context<TerminalApp>) {
    // Synthetic GPUI smoke input uses its own deterministic driver below.
    if active(cx) || unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } >= 0 {
        return;
    }
    let Some(drag) = Drag::new(app, window, cx) else {
        return;
    };
    cx.set_global(Controller {
        session: Some(drag),
        active: true,
        finish: None,
    });
    cx.spawn(async move |_, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let running = cx.update(|cx| {
                let Some(mut drag) = cx.global_mut::<Controller>().session.take() else {
                    return false;
                };
                let requested = cx.global_mut::<Controller>().finish.take();
                let cancel =
                    requested == Some(true) || unsafe { GetAsyncKeyState(VK_ESCAPE as i32) } < 0;
                let released =
                    requested == Some(false) || unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } >= 0;
                let mut pointer = POINT::default();
                let positioned = unsafe { GetCursorPos(&mut pointer) } != 0;
                let result = if positioned && !cancel {
                    drag.advance(pointer, cx)
                } else {
                    Ok(())
                };
                if let Err(error) = &result {
                    eprintln!("Tab drag: {error:#}");
                }
                if cancel || released || !positioned || result.is_err() {
                    drag.finish(cancel || !positioned || result.is_err(), cx);
                    cx.global_mut::<Controller>().active = false;
                    false
                } else {
                    cx.global_mut::<Controller>().session = Some(drag);
                    true
                }
            });
            if !running {
                break;
            }
        }
    })
    .detach();
}

impl Drag {
    #[cfg(all(feature = "ui-tests", debug_assertions))]
    pub(super) fn current(&self) -> windows::Entry {
        self.current.clone()
    }

    pub(super) fn new(app: &TerminalApp, window: &Window, cx: &App) -> Option<Self> {
        let drag = app.tab_drag.as_ref().filter(|drag| drag.started)?;
        let origin = cx
            .try_global::<windows::Windows>()?
            .entries
            .iter()
            .find(|entry| entry.id == window.window_handle().window_id().as_u64())?
            .clone();
        let (_, width, _) = app.tab_dimensions(window);
        Some(Self {
            origin: origin.clone(),
            current: origin.clone(),
            id: drag.id,
            original_id: drag.id,
            original_order: drag.original_order.clone(),
            original_scroll: drag.original_scroll,
            grab_fraction: (drag.grab_offset / width).clamp(0., 1.),
            grab_y: f32::from(drag.start.y).clamp(0., 35.),
            geometry: app.snapshot(window.window_bounds(), cx),
            floating: None,
            detached: false,
            touched: vec![origin],
        })
    }

    /// Screen coordinates are physical pixels. Convert through each HWND so mixed
    /// DPI, negative monitor coordinates and custom titlebar insets stay aligned.
    pub(super) fn advance(&mut self, pointer: POINT, cx: &mut App) -> anyhow::Result<()> {
        if let Some(target) = self.target(pointer, cx)
            && target.id != self.current.id
        {
            let restore_id = (target.id == self.origin.id).then_some(self.original_id);
            self.transfer(target, pointer, restore_id, cx)?;
            self.detached = false;
        }
        let current = self.current.clone();
        let local = current.handle.update(cx, |_, window, _| {
            let local = client_pointer(current.hwnd(), pointer, window.scale_factor());
            (
                local,
                f32::from(window.viewport_size().width) - TerminalApp::tab_inset(),
            )
        })?;
        if !self.detached && outside_strip(local.0, local.1) {
            let target = if let Some(floating) = &self.floating {
                floating.clone()
            } else {
                let mut geometry = self.geometry.clone();
                geometry.maximized = false;
                geometry.x = None;
                geometry.y = None;
                let root = windows::open(Startup::Empty, geometry, None, cx)?;
                let entry = cx
                    .global::<windows::Windows>()
                    .entries
                    .iter()
                    .find(|entry| entry.root.upgrade().as_ref() == Some(&root))
                    .expect("new drag window is registered")
                    .clone();
                self.floating = Some(entry.clone());
                entry
            };
            if target.id != self.current.id {
                self.transfer(target, pointer, None, cx)?;
            }
            self.detached = true;
        }
        if self.detached {
            self.move_floating(pointer, cx)?;
        }
        let current = self.current.clone();
        current.handle.update(cx, |app, window, cx| {
            let local = client_pointer(current.hwnd(), pointer, window.scale_factor());
            if let Some(drag) = &mut app.tab_drag {
                drag.pointer = local;
            }
            app.update_tab_drag(window);
            cx.notify();
        })?;
        Ok(())
    }

    fn target(&self, pointer: POINT, cx: &mut App) -> Option<windows::Entry> {
        // Walk actual z-order, stopping at unrelated windows. Ignore only the
        // detached preview, so it can merge with the tab strip underneath it.
        let ignored = self.detached.then_some(self.current.hwnd());
        let mut hwnd = unsafe { GetTopWindow(std::ptr::null_mut()) };
        while !hwnd.is_null() {
            let mut rect = RECT::default();
            if Some(hwnd as isize) != ignored
                && unsafe { IsWindowVisible(hwnd) } != 0
                && unsafe { GetWindowRect(hwnd, &mut rect) } != 0
                && contains(rect, pointer)
            {
                let entry = cx
                    .global::<windows::Windows>()
                    .entries
                    .iter()
                    .find(|entry| entry.hwnd() == hwnd as isize)?
                    .clone();
                let accepts = entry
                    .handle
                    .update(cx, |app, window, _| {
                        if app.closing
                            || app.confirm
                            || app.tab_rename.is_some()
                            || (self.id == 0
                                && entry.id != self.current.id
                                && app.tabs.iter().any(|tab| tab.id == 0))
                        {
                            return false;
                        }
                        let local = client_pointer(entry.hwnd(), pointer, window.scale_factor());
                        let (viewport, _, _) = app.tab_dimensions(window);
                        over_strip(local, viewport)
                    })
                    .unwrap_or(false);
                return accepts.then_some(entry);
            }
            hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) };
        }
        None
    }

    fn transfer(
        &mut self,
        target: windows::Entry,
        pointer: POINT,
        restore_id: Option<u64>,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        // Validate before taking ownership; never restart or shut down a session.
        target.handle.update(cx, |_, _, _| ())?;
        let current = self.current.clone();
        let old_id = self.id;
        let mut tab = Some(current.handle.update(cx, |app, window, cx| {
            let index = app
                .tabs
                .iter()
                .position(|tab| tab.id == old_id)
                .ok_or_else(|| anyhow::anyhow!("dragged tab no longer exists"))?;
            let positions = app.tab_positions(window);
            if let Some(terminal) = &app.tabs[index].terminal {
                terminal.update(cx, |terminal, cx| terminal.detach_window(window, cx));
            }
            window.blur(cx);
            let settings = app.tabs[index]
                .terminal
                .is_none()
                .then(|| SettingsTab::capture(app, cx));
            let mut tab = app.tabs.remove(index);
            tab._subscription = None;
            app.tab_drag = None;
            app.tab_motions.clear();
            app.tab_menu = None;
            app.search_visible = false;
            app.animate_tab_relayout(positions, window);
            if !app.tabs.is_empty() {
                let active = app
                    .active
                    .saturating_sub(usize::from(index < app.active))
                    .min(app.tabs.len() - 1);
                app.activate(active, window, cx);
            }
            cx.notify();
            Ok::<_, anyhow::Error>((tab, settings))
        })??);
        let result = target.handle.update(cx, |app, window, cx| {
            let (tab, settings) = tab.take().expect("tab has a single owner");
            if let Some(settings) = settings {
                settings.restore(app, window, cx);
            }
            let local = client_pointer(target.hwnd(), pointer, window.scale_factor());
            app.accept_dragged_tab(tab, local, self.grab_fraction, restore_id, window, cx)
        });
        match result {
            Ok(id) => self.id = id,
            Err(error) => {
                if let Some((tab, settings)) = tab {
                    let _ = current.handle.update(cx, |app, window, cx| {
                        if let Some(settings) = settings {
                            settings.restore(app, window, cx);
                        }
                        app.accept_dragged_tab(
                            tab,
                            point(px(0.), px(18.)),
                            self.grab_fraction,
                            Some(old_id),
                            window,
                            cx,
                        );
                    });
                }
                return Err(error);
            }
        }
        self.current = target.clone();
        if !self.touched.iter().any(|entry| entry.id == target.id) {
            self.touched.push(target.clone());
        }
        unsafe {
            ShowWindow(target.hwnd() as HWND, SW_SHOWNOACTIVATE);
        }
        let _ = target
            .handle
            .update(cx, |_, window, _| window.activate_window());
        unsafe {
            SetCapture(target.hwnd() as HWND);
        }
        let _ = current.handle.update(cx, |app, _, _| {
            if app.tabs.is_empty() {
                unsafe {
                    ShowWindow(current.hwnd() as HWND, SW_HIDE);
                }
            }
        });
        Ok(())
    }

    fn move_floating(&self, pointer: POINT, cx: &mut App) -> anyhow::Result<()> {
        let current = &self.current;
        current.handle.update(cx, |app, window, _| {
            let (_, width, _) = app.tab_dimensions(window);
            let scale = window.scale_factor();
            let mut rect = RECT::default();
            let mut origin = POINT::default();
            unsafe {
                if GetWindowRect(current.hwnd() as HWND, &mut rect) == 0
                    || ClientToScreen(current.hwnd() as HWND, &mut origin) == 0
                {
                    return;
                }
                SetWindowPos(
                    current.hwnd() as HWND,
                    HWND_TOP,
                    pointer.x
                        - ((TerminalApp::tab_inset() + width * self.grab_fraction) * scale) as i32
                        - (origin.x - rect.left),
                    pointer.y - (self.grab_y * scale) as i32 - (origin.y - rect.top),
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        })?;
        Ok(())
    }

    pub(super) fn finish(mut self, cancel: bool, cx: &mut App) {
        if cancel
            && self.current.id != self.origin.id
            && let Err(error) = self.transfer(
                self.origin.clone(),
                POINT::default(),
                Some(self.original_id),
                cx,
            )
        {
            eprintln!("Restore tab after cancelled drag: {error:#}");
        }
        let restored = cancel && self.current.id == self.origin.id;
        let current = self.current.clone();
        let _ = current.handle.update(cx, |app, window, cx| {
            if cancel {
                app.tab_drag = None;
                app.tab_motions.clear();
            } else {
                app.settle_transferred_tab(window);
            }
            if app.tabs.is_empty() {
                return;
            }
            if restored {
                app.tabs.sort_by_key(|tab| {
                    self.original_order
                        .iter()
                        .position(|id| *id == tab.id)
                        .unwrap_or(usize::MAX)
                });
                let active = app
                    .tabs
                    .iter()
                    .position(|tab| tab.id == self.original_id)
                    .unwrap_or(0);
                app.activate(active, window, cx);
                app.tab_scroll.set_offset(self.original_scroll);
            } else {
                app.restore_focus(window, cx);
            }
            window.activate_window();
            cx.notify();
        });
        unsafe {
            ReleaseCapture();
        }
        // Keep empty origins alive until Escape is no longer possible. Removing
        // them now avoids prompting to close a terminal that has already moved.
        for entry in &self.touched {
            let _ = entry.handle.update(cx, |app, window, cx| {
                app.tab_drag = None;
                if app.tabs.is_empty() {
                    app.closing = true;
                    window.remove_window();
                }
                cx.notify();
            });
        }
        let _ = current.handle.update(cx, |app, window, cx| {
            if !app.tabs.is_empty() {
                app.persist(window, cx);
            }
        });
    }
}

impl TerminalApp {
    fn accept_dragged_tab(
        &mut self,
        mut tab: Tab,
        pointer: Point<Pixels>,
        grab_fraction: f32,
        restore_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        tab.id = restore_id.unwrap_or_else(|| {
            if self.tabs.iter().any(|existing| existing.id == tab.id) {
                self.next_id
            } else {
                tab.id
            }
        });
        self.next_id = self.next_id.max(tab.id + 1);
        if let Some(terminal) = &tab.terminal {
            terminal.update(cx, |terminal, cx| terminal.attach_window(window, cx));
            tab._subscription = Some(self.subscribe_terminal(tab.id, terminal, window, cx));
        }
        let positions = self.tab_positions(window);
        let (_, width, _) = self.tab_dimensions(window);
        let position =
            ((f32::from(pointer.x) - Self::tab_inset() - f32::from(self.tab_scroll.offset().x))
                / width)
                .floor()
                .max(0.) as usize;
        let position = position.min(self.tabs.len());
        let id = tab.id;
        self.tabs.insert(position, tab);
        self.activate(position, window, cx);
        let (_, width, _) = self.tab_dimensions(window);
        self.tab_drag = Some(TabDrag {
            id,
            started: true,
            start: pointer,
            pointer,
            grab_offset: width * grab_fraction,
            original_order: self.tabs.iter().map(|tab| tab.id).collect(),
            original_scroll: self.tab_scroll.offset(),
            last_frame: Instant::now(),
        });
        self.animate_tab_relayout(positions, window);
        id
    }
}

fn client_pointer(hwnd: isize, mut pointer: POINT, scale: f32) -> Point<Pixels> {
    unsafe {
        ScreenToClient(hwnd as HWND, &mut pointer);
    }
    point(px(pointer.x as f32 / scale), px(pointer.y as f32 / scale))
}

fn contains(rect: RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

fn over_strip(pointer: Point<Pixels>, viewport: f32) -> bool {
    let x = f32::from(pointer.x) - TerminalApp::tab_inset();
    let y = f32::from(pointer.y);
    x >= 0. && x <= viewport && (0. ..36.).contains(&y)
}

fn outside_strip(pointer: Point<Pixels>, viewport: f32) -> bool {
    let x = f32::from(pointer.x) - TerminalApp::tab_inset();
    let y = f32::from(pointer.y);
    x < -24. || x > viewport + 24. || !(-24. ..60.).contains(&y)
}

#[cfg(test)]
mod tests {
    use super::{outside_strip, over_strip};
    use crate::app::TerminalApp;
    use gpui::{point, px};

    #[test]
    fn detach_hysteresis_and_drop_bounds() {
        let pointer = |x, y| point(px(TerminalApp::tab_inset() + x), px(y));
        assert!(over_strip(pointer(100., 18.), 800.));
        assert!(!over_strip(pointer(100., 37.), 800.));
        assert!(!outside_strip(pointer(100., 50.), 800.));
        assert!(outside_strip(pointer(100., 80.), 800.));
        assert!(outside_strip(pointer(-30., 18.), 800.));
        assert!(outside_strip(pointer(830., 18.), 800.));
    }
}

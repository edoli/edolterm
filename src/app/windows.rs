//! One GPUI application owns every terminal window and routes external launches.
use super::*;
use crate::instance::{Request, select_window};
use std::collections::VecDeque;

pub(super) struct SharedSettings(pub Settings);
impl Global for SharedSettings {}

#[derive(Clone)]
pub(super) struct Entry {
    pub id: u64,
    pub handle: WindowHandle<TerminalApp>,
    pub root: WeakEntity<TerminalApp>,
    hwnd: isize,
    pub activation: u64,
}
pub(super) struct Windows {
    pub entries: Vec<Entry>,
    pub dir: PathBuf,
    detected: Vec<Profile>,
    geometry: SavedSession,
    sequence: u64,
    pub preserve_session: bool,
    recent_requests: VecDeque<(String, Result<(), String>)>,
    #[cfg(windows)]
    pub _host: Option<crate::instance::Host>,
}
impl Global for Windows {}
impl Windows {
    pub fn new(dir: PathBuf, detected: Vec<Profile>, geometry: SavedSession) -> Self {
        Self {
            entries: Vec::new(),
            dir,
            detected,
            geometry,
            sequence: 0,
            preserve_session: false,
            recent_requests: VecDeque::new(),
            #[cfg(windows)]
            _host: None,
        }
    }
}

pub(super) fn activated(window: &Window, cx: &mut App) {
    if let Some(windows) = cx.try_global::<Windows>() {
        let id = window.window_handle().window_id().as_u64();
        if !windows.entries.iter().any(|entry| entry.id == id) {
            return;
        }
        let windows = cx.global_mut::<Windows>();
        windows.sequence = windows.sequence.saturating_add(1);
        let sequence = windows.sequence;
        if let Some(entry) = windows.entries.iter_mut().find(|entry| entry.id == id) {
            entry.activation = sequence;
        }
    }
}

pub(super) fn open(
    startup: Startup,
    saved: SavedSession,
    error: Option<String>,
    cx: &mut App,
) -> anyhow::Result<Entity<TerminalApp>> {
    let registry = cx.global::<Windows>();
    let dir = registry.dir.clone();
    let detected = registry.detected.clone();
    let settings = cx.global::<SharedSettings>().0.clone();
    let restore_size = size(
        px(if saved.width.is_finite() && saved.width >= 560. {
            saved.width.min(5000.)
        } else {
            1100.
        }),
        px(if saved.height.is_finite() && saved.height >= 320. {
            saved.height.min(3000.)
        } else {
            720.
        }),
    );
    let mut bounds = Bounds::centered(None, restore_size, cx);
    if let (Some(x), Some(y)) = (saved.x, saved.y)
        && x.is_finite()
        && y.is_finite()
    {
        let candidate = Bounds::new(point(px(x), px(y)), restore_size);
        if cx
            .displays()
            .iter()
            .any(|display| display.bounds().intersects(&candidate))
        {
            bounds = candidate;
        }
    }
    let options = WindowOptions {
        window_bounds: Some(if saved.maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        }),
        titlebar: Some(TitlebarOptions {
            title: Some("edolterm".into()),
            appears_transparent: cfg!(windows),
            ..Default::default()
        }),
        window_min_size: Some(size(px(560.), px(320.))),
        app_id: Some("dev.edoli.edolterm".into()),
        ..Default::default()
    };
    let mut created = None;
    let handle = cx.open_window(options, |window, cx| {
        let root =
            cx.new(|cx| TerminalApp::new(dir, settings, detected, startup, error, window, cx));
        let weak = root.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });
        created = Some((root.clone(), native_handle(window)));
        root
    })?;
    let (root, hwnd) = created.expect("window constructor ran");
    let registry = cx.global_mut::<Windows>();
    registry.sequence = registry.sequence.saturating_add(1);
    registry.entries.push(Entry {
        id: handle.window_id().as_u64(),
        handle,
        root: root.downgrade(),
        hwnd,
        activation: registry.sequence,
    });
    handle.update(cx, |_, window, _| {
        restore(hwnd);
        window.activate_window();
    })?;
    Ok(root)
}

pub(super) fn open_default(cx: &mut App) -> anyhow::Result<Entity<TerminalApp>> {
    let mut geometry = cx.global::<Windows>().geometry.clone();
    // New windows use the saved size, but start centered on the current display.
    geometry.x = None;
    geometry.y = None;
    open(Startup::Default, geometry, None, cx)
}

pub(super) fn route(request: &Request, cx: &mut App) -> Result<(), String> {
    if let Some((_, reply)) = cx
        .global::<Windows>()
        .recent_requests
        .iter()
        .find(|(id, _)| id == &request.id)
    {
        return reply.clone();
    }
    let result = route_inner(request, cx);
    let registry = cx.global_mut::<Windows>();
    registry
        .recent_requests
        .push_back((request.id.clone(), result.clone()));
    while registry.recent_requests.len() > 128 {
        registry.recent_requests.pop_front();
    }
    result
}
fn route_inner(request: &Request, cx: &mut App) -> Result<(), String> {
    request.validate()?;
    if cx.global::<Windows>().preserve_session {
        return Err("Edolterm을 다시 시작하고 있습니다. 잠시 후 다시 시도하세요.".into());
    }
    let entries = cx.global::<Windows>().entries.clone();
    let behavior = cx.global::<SharedSettings>().0.windowing_behavior;
    let target = select_window(
        behavior,
        request.new_window,
        entries.iter().map(|entry| {
            let closing = entry
                .root
                .upgrade()
                .is_none_or(|root| root.read(cx).closing);
            (
                entry.id,
                entry.activation,
                current_desktop(entry.hwnd),
                closing,
            )
        }),
    );
    if let Some(entry) = entries.iter().find(|entry| Some(entry.id) == target) {
        let directory = request.directory.clone();
        entry
            .handle
            .update(cx, |root, window, cx| {
                root.add_terminal(root.default_profile(), directory, window, cx);
                restore(entry.hwnd);
                window.activate_window();
            })
            .map_err(|error| error.to_string())?;
    } else {
        let mut geometry = cx.global::<Windows>().geometry.clone();
        geometry.x = None;
        geometry.y = None;
        let startup = request
            .directory
            .clone()
            .map_or(Startup::Default, Startup::Directory);
        open(startup, geometry, None, cx).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn save_session(current: SavedSession, current_id: u64, cx: &App) -> anyhow::Result<()> {
    let Some(registry) = cx.try_global::<Windows>() else {
        return current.save(&crate::config::directory());
    };
    if registry.preserve_session {
        return Ok(());
    }
    let mut snapshots = Vec::new();
    let mut active_window = 0;
    for entry in &registry.entries {
        if entry.id == current_id {
            active_window = snapshots.len();
            snapshots.push(current.clone());
        } else if let Some(root) = entry.root.upgrade() {
            let app = root.read(cx);
            if !app.closing {
                snapshots.push(app.snapshot(app.last_bounds, cx));
            }
        }
    }
    if snapshots.is_empty() {
        snapshots.push(current.clone());
    }
    let mut session = current;
    session.windows = snapshots;
    session.active_window = active_window;
    session.save(&registry.dir)
}

pub(super) fn native_handle(window: &Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
        _ => 0,
    }
}
pub(super) fn current_desktop(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        use ::windows::Win32::{
            Foundation::HWND,
            System::Com::{CLSCTX_ALL, CoCreateInstance},
            UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager},
        };
        let manager: ::windows::core::Result<IVirtualDesktopManager> =
            unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) };
        manager
            .and_then(|manager| unsafe { manager.IsWindowOnCurrentVirtualDesktop(HWND(hwnd as _)) })
            .is_ok_and(|value| value.as_bool())
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        true
    }
}
fn restore(hwnd: isize) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, SW_RESTORE, ShowWindow};
        if IsIconic(hwnd as _) != 0 {
            ShowWindow(hwnd as _, SW_RESTORE);
        }
    }
    #[cfg(not(windows))]
    let _ = hwnd;
}

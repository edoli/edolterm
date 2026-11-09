use crate::{
    config::{self, Profile, SavedSession, SavedTab, Settings},
    icons::{self, Icon, icon},
    platform,
    terminal_view::{TerminalEvent, TerminalView},
    text_field::{FieldEvent, TextField},
    theme::Theme,
};
use gpui::{prelude::*, *};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(debug_assertions)]
mod smoke;
mod tabs;
mod shortcuts;
mod windows;
use tabs::{TabDrag, TabMotion};
use shortcuts::SettingsPage;
use config::shortcuts::{self as keybindings, Bindings};

struct Tab {
    id: u64,
    terminal: Option<Entity<TerminalView>>,
    _subscription: Option<Subscription>,
}

enum Startup {
    Restore(SavedSession),
    Directory(PathBuf),
    Default,
}

struct Tip(String);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(rgb(0x343b47))
            .text_color(rgb(0xffffff))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}
fn tip(text: String) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}

pub struct TerminalApp {
    dir: PathBuf,
    settings: Settings,
    draft: Settings,
    detected: Vec<Profile>,
    profiles: Vec<Profile>,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    focus: FocusHandle,
    tab_scroll: ScrollHandle,
    tab_scroll_drag: bool,
    tab_drag: Option<TabDrag>,
    tab_motions: Vec<TabMotion>,
    menu: bool,
    menu_index: usize,
    confirm: bool,
    closing: bool,
    status: Option<String>,
    config_error: bool,
    search_visible: bool,
    search: Entity<TextField>,
    font_input: Entity<TextField>,
    history_input: Entity<TextField>,
    _subscriptions: Vec<Subscription>,
    settings_scroll: ScrollHandle,
    shortcuts_scroll: ScrollHandle,
    settings_page: SettingsPage,
    shortcut_capture: Option<(&'static str, Option<usize>)>,
    shortcut_bindings: Bindings,
    update_state: crate::update::State,
    #[cfg(windows)]
    update_handoff: Option<crate::update::Handoff>,
    apply_update: bool,
    last_bounds: WindowBounds,
}

impl TerminalApp {
    fn new(
        dir: PathBuf,
        settings: Settings,
        detected: Vec<Profile>,
        startup: Startup,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| TextField::new("", "터미널에서 찾기", cx));
        let font_input =
            cx.new(|cx| TextField::new(settings.font_family.clone(), "고정폭 글꼴 이름", cx));
        let history_input =
            cx.new(|cx| TextField::new(settings.scrollback.to_string(), "10000", cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &search,
                window,
                |this, _, e: &FieldEvent, window, cx| match e {
                    FieldEvent::Changed if this.search_visible => this.find(false, false, cx),
                    FieldEvent::Changed => {}
                    FieldEvent::Submit => this.find(false, true, cx),
                    FieldEvent::Cancel => this.hide_search(window, cx),
                },
            ),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    windows::activated(window, cx);
                }
                if !window.is_window_active() {
                    this.finish_tab_drag(true, window, cx);
                }
                cx.notify();
            }),
            cx.observe_window_bounds(window, |this, window, _| {
                this.last_bounds = window.window_bounds()
            }),
            cx.observe_window_appearance(window, |this, _, cx| {
                if this.settings.theme == "system" {
                    cx.notify();
                }
            }),
            cx.observe_global::<windows::SharedSettings>(|this, cx| {
                let settings = cx.global::<windows::SharedSettings>().0.clone();
                if this.settings != settings {
                    this.settings = settings.clone();
                    this.draft = settings;
                    this.shortcut_bindings = Bindings::new(&this.settings.keybindings).expect("validated settings");
                    this.shortcut_capture = None;
                    this.font_input = cx.new(|cx| {
                        TextField::new(this.settings.font_family.clone(), "고정폭 글꼴", cx)
                    });
                    this.history_input = cx.new(|cx| {
                        TextField::new(this.settings.scrollback.to_string(), "10000", cx)
                    });
                    this.profiles =
                        platform::merge_profiles(this.detected.clone(), &this.settings.profiles);
                    for tab in &this.tabs {
                        if let Some(terminal) = &tab.terminal {
                            terminal.update(cx, |terminal, cx| {
                                terminal.apply_settings(&this.settings, cx)
                            });
                        }
                    }
                    cx.notify();
                }
            }),
        ];
        let profiles = platform::merge_profiles(detected.clone(), &settings.profiles);
        let shortcut_bindings = Bindings::new(&settings.keybindings).expect("validated settings");
        let mut this = Self {
            dir: dir.clone(),
            draft: settings.clone(),
            settings,
            detected,
            profiles,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            focus: cx.focus_handle(),
            tab_scroll: ScrollHandle::new(),
            tab_scroll_drag: false,
            tab_drag: None,
            tab_motions: Vec::new(),
            menu: false,
            menu_index: 0,
            confirm: false,
            closing: false,
            status: error.clone(),
            config_error: error.is_some(),
            search_visible: false,
            search,
            font_input,
            history_input,
            _subscriptions: subscriptions,
            settings_scroll: ScrollHandle::new(),
            shortcuts_scroll: ScrollHandle::new(),
            settings_page: SettingsPage::General,
            shortcut_capture: None,
            shortcut_bindings,
            update_state: crate::update::read_result(&dir)
                .map(crate::update::State::Failed)
                .unwrap_or_default(),
            #[cfg(windows)]
            update_handoff: None,
            apply_update: false,
            last_bounds: window.window_bounds(),
        };
        if let Startup::Directory(directory) = startup {
            this.add_terminal(this.default_profile(), Some(directory), window, cx);
        } else if let Startup::Restore(saved) = startup
            && this.settings.restore_session
        {
            for tab in saved.tabs.into_iter().take(50) {
                if let Some(profile) = this
                    .profiles
                    .iter()
                    .find(|p| p.id == tab.profile_id)
                    .cloned()
                {
                    this.add_terminal(profile, Some(tab.cwd), window, cx);
                }
            }
            if !this.tabs.is_empty() {
                this.activate(saved.active.min(this.tabs.len() - 1), window, cx);
            }
        }
        if this.tabs.is_empty() {
            this.new_default(window, cx);
        }
        this
    }

    fn active_terminal(&self) -> Option<Entity<TerminalView>> {
        self.tabs.get(self.active)?.terminal.clone()
    }
    fn default_profile(&self) -> Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.settings.default_profile)
            .or_else(|| self.profiles.first())
            .cloned()
            .unwrap_or_else(|| {
                Profile::new(
                    "shell",
                    "Shell",
                    if cfg!(windows) {
                        "powershell.exe"
                    } else {
                        "/bin/sh"
                    },
                    &[],
                )
            })
    }
    fn new_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_terminal(self.default_profile(), None, window, cx);
    }
    fn add_terminal(
        &mut self,
        profile: Profile,
        cwd: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let terminal =
            cx.new(|cx| TerminalView::new(profile, cwd, self.settings.clone(), window, cx));
        let subscription = cx.subscribe_in(
            &terminal,
            window,
            move |this, terminal, e: &TerminalEvent, window, cx| {
                if matches!(e, TerminalEvent::Exited(true)) {
                    this.close_tab(id, window, cx);
                } else {
                    if this.tabs.get(this.active).is_some_and(|t| t.id == id) {
                        window.set_window_title(&format!("{} — edolterm", terminal.read(cx).title));
                    }
                    cx.notify();
                }
            },
        );
        self.tabs.push(Tab {
            id,
            terminal: Some(terminal),
            _subscription: Some(subscription),
        });
        self.menu = false;
        self.activate(self.tabs.len() - 1, window, cx);
    }
    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        self.shortcut_capture = None;
        self.search_visible = false;
        self.menu = false;
        self.tab_scroll.scroll_to_item(index);
        if let Some(terminal) = self.active_terminal() {
            terminal.read(cx).focus.clone().focus(window, cx);
            window.set_window_title(&format!("{} — edolterm", terminal.read(cx).title));
        } else {
            self.focus.focus(window, cx);
            window.set_window_title("설정 — edolterm");
        }
        cx.notify();
    }
    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_tab_drag(true, window, cx);
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.close_window(window, cx);
            return;
        }
        let active = if self.active > index {
            self.active - 1
        } else {
            self.active.min(self.tabs.len() - 1)
        };
        self.activate(active, window, cx);
    }
    fn settings_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.tabs.iter().position(|t| t.id == 0).unwrap_or_else(|| {
            self.tabs.push(Tab {
                id: 0,
                terminal: None,
                _subscription: None,
            });
            self.tabs.len() - 1
        });
        self.activate(index, window, cx);
    }
    #[cfg(debug_assertions)]
    fn reorder(&mut self, id: u64, target: usize, window: &mut Window, cx: &mut Context<Self>) {
        let active_id = self.tabs[self.active].id;
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let tab = self.tabs.remove(from);
        let target = target.min(self.tabs.len());
        self.tabs.insert(target, tab);
        let index = self
            .tabs
            .iter()
            .position(|t| t.id == active_id)
            .unwrap_or(0);
        self.activate(index, window, cx);
    }

    pub fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.closing {
            return false;
        }
        let active = self
            .tabs
            .iter()
            .filter_map(|t| t.terminal.as_ref())
            .any(|t| !t.read(cx).exited && t.read(cx).session.is_some());
        let other_active = self.apply_update
            && cx.try_global::<windows::Windows>().is_some_and(|registry| {
                registry
                    .entries
                    .iter()
                    .filter(|entry| entry.id != window.window_handle().window_id().as_u64())
                    .any(|entry| {
                        entry.root.upgrade().is_some_and(|root| {
                            root.read(cx)
                                .tabs
                                .iter()
                                .filter_map(|tab| tab.terminal.as_ref())
                                .any(|terminal| {
                                    !terminal.read(cx).exited && terminal.read(cx).session.is_some()
                                })
                        })
                    })
            });
        if self.settings.confirm_close && (active || other_active) {
            self.confirm = true;
            self.menu = false;
            self.focus.focus(window, cx);
            cx.notify();
        } else {
            self.close_window(window, cx);
        }
        false
    }

    fn snapshot(&self, bounds: WindowBounds, cx: &App) -> SavedSession {
        let b = bounds.get_bounds();
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let t = tab.terminal.as_ref()?.read(cx);
                let s = t.session.as_ref()?;
                Some(SavedTab {
                    profile_id: t.profile.id.clone(),
                    cwd: s.working_directory(),
                })
            })
            .collect();
        let active = self
            .tabs
            .iter()
            .take(self.active)
            .filter(|t| t.terminal.is_some())
            .count()
            .min(tabs.len().saturating_sub(1));
        SavedSession {
            tabs: if self.settings.restore_session {
                tabs
            } else {
                Vec::new()
            },
            active,
            x: Some(b.origin.x.into()),
            y: Some(b.origin.y.into()),
            width: b.size.width.into(),
            height: b.size.height.into(),
            maximized: matches!(bounds, WindowBounds::Maximized(_)),
            windows: Vec::new(),
            active_window: 0,
        }
    }

    fn persist(&mut self, window: &Window, cx: &App) -> bool {
        self.last_bounds = window.window_bounds();
        let saved = self.snapshot(self.last_bounds, cx);
        let result = if cx.try_global::<windows::Windows>().is_some() {
            windows::save_session(saved, window.window_handle().window_id().as_u64(), cx)
        } else {
            saved.save(&self.dir)
        };
        if let Err(e) = result {
            eprintln!("{e:#}");
            self.status = Some(format!("{e:#}"));
            return false;
        }
        true
    }
    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.confirm = false;
        if !self.persist(window, cx) {
            self.closing = false;
            self.apply_update = false;
            cx.notify();
            return;
        }
        #[cfg(windows)]
        if self.apply_update {
            let result = self
                .update_handoff
                .take()
                .ok_or_else(|| anyhow::anyhow!("Update helper is not ready"))
                .and_then(|handoff| handoff.commit().map_err(Into::into));
            if let Err(error) = result {
                self.closing = false;
                self.apply_update = false;
                self.update_state = crate::update::State::Failed(error.to_string());
                cx.notify();
                return;
            }
            // Keep the complete snapshot while every window exits for the updater.
            let entries = cx.global::<windows::Windows>().entries.clone();
            cx.global_mut::<windows::Windows>().preserve_session = true;
            let current = window.window_handle().window_id().as_u64();
            for entry in entries.iter().filter(|entry| entry.id != current) {
                let _ = entry
                    .handle
                    .update(cx, |app, window, cx| app.close_window(window, cx));
            }
        }
        let receivers: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|t| t.terminal.as_ref())
            .filter_map(|t| t.read(cx).session.as_ref())
            .map(|s| {
                s.shutdown();
                s.completed.clone()
            })
            .collect();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let start = Instant::now();
            while receivers.iter().any(|r| r.is_empty() && !r.is_closed())
                && start.elapsed() < Duration::from_secs(3)
            {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
            }
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        })
        .detach();
        cx.notify();
    }

    fn show_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_terminal().is_none() {
            return;
        }
        self.menu = false;
        self.search_visible = true;
        self.search
            .update(cx, |field, cx| field.select_all(window, cx));
        cx.notify();
    }
    fn hide_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_visible = false;
        if let Some(t) = self.active_terminal() {
            t.update(cx, |t, cx| {
                t.search_match = None;
                t.focus.focus(window, cx);
                cx.notify();
            });
        }
        cx.notify();
    }
    fn find(&mut self, backwards: bool, advance: bool, cx: &mut Context<Self>) {
        let text = self.search.read(cx).text.clone();
        if let Some(t) = self.active_terminal() {
            t.update(cx, |t, cx| t.search(&text, backwards, advance, cx));
        }
        cx.notify();
    }
    fn reset_general_settings(&mut self, cx: &mut Context<Self>) {
        self.draft = Settings {
            profiles: self.draft.profiles.clone(),
            keybindings: self.draft.keybindings.clone(),
            allow_osc52: self.draft.allow_osc52,
            ..Settings::default()
        };
        self.font_input =
            cx.new(|cx| TextField::new(self.draft.font_family.clone(), "고정폭 글꼴", cx));
        self.history_input =
            cx.new(|cx| TextField::new(self.draft.scrollback.to_string(), "10000", cx));
        self.status = Some("일반 설정을 기본값으로 되돌렸습니다. 저장하면 적용됩니다.".into());
        cx.notify();
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = Bindings::new(&self.draft.keybindings)
            .and_then(|bindings| bindings.validate_layout(cx.keyboard_mapper().as_ref()))
        {
            self.status = Some(error.to_string());
            cx.notify();
            return;
        }
        self.draft.font_family = self.font_input.read(cx).text.trim().into();
        let history = self.history_input.read(cx).text.trim().parse();
        let Ok(history) = history else {
            self.status = Some("스크롤백에 0–1000000 사이의 정수를 입력하세요.".into());
            cx.notify();
            return;
        };
        self.draft.scrollback = history;
        match self.draft.save(&self.dir) {
            Ok(()) => {
                self.settings = self.draft.clone();
                self.shortcut_bindings = Bindings::new(&self.settings.keybindings).expect("validated settings");
                self.shortcut_capture = None;
                cx.set_global(windows::SharedSettings(self.settings.clone()));
                self.config_error = false;
                self.profiles =
                    platform::merge_profiles(self.detected.clone(), &self.settings.profiles);
                for tab in &self.tabs {
                    if let Some(t) = &tab.terminal {
                        t.update(cx, |t, cx| t.apply_settings(&self.settings, cx));
                    }
                }
                self.status = Some("설정을 저장했습니다.".into());
            }
            Err(e) => self.status = Some(format!("{e:#}")),
        }
        cx.notify();
    }
    fn reload_settings(&mut self, cx: &mut Context<Self>) {
        match Settings::load(&self.dir) {
            Ok(settings) => {
                cx.set_global(windows::SharedSettings(settings.clone()));
                self.settings = settings.clone();
                self.draft = settings;
                self.shortcut_bindings = Bindings::new(&self.settings.keybindings).expect("validated settings");
                self.shortcut_capture = None;
                self.font_input = cx
                    .new(|cx| TextField::new(self.settings.font_family.clone(), "고정폭 글꼴", cx));
                self.history_input =
                    cx.new(|cx| TextField::new(self.settings.scrollback.to_string(), "10000", cx));
                self.profiles =
                    platform::merge_profiles(self.detected.clone(), &self.settings.profiles);
                for tab in &self.tabs {
                    if let Some(t) = &tab.terminal {
                        t.update(cx, |t, cx| t.apply_settings(&self.settings, cx));
                    }
                }
                self.config_error = false;
                self.status = Some("설정을 다시 불러왔습니다.".into());
            }
            Err(e) => {
                self.config_error = true;
                self.status = Some(format!("{e:#}"));
            }
        }
        cx.notify();
    }

    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let m = k.modifiers;
        if self.tab_drag.as_ref().is_some_and(|drag| drag.started) {
            if k.key == "escape" {
                self.finish_tab_drag(true, window, cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.closing {
            cx.stop_propagation();
            return;
        }
        if self.shortcut_capture.is_some() && !self.confirm {
            self.capture_shortcut(k, cx);
            window.prevent_default();
            cx.stop_propagation();
            return;
        }
        if self.confirm {
            if k.key == "escape" {
                self.confirm = false;
                self.apply_update = false;
                self.restore_focus(window, cx);
            } else if k.key == "enter" {
                self.close_window(window, cx);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.menu && !m.control && !m.alt {
            match k.key.as_str() {
                "escape" => {
                    self.menu = false;
                    self.restore_focus(window, cx);
                }
                "up" => {
                    self.menu_index =
                        (self.menu_index + self.profiles.len()) % (self.profiles.len() + 1)
                }
                "down" => self.menu_index = (self.menu_index + 1) % (self.profiles.len() + 1),
                "enter" => {
                    if let Some(p) = self.profiles.get(self.menu_index).cloned() {
                        self.add_terminal(p, None, window, cx);
                    } else {
                        self.settings_tab(window, cx);
                    }
                }
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.search_visible && self.search.read(cx).focus.is_focused(window)
            && k.key == "enter" && m.shift
        {
            self.find(true, true, cx);
        } else if let Some(action) = self.shortcut_bindings.action(k, cx.keyboard_mapper().as_ref()) {
            if !self.run_shortcut(action, window, cx) { return; }
        } else {
            return;
        }
        window.prevent_default();
        cx.stop_propagation();
    }
    fn restore_focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(t) = self.active_terminal() {
            t.read(cx).focus.clone().focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
    }

    fn tab_dimensions(&self, window: &Window) -> (f32, f32, f32) {
        let controls = cfg!(windows) && !window.is_fullscreen();
        let left = if cfg!(target_os = "macos") { 78. } else { 0. };
        let viewport = (f32::from(window.viewport_size().width)
            - 80.
            - 28.
            - left
            - if controls { 138. } else { 0. })
        .max(1.);
        let tab_width = (viewport / self.tabs.len().max(1) as f32).clamp(100., 240.);
        (
            viewport,
            tab_width,
            (tab_width * self.tabs.len() as f32).min(viewport),
        )
    }

    fn tabs_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.animate_tabs(window, cx);
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let controls = cfg!(windows) && !window.is_fullscreen();
        let left = if cfg!(target_os = "macos") { 78. } else { 0. };
        let (viewport, tab_width, tabs_extent) = self.tab_dimensions(window);
        let mut strip = div()
            .id("tab-strip")
            .flex()
            .h_full()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll)
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| {
                let scroll = &this.tab_scroll;
                let delta = e.delta.pixel_delta(px(36.));
                let x = f32::from(scroll.offset().x)
                    + if delta.x == px(0.) {
                        f32::from(delta.y)
                    } else {
                        f32::from(delta.x)
                    };
                scroll.set_offset(point(
                    px(x.clamp(-f32::from(scroll.max_offset().x), 0.)),
                    px(0.),
                ));
                cx.stop_propagation();
                cx.notify();
            }));
        let dragging = self
            .tab_drag
            .as_ref()
            .filter(|drag| drag.started)
            .map(|drag| drag.id);
        for (index, tab) in self.tabs.iter().enumerate() {
            if dragging == Some(tab.id) {
                strip = strip.child(
                    div()
                        .w(px(tab_width))
                        .h_full()
                        .flex_shrink_0()
                        .bg(theme.bar),
                );
            } else {
                strip = strip.child(
                    self.tab_element(tab, index, tab_width, false, cx)
                        .left(px(self.tab_slide_offset(tab.id))),
                );
            }
        }
        let overflow = tab_width * self.tabs.len() as f32 > viewport + 1.;
        let total = tab_width * self.tabs.len() as f32;
        let thumb_width = (viewport * viewport / total.max(1.)).max(24.);
        let max_scroll = (total - viewport).max(1.);
        let thumb_left = (-f32::from(self.tab_scroll.offset().x) / max_scroll).clamp(0., 1.)
            * (viewport - thumb_width);
        let active = self.active;
        let dragged_left = dragging.map(|_| self.dragged_tab_left(window));
        let active_slide = self
            .tabs
            .get(active)
            .map_or(0., |tab| self.tab_slide_offset(tab.id));
        let scroll = self.tab_scroll.clone();
        div()
            .relative()
            .flex()
            .items_center()
            .h(px(36.))
            .flex_shrink_0()
            .bg(theme.bar)
            .when(left > 0., |d| d.child(div().w(px(left)).h_full()))
            .child(
                div()
                    .relative()
                    .w(px(tabs_extent))
                    .h_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(strip)
                    .when_some(dragging, |container, id| {
                        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
                            return container;
                        };
                        container.child(
                            self.tab_element(&self.tabs[index], index, tab_width, true, cx)
                                .absolute()
                                .top_0()
                                .left(px(self.dragged_tab_left(window))),
                        )
                    })
                    .when(overflow, |d| {
                        d.child(
                            div()
                                .id("tab-scrollbar")
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .h(px(5.))
                                .bg(theme.border)
                                .cursor_default()
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(thumb_left))
                                        .w(px(thumb_width))
                                        .h_full()
                                        .bg(theme.muted),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                        this.tab_scroll_drag = true;
                                        let ratio =
                                            ((f32::from(e.position.x) - left - thumb_width / 2.)
                                                / (viewport - thumb_width))
                                                .clamp(0., 1.);
                                        this.tab_scroll
                                            .set_offset(point(px(-ratio * max_scroll), px(0.)));
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .on_mouse_move(cx.listener(
                                    move |this, e: &MouseMoveEvent, _, cx| {
                                        if this.tab_scroll_drag && e.dragging() {
                                            let ratio = ((f32::from(e.position.x)
                                                - left
                                                - thumb_width / 2.)
                                                / (viewport - thumb_width))
                                                .clamp(0., 1.);
                                            this.tab_scroll
                                                .set_offset(point(px(-ratio * max_scroll), px(0.)));
                                            cx.notify();
                                        }
                                    },
                                ))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, _| this.tab_scroll_drag = false),
                                )
                                .on_mouse_up_out(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, _| this.tab_scroll_drag = false),
                                ),
                        )
                    }),
            )
            .child(
                icon_button("new-tab", Icon::Plus, "새 탭", theme)
                    .cursor_default()
                    .w(px(36.))
                    .h_full()
                    .bg(theme.bar)
                    .tooltip(tip(format!("새 탭 · {}", self.shortcut_label(keybindings::Action::NewTab))))
                    .on_click(cx.listener(|this, _, window, cx| this.new_default(window, cx))),
            )
            .child(
                icon_button("profiles", Icon::ChevronDown, "프로필 및 설정", theme)
                    .cursor_default()
                    .w(px(36.))
                    .h_full()
                    .bg(theme.bar)
                    .tooltip(tip(format!("프로필 및 설정 · {}", self.shortcut_label(keybindings::Action::ProfileMenu))))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.menu = !this.menu;
                        this.menu_index = 0;
                        this.focus.focus(window, cx);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(28.))
                    .h_full()
                    .flex_shrink_0()
                    .window_control_area(WindowControlArea::Drag),
            )
            .when(controls, |d| d.child(window_controls(window, theme)))
            // Keep the divider gap under the active tab, including drag/slide motion.
            // Read the scroll offset in prepaint so tab reveal/scroll stays aligned.
            .child(
                canvas(
                    move |_, _, _| {
                        let offset = f32::from(scroll.offset().x);
                        let tab_left = left
                            + dragged_left
                                .unwrap_or(active as f32 * tab_width + offset + active_slide);
                        let start = tab_left.clamp(left, left + tabs_extent);
                        let end = (tab_left + tab_width).clamp(left, left + tabs_extent);
                        (start, end)
                    },
                    move |bounds, (start, end), window, _| {
                        let y = bounds.bottom() - px(1.);
                        if start > 0. {
                            window.paint_quad(fill(
                                Bounds::new(point(bounds.left(), y), size(px(start), px(1.))),
                                theme.border,
                            ));
                        }
                        let remaining = f32::from(bounds.size.width) - end;
                        if remaining > 0. {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(bounds.left() + px(end), y),
                                    size(px(remaining), px(1.)),
                                ),
                                theme.border,
                            ));
                        }
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .size_full(),
            )
            .when(self.tab_drag.is_some(), |bar| {
                bar.child(self.tab_drag_listener(cx))
            })
            .into_any_element()
    }

    fn profile_menu(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let (_, _, tabs_extent) = self.tab_dimensions(window);
        let inset = if cfg!(target_os = "macos") { 78. } else { 0. };
        let menu_left = (inset + tabs_extent + 40.)
            .min(f32::from(window.viewport_size().width) - 348.)
            .max(8.);
        let mut menu = div()
            .id("profile-menu")
            .absolute()
            .top(px(40.))
            .left(px(menu_left))
            .w(px(340.))
            .max_h(px(500.))
            .overflow_y_scroll()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.panel)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let default = self.default_profile().id;
        for (index, p) in self.profiles.iter().enumerate() {
            let profile = p.clone();
            menu = menu.child(
                div()
                    .id(("profile", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .cursor_default()
                    .bg(if self.menu_index == index {
                        theme.hover
                    } else {
                        theme.panel
                    })
                    .hover(|s| s.bg(theme.hover))
                    .child(icon(Icon::Terminal, 16.).text_color(theme.accent))
                    .child(
                        div()
                            .flex_1()
                            .text_color(theme.fg)
                            .when(p.id == default, |d| d.font_weight(FontWeight::BOLD))
                            .child(p.name.clone()),
                    )
                    .when(index < 9, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme.muted)
                                .child(self.shortcut_label(keybindings::Action::NewProfile(index))),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.add_terminal(profile.clone(), None, window, cx)
                    })),
            );
        }
        menu.child(div().h(px(1.)).my_1().bg(theme.border))
            .child(
                div()
                    .id("open-settings")
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .cursor_default()
                    .bg(if self.menu_index == self.profiles.len() {
                        theme.hover
                    } else {
                        theme.panel
                    })
                    .hover(|s| s.bg(theme.hover))
                    .child(icon(Icon::Settings, 16.))
                    .child(div().flex_1().child("설정"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.muted)
                            .child(self.shortcut_label(keybindings::Action::Settings)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.settings_tab(window, cx))),
            )
            .into_any_element()
    }

    fn check_update(&mut self, cx: &mut Context<Self>) {
        if self.update_state.is_busy() || self.closing {
            return;
        }
        #[cfg(windows)]
        {
            self.update_handoff = None;
        }
        self.update_state = crate::update::State::Checking;
        let task = cx
            .background_executor()
            .spawn(async { crate::update::check() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.update_state =
                    result.unwrap_or_else(|error| crate::update::State::Failed(error.to_string()));
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    #[cfg(windows)]
    fn download_update(&mut self, cx: &mut Context<Self>) {
        let crate::update::State::Available(release) = self.update_state.clone() else {
            return;
        };
        if self.closing {
            return;
        }
        self.update_state = crate::update::State::Installing;
        let config = self.dir.clone();
        let task = cx.background_executor().spawn(async move {
            let exe = std::env::current_exe()?;
            crate::update::prepare(&release, &exe, crate::update::install_scope(&exe), &config)
        });
        cx.spawn(async move |this, cx| {
            let result: anyhow::Result<crate::update::Handoff> = task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(handoff) => {
                        this.update_handoff = Some(handoff);
                        this.update_state = crate::update::State::Ready;
                    }
                    Err(error) => {
                        this.update_state = crate::update::State::Failed(error.to_string())
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn update_view(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        use crate::update::State;
        let label: String = match &self.update_state {
            State::Unchecked => "GitHub Releases에서 새 버전을 확인합니다.".into(),
            State::Checking => "업데이트 확인 중…".into(),
            State::Current => "최신 버전입니다.".into(),
            State::NoRelease => "게시된 릴리스가 없습니다.".into(),
            State::Available(release) => format!("{} 버전 사용 가능", release.version),
            State::Installing => "업데이트 다운로드 및 검증 중…".into(),
            State::Ready => "준비 완료. 다시 시작하면 현재 셸이 종료됩니다.".into(),
            State::Failed(error) => format!("업데이트 실패: {error}"),
        };
        let mut row = div()
            .flex()
            .items_center()
            .gap_3()
            .min_w_0()
            .text_size(px(12.))
            .text_color(theme.muted)
            .child(
                div()
                    .flex_shrink_0()
                    .child(format!("edolterm {}", env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .id("update-status")
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip(tip(label.clone()))
                    .child(label),
            );
        if !self.update_state.is_busy() {
            row = row.child(
                button("check-update", "업데이트 확인", theme)
                    .px_3()
                    .py_2()
                    .on_click(cx.listener(|this, _, _, cx| this.check_update(cx))),
            );
        }
        #[cfg(windows)]
        if matches!(self.update_state, State::Available(_)) {
            row = row.child(
                button("download-update", "업데이트 다운로드", theme)
                    .px_3()
                    .py_2()
                    .on_click(cx.listener(|this, _, _, cx| this.download_update(cx))),
            );
        } else if matches!(self.update_state, State::Ready) {
            row = row.child(
                button("apply-update", "다시 시작하여 업데이트", theme)
                    .px_3()
                    .py_2()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_update = true;
                        this.should_close(window, cx);
                    })),
            );
        }
        row.into_any_element()
    }

    fn general_settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let mut profiles = div().flex().flex_wrap().gap_2();
        for (index, p) in self.profiles.iter().enumerate() {
            let id = p.id.clone();
            let selected = self.draft.default_profile == p.id
                || self.draft.default_profile.is_empty() && index == 0;
            profiles = profiles.child(
                button(("default-profile", index), p.name.clone(), theme)
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.draft.default_profile = id.clone();
                        cx.notify();
                    })),
            );
        }
        let behavior = div().flex().flex_col().gap_2().children(
            config::WindowingBehavior::ALL
                .into_iter()
                .enumerate()
                .map(|(index, choice)| {
                    button(("windowing-behavior", index), choice.label(), theme)
                        .justify_start()
                        .px_3()
                        .py_2()
                        .border_1()
                        .border_color(if self.draft.windowing_behavior == choice {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .aria_selected(self.draft.windowing_behavior == choice)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.draft.windowing_behavior = choice;
                            cx.notify();
                        }))
                }),
        );
        let themes = div().flex().gap_2().children(
            [("dark", "다크"), ("light", "라이트"), ("system", "시스템")]
                .into_iter()
                .enumerate()
                .map(|(index, (value, label))| {
                    button(("theme", index), label, theme)
                        .px_4()
                        .py_2()
                        .border_1()
                        .border_color(if self.draft.theme == value { theme.accent } else { theme.border })
                        .aria_selected(self.draft.theme == value)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.draft.theme = value.into();
                            cx.notify();
                        }))
                }),
        );
        let content = div().flex().flex_col().gap_5().w_full().max_w(px(820.)).mx_auto().p_8()
            .child(self.update_view(theme, cx))
            .child(div().text_size(px(25.)).font_weight(FontWeight::BOLD).child("설정"))
            .child(div().text_color(theme.muted).child("설정은 저장하면 열린 터미널에도 적용됩니다."))
            .child(setting("기본 프로필", "새 탭의 + 버튼으로 실행할 셸", profiles))
            .child(setting("새 인스턴스 동작", "탐색기나 바로 가기에서 실행할 때 사용할 창 · 대상 창이 없으면 새 창 생성", behavior))
            .child(setting("터미널 글꼴", "설치된 고정폭 글꼴 이름 · 한글과 이모지는 대체 글꼴 사용", self.font_input.clone()))
            .child(setting("글꼴 크기", "8–48 px", div().flex().items_center().gap_3()
                .child(icon_button("font-smaller", Icon::Minus, "글꼴 크기 줄이기", theme).size(px(32.)).on_click(cx.listener(|this, _, _, cx| { this.draft.font_size = (this.draft.font_size - 1.).max(8.); cx.notify(); })))
                .child(format!("{} px", self.draft.font_size))
                .child(icon_button("font-larger", Icon::Plus, "글꼴 크기 늘리기", theme).size(px(32.)).on_click(cx.listener(|this, _, _, cx| { this.draft.font_size = (this.draft.font_size + 1.).min(48.); cx.notify(); })))))
            .child(setting("스크롤백", "탭당 보관할 줄 수 · 최대 1,000,000줄", self.history_input.clone()))
            .child(setting("테마", "시스템을 선택하면 운영체제의 테마 설정을 따릅니다.", themes))
            .child(toggle("confirm-close", "창을 닫을 때 열린 세션 확인", self.draft.confirm_close, theme).on_click(cx.listener(|this, _, _, cx| { this.draft.confirm_close = !this.draft.confirm_close; cx.notify(); })))
            .child(toggle("restore-session", "다음 실행 시 탭과 시작 폴더 복원", self.draft.restore_session, theme).on_click(cx.listener(|this, _, _, cx| { this.draft.restore_session = !this.draft.restore_session; cx.notify(); })))
            .child(div().text_size(px(12.)).text_color(theme.muted).child("복원할 때 셸은 새로 실행됩니다. 실행 중인 명령과 출력은 복원되지 않습니다."))
            .child(div().h(px(1.)).bg(theme.border))
            .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child("사용자 지정 프로필"))
            .child(div().text_color(theme.muted).child("설정 파일의 [[profiles]]에서 실행 파일, 인자, 시작 폴더, 환경변수를 추가할 수 있습니다. 파일을 편집한 뒤 다시 불러오세요."))
            .child(div().text_size(px(12.)).text_color(theme.muted).child(self.dir.join("settings.toml").display().to_string()))
            .child(div().flex().gap_2()
                .child(button("edit-config", "설정 파일 열기", theme).px_3().py_2().on_click(cx.listener(|this, _, _, cx| {
                    if let Err(e) = platform::open_file(&this.dir.join("settings.toml")) { this.status = Some(format!("{e:#}")); cx.notify(); }
                })))
                .child(button("reload-config", "다시 불러오기", theme).px_3().py_2().on_click(cx.listener(|this, _, _, cx| this.reload_settings(cx)))));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings_scroll)
                    .child(content),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_6()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        button("reset-settings", "모두 기본값으로", theme)
                            .px_3()
                            .py_2()
                            .on_click(cx.listener(|this, _, _, cx| this.reset_general_settings(cx))),
                    )
                    .child(
                        button("save-settings", "설정 저장", theme)
                            .px_5()
                            .py_2()
                            .bg(theme.accent)
                            .text_color(theme.bg)
                            .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
                    ),
            )
            .into_any_element()
    }
}

fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: Theme) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label.clone())
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded_sm()
        .cursor_pointer()
        .bg(theme.panel)
        .text_color(theme.fg)
        .hover(move |s| s.bg(theme.hover))
        .child(label)
}

fn icon_button(
    id: impl Into<ElementId>,
    name: Icon,
    label: &'static str,
    theme: Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded_sm()
        .cursor_pointer()
        .bg(theme.panel)
        .text_color(theme.fg)
        .hover(move |s| s.bg(theme.hover))
        .child(icon(name, 16.))
}

fn toggle(id: &'static str, label: &'static str, enabled: bool, theme: Theme) -> Stateful<Div> {
    icon_button(
        id,
        if enabled {
            Icon::CheckSquare
        } else {
            Icon::Square
        },
        label,
        theme,
    )
    .justify_start()
    .gap_2()
    .px_3()
    .py_2()
    .child(label)
}
fn setting(label: &'static str, description: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().font_weight(FontWeight::MEDIUM).child(label))
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(0x929baa))
                .child(description),
        )
        .child(control)
}

fn window_controls(window: &Window, theme: Theme) -> AnyElement {
    let maximized = window.is_maximized();
    let foreground = if window.is_window_active() {
        theme.fg
    } else {
        theme.fg.opacity(0.4)
    };
    let button = |id, area, symbol, label: &'static str| {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .window_control_area(area)
            .flex()
            .items_center()
            .justify_center()
            .w(px(46.))
            .h_full()
            .flex_shrink_0()
            .font_family("Segoe MDL2 Assets")
            .text_size(px(10.))
            .line_height(px(10.))
            .text_color(foreground)
            .hover(move |s| {
                if area == WindowControlArea::Close {
                    s.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))
                } else {
                    s.bg(theme.hover).text_color(theme.fg)
                }
            })
            .child(symbol)
    };
    div()
        .flex()
        .h_full()
        .flex_shrink_0()
        .child(button(
            "minimize",
            WindowControlArea::Min,
            "\u{e921}",
            "최소화",
        ))
        .child(button(
            "maximize",
            WindowControlArea::Max,
            if maximized { "\u{e923}" } else { "\u{e922}" },
            "최대화 또는 복원",
        ))
        .child(button(
            "close",
            WindowControlArea::Close,
            "\u{e8bb}",
            "닫기",
        ))
        .into_any_element()
}

impl Render for TerminalApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let content = if let Some(t) = self.active_terminal() {
            div().flex_1().min_h_0().child(t).into_any_element()
        } else {
            self.settings_view(cx)
        };
        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .track_focus(&self.focus)
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .text_color(theme.fg)
            .bg(theme.bg)
            .capture_key_down(cx.listener(Self::key))
            .child(self.tabs_view(window, cx))
            .child(content);
        if let Some(status) = &self.status {
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .bg(theme.panel)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_size(px(12.))
                    .child(div().flex_1().child(status.clone()))
                    .when(self.config_error, |d| {
                        d.child(button("repair-config", "설정", theme).px_2().on_click(
                            cx.listener(|this, _, window, cx| this.settings_tab(window, cx)),
                        ))
                    })
                    .child(
                        icon_button("dismiss-status", Icon::Close, "알림 닫기", theme)
                            .size(px(22.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.status = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        if self.search_visible {
            let result = self
                .active_terminal()
                .map(|t| t.read(cx).search_result.clone())
                .unwrap_or_default();
            root = root.child(
                div()
                    .absolute()
                    .top(px(44.))
                    .right(px(16.))
                    .w(px(450.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .bg(theme.panel)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .child(div().flex_1().min_w_0().child(self.search.clone()))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.muted)
                            .child(result),
                    )
                    .child(
                        icon_button("find-prev", Icon::ArrowUp, "이전 검색 결과", theme)
                            .size(px(26.))
                            .on_click(cx.listener(|this, _, _, cx| this.find(true, true, cx))),
                    )
                    .child(
                        icon_button("find-next", Icon::ArrowDown, "다음 검색 결과", theme)
                            .size(px(26.))
                            .on_click(cx.listener(|this, _, _, cx| this.find(false, true, cx))),
                    )
                    .child(
                        icon_button("close-search", Icon::Close, "검색 닫기", theme)
                            .size(px(26.))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.hide_search(window, cx)),
                            ),
                    ),
            );
        }
        if self.menu {
            root = root.child(
                div()
                    .id("menu-backdrop")
                    .absolute()
                    .top(px(36.))
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.menu = false;
                            this.restore_focus(window, cx);
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
            );
            root = root.child(self.profile_menu(window, cx));
        }
        if self.confirm || self.closing {
            let closing = self.closing;
            root = root.child(div().absolute().inset_0().flex().items_center().justify_center().bg(rgba(0x00000088))
                .child(div().flex().flex_col().gap_4().w(px(420.)).p_6().rounded_lg().bg(theme.panel).border_1().border_color(theme.border).shadow_lg()
                    .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child(if closing { "터미널 종료 중…" } else { "열린 터미널을 종료할까요?" }))
                    .child(if closing { "셸과 터미널 연결을 정리하고 있습니다." } else if self.apply_update { "업데이트를 위해 모든 창의 명령을 종료합니다. 다시 시작하면 창과 탭을 복원합니다." } else { "실행 중인 명령이 함께 종료됩니다. 다음 실행 시 셸은 새로 시작됩니다." })
                    .when(!closing, |d| d.child(div().flex().justify_end().gap_2()
                        .child(button("cancel-close", "취소", theme).px_4().py_2().on_click(cx.listener(|this, _, window, cx| { this.confirm = false; this.apply_update = false; this.restore_focus(window, cx); cx.notify(); })))
                        .child(button("confirm-close", "모두 종료", theme).px_4().py_2().bg(theme.accent).text_color(theme.bg).on_click(cx.listener(|this, _, window, cx| this.close_window(window, cx))))))));
        }
        root
    }
}

pub fn run(launch_options: crate::cli::Options) {
    #[cfg(debug_assertions)]
    let smoke_mode = launch_options.smoke_mode.clone();
    #[cfg(debug_assertions)]
    let dir = smoke_mode
        .as_ref()
        .map(|mode| smoke::directory(mode))
        .unwrap_or_else(config::directory);
    #[cfg(not(debug_assertions))]
    let dir = config::directory();
    #[cfg(windows)]
    let host = {
        #[cfg(debug_assertions)]
        let enabled = smoke_mode.as_deref().is_none_or(|mode| mode == "instances");
        #[cfg(not(debug_assertions))]
        let enabled = true;
        if enabled {
            let result = crate::instance::Request::new(
                launch_options.working_directory.clone(),
                launch_options.new_window,
            )
            .and_then(|request| crate::instance::start(&dir, request));
            match result {
                Ok(crate::instance::Outcome::Forwarded) => return,
                Ok(crate::instance::Outcome::Host(host)) => Some(host),
                Err(error) => {
                    platform::show_startup_error(&format!("{error:#}"));
                    std::process::exit(2);
                }
            }
        } else {
            None
        }
    };
    let loaded = Settings::load(&dir);
    let (settings, mut error) = match loaded {
        Ok(s) => (s, None),
        Err(e) => (
            Settings::default(),
            Some(format!(
                "설정 파일을 보존하고 기본 설정으로 시작합니다. {e:#}"
            )),
        ),
    };
    #[cfg(debug_assertions)]
    let settings = if smoke_mode.is_some() {
        Settings::default()
    } else {
        settings
    };
    if error.is_none()
        && !dir.join("settings.toml").exists()
        && let Err(e) = settings.save(&dir)
    {
        error = Some(format!("{e:#}"));
    }
    let saved = match SavedSession::load(&dir) {
        Ok(s) => s,
        Err(e) => {
            error.get_or_insert_with(|| format!("이전 세션을 읽지 못했습니다: {e:#}"));
            SavedSession::default()
        }
    };
    #[cfg(debug_assertions)]
    let saved = if smoke_mode.is_some() {
        SavedSession::default()
    } else {
        saved
    };
    let detected = platform::discover_profiles();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            cx.set_global(windows::SharedSettings(settings.clone()));
            cx.set_global(windows::Windows::new(dir, detected, saved.clone()));
            #[cfg(windows)]
            {
                let requests = host.as_ref().map(|host| host.requests.clone());
                cx.global_mut::<windows::Windows>()._host = host;
                if let Some(requests) = requests {
                    cx.spawn(async move |cx| {
                        while let Ok(envelope) = requests.recv().await {
                            let result = cx.update(|cx| windows::route(&envelope.request, cx));
                            let _ = envelope.reply.send(result);
                        }
                    })
                    .detach();
                }
            }
            cx.on_window_closed(|cx, id| {
                cx.global_mut::<windows::Windows>()
                    .entries
                    .retain(|entry| entry.id != id.as_u64());
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let initial_windows = if launch_options.working_directory.is_none()
                && settings.restore_session
                && !saved.windows.is_empty()
            {
                saved.windows.iter().take(50).cloned().collect::<Vec<_>>()
            } else {
                vec![saved.clone()]
            };
            let mut first = None;
            let mut handles = Vec::new();
            for geometry in initial_windows {
                let startup = match launch_options.working_directory.clone() {
                    Some(directory) => Startup::Directory(directory),
                    None => Startup::Restore(geometry.clone()),
                };
                match windows::open(startup, geometry, error.clone(), cx) {
                    Ok(root) => {
                        if first.is_none() {
                            first = Some(root);
                        }
                        handles.push(
                            cx.global::<windows::Windows>()
                                .entries
                                .last()
                                .unwrap()
                                .handle,
                        );
                    }
                    Err(error) => {
                        eprintln!("창을 열지 못했습니다: {error:#}");
                    }
                }
            }
            if handles.is_empty() {
                cx.quit();
                return;
            }
            let active = saved.active_window.min(handles.len() - 1);
            let _ = handles[active].update(cx, |_, window, _| window.activate_window());
            #[cfg(debug_assertions)]
            if let Some(mode) = smoke_mode {
                let root = first.unwrap();
                let _ = AnyWindowHandle::from(handles[0]).update(cx, |_, window, cx| {
                    smoke::start(root, mode, launch_options.working_directory, window, cx);
                });
            }
        });
}

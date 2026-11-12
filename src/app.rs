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

mod preferences;
mod scrollbar;
mod shortcuts;
#[cfg(debug_assertions)]
mod smoke;
mod tab_menu;
mod tabs;
mod taskbar;
mod windows;
use config::shortcuts::{self as keybindings, Bindings};
use shortcuts::SettingsPage;
use tab_menu::TabMenu;
use tabs::{TabDrag, TabMotion, TabRename};

struct Tab {
    id: u64,
    custom_title: Option<String>,
    color: Option<u32>,
    terminal: Option<Entity<TerminalView>>,
    _subscription: Option<Subscription>,
}

enum Startup {
    #[cfg(windows)]
    Handoff(crate::handoff::Incoming),
    Restore(SavedSession),
    Launch {
        profile: Option<Profile>,
        directory: Option<PathBuf>,
    },
    Default,
}

struct Tip(String);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Theme::current(cx).tooltip;
        div()
            .px_2()
            .py_1()
            .rounded_xs()
            .shadow_md()
            .bg(colors.bg)
            .text_color(colors.fg)
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
    profiles: Vec<Profile>,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    focus: FocusHandle,
    tab_scroll: ScrollHandle,
    tab_scroll_drag: bool,
    tab_drag: Option<TabDrag>,
    tab_motions: Vec<TabMotion>,
    tab_rename: Option<TabRename>,
    tab_menu: Option<TabMenu>,
    menu: bool,
    menu_index: usize,
    confirm: bool,
    closing: bool,
    status: Option<String>,
    config_error: bool,
    search_visible: bool,
    search: Entity<TextField>,
    preferences: Option<preferences::Preferences>,
    _subscriptions: Vec<Subscription>,
    settings_scroll: ScrollHandle,
    shortcuts_scroll: ScrollHandle,
    settings_scroll_drag: Option<Pixels>,
    settings_page: SettingsPage,
    shortcut_capture: Option<(&'static str, Option<usize>)>,
    shortcut_bindings: Bindings,
    update_state: crate::update::State,
    #[cfg(windows)]
    update_handoff: Option<crate::update::Handoff>,
    apply_update: bool,
    last_bounds: WindowBounds,
    retiring_sessions: Vec<async_channel::Receiver<()>>,
}

impl TerminalApp {
    fn new(
        dir: PathBuf,
        settings: Settings,
        startup: Startup,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| TextField::new("", "터미널에서 찾기", cx));
        let subscriptions = vec![
            cx.observe_global::<preferences::ProfileSearch>(|_, cx| cx.notify()),
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
                    if this.tab_menu.take().is_some() {
                        this.restore_focus(window, cx);
                    }
                }
                cx.notify();
            }),
            cx.observe_window_bounds(window, |this, window, _| {
                this.last_bounds = window.window_bounds()
            }),
            cx.observe_window_appearance(window, |this, window, _| {
                if this.settings.theme == "system" {
                    window.refresh();
                }
            }),
            cx.observe_global::<windows::SharedSettings>(|this, cx| {
                let settings = cx.global::<windows::SharedSettings>().0.clone();
                if this.settings != settings {
                    let mut previous = this.settings.clone();
                    previous.discovered_profiles = settings.discovered_profiles.clone();
                    if previous == settings {
                        // Preserve unsaved text/number edits when a profile search completes.
                        this.settings.discovered_profiles = settings.discovered_profiles.clone();
                        this.draft.discovered_profiles = settings.discovered_profiles.clone();
                        this.profiles = platform::configured_profiles(&settings);
                        if let Some(prefs) = &this.preferences {
                            let choices = this
                                .profiles
                                .iter()
                                .map(|p| {
                                    crate::select::Choice::new(p.id.clone(), p.name.clone())
                                        .with_profile_icon(p.program.clone())
                                })
                                .collect();
                            prefs
                                .profile
                                .update(cx, |select, cx| select.replace_choices(choices, cx));
                        }
                        cx.notify();
                        return;
                    }
                    this.settings = settings.clone();
                    this.draft = settings;
                    this.shortcut_bindings =
                        Bindings::new(&this.settings.keybindings).expect("validated settings");
                    this.shortcut_capture = None;
                    this.preferences = None;
                    this.profiles = platform::configured_profiles(&this.settings);
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
        let profiles = platform::configured_profiles(&settings);
        let shortcut_bindings = Bindings::new(&settings.keybindings).expect("validated settings");
        let mut this = Self {
            dir: dir.clone(),
            draft: settings.clone(),
            settings,
            profiles,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            focus: cx.focus_handle(),
            tab_scroll: ScrollHandle::new(),
            tab_scroll_drag: false,
            tab_drag: None,
            tab_motions: Vec::new(),
            tab_rename: None,
            tab_menu: None,
            menu: false,
            menu_index: 0,
            confirm: false,
            closing: false,
            status: error.clone(),
            config_error: error.is_some(),
            search_visible: false,
            search,
            preferences: None,
            _subscriptions: subscriptions,
            settings_scroll: ScrollHandle::new(),
            shortcuts_scroll: ScrollHandle::new(),
            settings_scroll_drag: None,
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
            retiring_sessions: Vec::new(),
        };
        #[cfg(windows)]
        let startup = match startup {
            Startup::Handoff(incoming) => {
                this.add_handoff(incoming, window, cx);
                Startup::Default
            }
            startup => startup,
        };
        if let Startup::Launch { profile, directory } = startup {
            this.add_terminal(
                profile.unwrap_or_else(|| this.default_profile()),
                directory,
                window,
                cx,
            );
        } else if let Startup::Restore(saved) = startup
            && this.settings.restore_session
        {
            let active = saved.active;
            let mut restored_active = 0;
            for (index, tab) in saved.tabs.into_iter().take(50).enumerate() {
                if let Some(profile) = this
                    .profiles
                    .iter()
                    .find(|p| p.id == tab.profile_id)
                    .cloned()
                {
                    if index <= active {
                        restored_active = this.tabs.len();
                    }
                    this.create_terminal(profile, Some(tab.cwd), window, cx);
                    this.tabs.last_mut().unwrap().custom_title = tab.custom_title;
                    this.tabs.last_mut().unwrap().color =
                        tab.color.filter(|color| *color <= 0xffffff);
                }
            }
            if !this.tabs.is_empty() {
                // Keep the initial PowerShell's runtime initialization free of
                // competing background shells. Selecting any tab still starts it now.
                this.activate(restored_active, window, cx);
                this.start_restored_backgrounds(cx);
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

    fn start_restored_backgrounds(&self, cx: &mut Context<Self>) {
        let ids: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.active)
            .map(|(_, tab)| tab.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let began = Instant::now();
            loop {
                match this.update(cx, |app, cx| {
                    if app.closing {
                        return None;
                    }
                    Some(
                        began.elapsed() >= Duration::from_secs(3)
                            || app.active_terminal().is_none_or(|terminal| {
                                let terminal = terminal.read(cx);
                                terminal.error.is_some()
                                    || terminal.exited
                                    || terminal
                                        .session
                                        .as_ref()
                                        .is_some_and(|session| session.prompt_reported())
                            }),
                    )
                }) {
                    Ok(Some(true)) => break,
                    Ok(Some(false)) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(25))
                            .await
                    }
                    _ => return,
                }
            }
            let _ = this.update(cx, |app, cx| {
                if app.closing {
                    return;
                }
                platform::trace_startup("background_restore_start");
                for tab in &app.tabs {
                    if ids.contains(&tab.id)
                        && let Some(terminal) = &tab.terminal
                    {
                        terminal.update(cx, |terminal, cx| terminal.start(false, cx));
                    }
                }
            });
        })
        .detach();
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
        self.create_terminal(profile, cwd, window, cx);
        self.activate(self.tabs.len() - 1, window, cx);
    }
    fn create_terminal(
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
            move |this, _, e: &TerminalEvent, window, cx| {
                if matches!(e, TerminalEvent::Exited(true)) {
                    this.close_tab(id, window, cx);
                } else {
                    if this.tabs.get(this.active).is_some_and(|t| t.id == id) {
                        this.update_window_title(window, cx);
                    }
                    cx.notify();
                }
            },
        );
        self.tabs.push(Tab {
            id,
            custom_title: None,
            color: None,
            terminal: Some(terminal),
            _subscription: Some(subscription),
        });
        self.menu = false;
    }

    #[cfg(windows)]
    fn add_handoff(
        &mut self,
        incoming: crate::handoff::Incoming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile = incoming
            .program()
            .map(|program| {
                let name = PathBuf::from(&program)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let mut profile = self
                    .profiles
                    .iter()
                    .find(|profile| {
                        PathBuf::from(profile.program.trim_matches('"'))
                            .file_stem()
                            .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(&name))
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        Profile::new(&format!("external:{name}"), &name, &program, &[])
                    });
                profile.program = program;
                profile
            })
            .unwrap_or_else(|| self.default_profile());
        self.create_terminal(profile, None, window, cx);
        let terminal = self.tabs.last().unwrap().terminal.as_ref().unwrap().clone();
        terminal.update(cx, |view, _| view.receive_handoff(incoming));
        self.activate(self.tabs.len() - 1, window, cx);
    }
    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        self.tab_rename = None;
        self.tab_menu = None;
        self.shortcut_capture = None;
        self.search_visible = false;
        self.menu = false;
        self.tab_scroll.scroll_to_item(index);
        if let Some(terminal) = self.active_terminal() {
            terminal.update(cx, |terminal, cx| terminal.start(true, cx));
            terminal.read(cx).focus.clone().focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
        self.update_window_title(window, cx);
        cx.notify();
    }
    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_tab_drag(true, window, cx);
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let removed = self.tabs.remove(index);
        self.retiring_sessions
            .retain(|r| r.is_empty() && !r.is_closed());
        if let Some(terminal) = removed.terminal {
            self.retiring_sessions
                .extend(terminal.update(cx, |terminal, _| terminal.shutdown()));
        }
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
                custom_title: None,
                color: None,
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
            .any(|t| t.read(cx).is_running());
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
                                .any(|terminal| terminal.read(cx).is_running())
                        })
                    })
            });
        if self.settings.confirm_close && (active || other_active) {
            self.confirm = true;
            self.tab_rename = None;
            self.tab_menu = None;
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
                Some(SavedTab {
                    profile_id: t.profile.id.clone(),
                    cwd: t.working_directory(),
                    custom_title: tab.custom_title.clone(),
                    color: tab.color,
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
        let mut receivers: Vec<_> = self
            .tabs
            .iter()
            .filter_map(|t| t.terminal.as_ref())
            .flat_map(|terminal| terminal.update(cx, |terminal, _| terminal.shutdown()))
            .collect();
        receivers.append(&mut self.retiring_sessions);
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
            discovered_profiles: self.draft.discovered_profiles.clone(),
            keybindings: self.draft.keybindings.clone(),
            allow_osc52: self.draft.allow_osc52,
            ..Settings::default()
        };
        self.preferences = None;
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
        if let Err(error) = self.read_preferences(cx) {
            self.status = Some(error.into());
            cx.notify();
            return;
        }
        match self.draft.save(&self.dir) {
            Ok(()) => {
                self.settings = self.draft.clone();
                self.shortcut_bindings =
                    Bindings::new(&self.settings.keybindings).expect("validated settings");
                self.shortcut_capture = None;
                cx.set_global(windows::SharedSettings(self.settings.clone()));
                self.config_error = false;
                self.profiles = platform::configured_profiles(&self.settings);
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
                self.shortcut_bindings =
                    Bindings::new(&self.settings.keybindings).expect("validated settings");
                self.shortcut_capture = None;
                self.preferences = None;
                self.profiles = platform::configured_profiles(&self.settings);
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
        // The rename dialog owns keyboard input, including text editing shortcuts.
        if self.tab_rename_key(e, window, cx) {
            return;
        }
        if self.tab_menu_key(e, window, cx) {
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
        if self.search_visible
            && self.search.read(cx).focus.is_focused(window)
            && k.key == "enter"
            && m.shift
        {
            self.find(true, true, cx);
        } else if let Some(action) = self
            .shortcut_bindings
            .action(k, cx.keyboard_mapper().as_ref())
        {
            if !self.run_shortcut(action, window, cx) {
                return;
            }
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
        // GPUI snaps each explicit width separately. Round down before summing so
        // fitted tabs cannot grow past their container by a few device pixels.
        let scale = window.scale_factor();
        let tab_width = (tab_width * scale).floor() / scale;
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
                    .tooltip(tip(format!(
                        "새 탭 · {}",
                        self.shortcut_label(keybindings::Action::NewTab)
                    )))
                    .on_click(cx.listener(|this, _, window, cx| this.new_default(window, cx))),
            )
            .child(
                icon_button("profiles", Icon::ChevronDown, "프로필 및 설정", theme)
                    .cursor_default()
                    .w(px(36.))
                    .h_full()
                    .bg(theme.bar)
                    .tooltip(tip(format!(
                        "프로필 및 설정 · {}",
                        self.shortcut_label(keybindings::Action::ProfileMenu)
                    )))
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
            // Leave the whole scrollbar clear, or just the active tab when it is absent.
            // Read the scroll offset in prepaint so tab reveal/scroll stays aligned.
            .child(
                canvas(
                    move |_, _, _| {
                        if overflow {
                            return (left, left + tabs_extent);
                        }
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
                    .child(
                        div()
                            .text_color(theme.accent)
                            .child(icons::profiles::element(&p.program, 16., cx)),
                    )
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
                settings_button("check-update", "업데이트 확인", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.check_update(cx))),
            );
        }
        #[cfg(windows)]
        if matches!(self.update_state, State::Available(_)) {
            row = row.child(
                settings_button("download-update", "업데이트 다운로드", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.download_update(cx))),
            );
        } else if matches!(self.update_state, State::Ready) {
            row = row.child(
                settings_button("apply-update", "다시 시작하여 업데이트", theme).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.apply_update = true;
                        this.should_close(window, cx);
                    }),
                ),
            );
        }
        row.into_any_element()
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

fn settings_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
) -> Stateful<Div> {
    settings_action(id, label, theme, false)
}

fn primary_settings_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
) -> Stateful<Div> {
    settings_action(id, label, theme, true)
}

fn settings_action(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
    primary: bool,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label.clone())
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .h(px(34.))
        .px_3()
        .rounded_md()
        .border_1()
        .border_color(if primary { theme.accent } else { theme.border })
        .bg(if primary { theme.accent } else { theme.hover })
        .text_color(if primary { theme.bg } else { theme.fg })
        .cursor_pointer()
        .tab_index(0)
        .tab_stop(true)
        .focus_visible(move |s| s.border_color(theme.accent))
        .hover(move |s| {
            s.bg(if primary {
                theme.accent.opacity(0.9)
            } else {
                theme.border
            })
            .border_color(if primary { theme.accent } else { theme.muted })
        })
        .active(move |s| {
            s.bg(if primary {
                theme.accent.opacity(0.8)
            } else {
                theme.panel
            })
        })
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

fn window_controls(window: &Window, theme: Theme) -> AnyElement {
    let maximized = window.is_maximized();
    let foreground = if window.is_window_active() {
        theme.fg
    } else {
        theme.window.inactive_fg
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
                    s.bg(theme.window.close_hover.bg)
                        .text_color(theme.window.close_hover.fg)
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
            self.ensure_preferences(window, cx);
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
        if self.tab_rename.is_some() {
            root = root.child(self.tab_rename_dialog(cx));
        }
        if self.tab_menu.is_some() {
            root = root.child(self.tab_context_menu(window, cx));
        }
        if self.confirm || self.closing {
            let closing = self.closing;
            root = root.child(div().absolute().inset_0().flex().items_center().justify_center().bg(theme.overlay)
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

/// Match the first selected tab without creating any UI state. Keep the same
/// filtering and active-index rules as TerminalApp::new's restoration path.
#[cfg(windows)]
fn initial_session(
    settings: &Settings,
    profiles: &[Profile],
    saved: &SavedSession,
    options: &crate::cli::Options,
) -> Option<(Profile, Option<PathBuf>)> {
    // Handoffs already have a child. Multiple windows retain the existing
    // per-window launcher ordering rather than speculating about a target.
    if options.handoff.is_some() || saved.windows.len() > 1 {
        return None;
    }
    let default = || {
        profiles
            .iter()
            .find(|p| p.id == settings.default_profile)
            .or_else(|| profiles.first())
            .cloned()
    };
    if options.profile.is_some() || options.working_directory.is_some() {
        let profile = if let Some(id) = &options.profile {
            profiles.iter().find(|p| &p.id == id).cloned()
        } else {
            default()
        }?;
        return Some((profile, options.working_directory.clone()));
    }
    if settings.restore_session {
        let saved = saved.windows.first().unwrap_or(saved);
        let mut first = None;
        let mut selected = None;
        for (index, tab) in saved.tabs.iter().take(50).enumerate() {
            if let Some(profile) = profiles.iter().find(|p| p.id == tab.profile_id) {
                let candidate = (profile.clone(), Some(tab.cwd.clone()));
                first.get_or_insert_with(|| candidate.clone());
                if index <= saved.active {
                    selected = Some(candidate);
                }
            }
        }
        if let Some(selected) = selected.or(first) {
            return Some(selected);
        }
    }
    default().map(|profile| (profile, None))
}

#[cfg(all(test, windows))]
mod startup_tests {
    use super::{PathBuf, Profile, SavedSession, SavedTab, Settings, initial_session};

    #[test]
    fn prestart_matches_default_explicit_and_filtered_restore_selection() {
        let profiles = vec![
            Profile::new("pwsh", "PowerShell", "pwsh.exe", &[]),
            Profile::new("cmd", "CMD", "cmd.exe", &[]),
        ];
        let mut settings = Settings {
            default_profile: "cmd".into(),
            ..Settings::default()
        };
        let mut saved = SavedSession::default();
        let mut options = crate::cli::Options::default();
        let selected =
            |settings: &Settings, saved: &SavedSession, options: &crate::cli::Options| {
                initial_session(settings, &profiles, saved, options).map(|(p, cwd)| (p.id, cwd))
            };
        assert_eq!(
            selected(&settings, &saved, &options),
            Some(("cmd".into(), None))
        );
        saved.tabs = ["missing", "pwsh", "missing", "cmd"]
            .into_iter()
            .enumerate()
            .map(|(i, id)| SavedTab {
                profile_id: id.into(),
                cwd: PathBuf::from(format!("C:/tab{i}")),
                custom_title: None,
                color: None,
            })
            .collect();
        for (active, id, cwd) in [
            (0, "pwsh", "C:/tab1"),
            (1, "pwsh", "C:/tab1"),
            (2, "pwsh", "C:/tab1"),
            (3, "cmd", "C:/tab3"),
            (99, "cmd", "C:/tab3"),
        ] {
            saved.active = active;
            assert_eq!(
                selected(&settings, &saved, &options),
                Some((id.into(), Some(cwd.into())))
            );
        }
        options.working_directory = Some("C:/explicit".into());
        assert_eq!(
            selected(&settings, &saved, &options),
            Some(("cmd".into(), options.working_directory.clone()))
        );
        options.profile = Some("pwsh".into());
        assert_eq!(
            selected(&settings, &saved, &options),
            Some(("pwsh".into(), options.working_directory.clone()))
        );
        options.profile = Some("missing".into());
        assert!(selected(&settings, &saved, &options).is_none());
        options = crate::cli::Options::default();
        settings.restore_session = false;
        assert_eq!(
            selected(&settings, &saved, &options),
            Some(("cmd".into(), None))
        );
        settings.restore_session = true;
        let single_window = SavedSession {
            windows: vec![saved.clone()],
            ..SavedSession::default()
        };
        assert_eq!(
            selected(&settings, &single_window, &options),
            selected(&settings, &saved, &options)
        );
        let multiple = SavedSession {
            windows: vec![saved.clone(), saved],
            ..SavedSession::default()
        };
        assert!(selected(&settings, &multiple, &options).is_none());
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
            .and_then(|mut request| {
                request.profile = launch_options.profile.clone();
                request.handoff = launch_options
                    .handoff
                    .as_ref()
                    .map(crate::handoff::Incoming::descriptor);
                if request.handoff.is_some() {
                    request.version = 2;
                }
                crate::instance::start(&dir, request)
            });
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
    platform::trace_startup("instance_ready");
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
    let initialize_profiles = error.is_none();
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
    platform::trace_startup("config_ready");
    let mut startup_settings = settings;
    let startup_directory = dir.clone();
    #[cfg(windows)]
    let startup_saved = saved.clone();
    // Incoming contains owned OS handles and stays on the main thread.
    let startup_profile = launch_options.profile.clone();
    #[cfg(windows)]
    let startup_options = crate::cli::Options {
        profile: startup_profile.clone(),
        working_directory: launch_options.working_directory.clone(),
        ..Default::default()
    };
    #[cfg(windows)]
    let prestart_enabled = launch_options.handoff.is_none();
    #[cfg(all(windows, debug_assertions))]
    let prestart_enabled = prestart_enabled && smoke_mode.is_none();
    let bootstrap = std::thread::Builder::new()
        .name("edolterm-bootstrap".into())
        .spawn(move || -> anyhow::Result<_> {
            let profile_error = if initialize_profiles {
                platform::initialize_profiles(&mut startup_settings, &startup_directory)
                    .err()
                    .map(|error| format!("프로필 검색 결과를 저장하지 못했습니다: {error:#}"))
            } else {
                None
            };
            platform::trace_startup("profiles_ready");
            let profiles = platform::configured_profiles(&startup_settings);
            let launch_profile = startup_profile
                .as_ref()
                .map(|id| {
                    profiles
                        .iter()
                        .find(|profile| &profile.id == id)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("프로필을 찾을 수 없습니다: {id}"))
                })
                .transpose()?;
            #[cfg(windows)]
            let starts = if prestart_enabled {
                initial_session(
                    &startup_settings,
                    &profiles,
                    &startup_saved,
                    &startup_options,
                )
                .map(|(profile, cwd)| {
                    crate::terminal_view::SessionStarts::prestart(
                        profile,
                        cwd,
                        startup_settings.clone(),
                    )
                })
                .transpose()?
            } else {
                None
            };
            #[cfg(not(windows))]
            let starts: Option<crate::terminal_view::SessionStarts> = None;
            Ok((startup_settings, launch_profile, starts, profile_error))
        });
    let bootstrap = match bootstrap {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            platform::show_startup_error(&format!("시작 작업을 만들지 못했습니다: {error}"));
            std::process::exit(2);
        }
    };
    platform::trace_startup("gpui_start");
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            platform::trace_startup("gpui_ready");
            let (settings, launch_profile, starts, profile_error) = match bootstrap.join() {
                Ok(Ok(prepared)) => prepared,
                result => {
                    let error = match result {
                        Ok(Err(error)) => format!("{error:#}"),
                        Err(_) => "시작 작업이 종료되었습니다.".into(),
                        _ => unreachable!(),
                    };
                    platform::show_startup_error(&error);
                    std::process::exit(2);
                }
            };
            let error = error.or(profile_error);
            if let Some(starts) = starts {
                cx.set_global(starts);
            }
            cx.set_app_identity(crate::taskbar::APP_ID, "Edolterm");
            cx.set_global(preferences::ProfileSearch::default());
            cx.set_global(windows::SharedSettings(settings.clone()));
            cx.set_global(windows::Windows::new(dir, saved.clone()));
            #[cfg(debug_assertions)]
            let update_taskbar = smoke_mode.is_none();
            #[cfg(not(debug_assertions))]
            let update_taskbar = true;
            #[cfg(windows)]
            {
                let requests = host.as_ref().map(|host| host.requests.clone());
                cx.global_mut::<windows::Windows>()._host = host;
                if let Some(requests) = requests {
                    cx.spawn(async move |cx| {
                        while let Ok(mut envelope) = requests.recv().await {
                            let result = cx.update(|cx| {
                                windows::route(&envelope.request, envelope.handoff.take(), cx)
                            });
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
            #[cfg(windows)]
            let mut incoming = launch_options.handoff;
            #[cfg(windows)]
            let external = incoming.is_some();
            #[cfg(not(windows))]
            let external = false;
            let initial_windows = if !external
                && launch_options.working_directory.is_none()
                && launch_profile.is_none()
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
                let startup =
                    if launch_profile.is_some() || launch_options.working_directory.is_some() {
                        Startup::Launch {
                            profile: launch_profile.clone(),
                            directory: launch_options.working_directory.clone(),
                        }
                    } else {
                        Startup::Restore(geometry.clone())
                    };
                #[cfg(windows)]
                let startup = incoming.take().map(Startup::Handoff).unwrap_or(startup);
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
            if update_taskbar {
                let root = cx.global::<windows::Windows>().entries[active].root.clone();
                let _ = handles[active].update(cx, |_, window, _| taskbar::start(root, window));
            }
            #[cfg(debug_assertions)]
            if let Some(mode) = smoke_mode {
                let root = first.unwrap();
                let _ = AnyWindowHandle::from(handles[0]).update(cx, |_, window, cx| {
                    smoke::start(root, mode, launch_options.working_directory, window, cx);
                });
            }
        });
}

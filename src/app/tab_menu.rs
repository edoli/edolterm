use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TabAction {
    Color,
    Rename,
    Duplicate,
    Export,
    Find,
    CloseOthers,
    CloseRight,
    Close,
}

impl TabAction {
    pub(super) const ALL: [Self; 8] = [
        Self::Color,
        Self::Rename,
        Self::Duplicate,
        Self::Export,
        Self::Find,
        Self::CloseOthers,
        Self::CloseRight,
        Self::Close,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Color => "탭 색 변경",
            Self::Rename => "탭 이름 바꾸기",
            Self::Duplicate => "탭 복제",
            Self::Export => "텍스트 내보내기",
            Self::Find => "찾기",
            Self::CloseOthers => "다른 탭 닫기",
            Self::CloseRight => "오른쪽 탭들 닫기",
            Self::Close => "탭 닫기",
        }
    }

    fn icon(self) -> Option<Icon> {
        match self {
            Self::Color => Some(Icon::Palette),
            Self::Rename => Some(Icon::Rename),
            Self::Duplicate => Some(Icon::Copy),
            Self::Export => Some(Icon::Export),
            Self::Find => Some(Icon::Search),
            Self::CloseOthers | Self::CloseRight => None,
            Self::Close => Some(Icon::Close),
        }
    }

    fn shortcut(self) -> Option<keybindings::Action> {
        match self {
            Self::Duplicate => Some(keybindings::Action::DuplicateTab),
            Self::Find => Some(keybindings::Action::Find),
            Self::Close => Some(keybindings::Action::CloseTab),
            _ => None,
        }
    }
}

const COLORS: [(&str, u32); 16] = [
    ("빨강", 0xe74856),
    ("주황", 0xf28c28),
    ("노랑", 0xe5c54b),
    ("초록", 0x16a365),
    ("청록", 0x21b6a8),
    ("하늘", 0x3a96dd),
    ("파랑", 0x3b78ff),
    ("남색", 0x6963d9),
    ("보라", 0x9b59b6),
    ("자홍", 0xd457a6),
    ("분홍", 0xef9aab),
    ("갈색", 0xa77853),
    ("회색", 0x888888),
    ("은색", 0xbbbbbb),
    ("흰색", 0xf2f2f2),
    ("검정", 0x303030),
];

pub(super) struct TabMenu {
    pub(super) id: u64,
    pub(super) position: Point<Pixels>,
    pub(super) index: usize,
    pub(super) colors: bool,
}

impl TerminalApp {
    pub(super) fn show_tab_menu(
        &mut self,
        id: u64,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_tab_drag(true, window, cx);
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.activate(index, window, cx);
        self.tab_menu = Some(TabMenu {
            id,
            position,
            index: 0,
            colors: false,
        });
        let first = TabAction::ALL
            .iter()
            .position(|action| self.tab_action_enabled(id, *action, cx))
            .unwrap_or(7);
        self.tab_menu.as_mut().unwrap().index = first;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn dismiss_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tab_menu = None;
        self.restore_focus(window, cx);
        cx.notify();
    }

    pub(super) fn tab_action_enabled(&self, id: u64, action: TabAction, cx: &App) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        let tab = &self.tabs[index];
        match action {
            TabAction::Close => true,
            TabAction::CloseOthers => self.tabs.len() > 1,
            TabAction::CloseRight => index + 1 < self.tabs.len(),
            TabAction::Export => tab
                .terminal
                .as_ref()
                .is_some_and(|terminal| terminal.read(cx).session.is_some()),
            _ => tab.terminal.is_some(),
        }
    }

    pub(super) fn duplicate_tab(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return false;
        };
        let Some(terminal) = &tab.terminal else {
            return false;
        };
        let view = terminal.read(cx);
        let (profile, cwd, title, color) = (
            view.profile.clone(),
            view.working_directory(),
            tab.custom_title.clone(),
            tab.color,
        );
        self.add_terminal(profile, Some(cwd), window, cx);
        let tab = self.tabs.last_mut().unwrap();
        tab.custom_title = title;
        tab.color = color;
        self.update_window_title(window, cx);
        self.persist(window, cx);
        true
    }

    pub(super) fn run_tab_action(
        &mut self,
        action: TabAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = &self.tab_menu else {
            return;
        };
        let id = menu.id;
        if !self.tab_action_enabled(id, action, cx) {
            return;
        }
        if action == TabAction::Color {
            let color = self
                .tabs
                .iter()
                .find(|tab| tab.id == id)
                .and_then(|tab| tab.color);
            let index = COLORS
                .iter()
                .position(|(_, value)| Some(*value) == color)
                .map_or(0, |i| i + 1);
            let menu = self.tab_menu.as_mut().unwrap();
            menu.colors = true;
            menu.index = index;
            cx.notify();
            return;
        }
        self.dismiss_tab_menu(window, cx);
        match action {
            TabAction::Rename => self.begin_tab_rename(id, window, cx),
            TabAction::Duplicate => {
                self.duplicate_tab(id, window, cx);
            }
            TabAction::Export => self.export_tab(id, cx),
            TabAction::Find => self.show_search(window, cx),
            TabAction::Close => self.close_tab(id, window, cx),
            TabAction::CloseOthers | TabAction::CloseRight => {
                let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
                    return;
                };
                let ids: Vec<_> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(i, tab)| {
                        if action == TabAction::CloseOthers {
                            tab.id != id
                        } else {
                            *i > index
                        }
                    })
                    .map(|(_, tab)| tab.id)
                    .collect();
                for id in ids.into_iter().rev() {
                    self.close_tab(id, window, cx);
                }
                self.persist(window, cx);
            }
            TabAction::Color => unreachable!(),
        }
    }

    pub(super) fn set_tab_color(
        &mut self,
        color: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = &self.tab_menu else {
            return;
        };
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == menu.id) {
            tab.color = color;
        }
        self.dismiss_tab_menu(window, cx);
        self.persist(window, cx);
    }

    fn export_tab(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(terminal) = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.terminal.as_ref())
        else {
            return;
        };
        let terminal = terminal.read(cx);
        let Some(session) = &terminal.session else {
            return;
        };
        let text = session.plain_text();
        let directory = terminal.working_directory();
        self.export_text(text, directory, cx);
    }

    fn export_text(&mut self, text: String, directory: PathBuf, cx: &mut Context<Self>) {
        let dialog = cx.prompt_for_new_path(&directory, Some("edolterm.txt"));
        cx.spawn(async move |this, cx| {
            let result: anyhow::Result<Option<PathBuf>> = async {
                let Some(path) = dialog.await?? else {
                    return Ok(None);
                };
                cx.background_spawn(async move {
                    config::atomic_write(&path, text.as_bytes())?;
                    anyhow::Ok(Some(path))
                })
                .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(Some(path)) => Some(format!("텍스트를 저장했습니다: {}", path.display())),
                    Ok(None) => return,
                    Err(error) => Some(format!("텍스트 내보내기 실패: {error:#}")),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn tab_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = &self.tab_menu else {
            return false;
        };
        let (id, index, colors) = (menu.id, menu.index, menu.colors);
        let count = if colors {
            COLORS.len() + 1
        } else {
            TabAction::ALL.len()
        };
        match event.keystroke.key.as_str() {
            "escape" | "tab" => self.dismiss_tab_menu(window, cx),
            "enter" | "space" => {
                if colors {
                    self.set_tab_color(index.checked_sub(1).map(|i| COLORS[i].1), window, cx);
                } else {
                    self.run_tab_action(TabAction::ALL[index], window, cx);
                }
            }
            key @ ("up" | "down" | "left" | "right" | "home" | "end") => {
                let forward = matches!(key, "down" | "right" | "home");
                let mut next = match key {
                    "home" => 0,
                    "end" => count - 1,
                    _ => (index + if forward { 1 } else { count - 1 }) % count,
                };
                for _ in 0..count {
                    if colors || self.tab_action_enabled(id, TabAction::ALL[next], cx) {
                        break;
                    }
                    next = (next + if forward { 1 } else { count - 1 }) % count;
                }
                self.tab_menu.as_mut().unwrap().index = next;
                cx.notify();
            }
            _ => {}
        }
        window.prevent_default();
        cx.stop_propagation();
        true
    }

    pub(super) fn tab_context_menu(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let menu = self.tab_menu.as_ref().unwrap();
        let width = 280_f32.min((f32::from(window.viewport_size().width) - 16.).max(1.));
        let height = if menu.colors { 246. } else { 282. };
        let left = f32::from(menu.position.x)
            .min(f32::from(window.viewport_size().width) - width - 8.)
            .max(8.);
        let top = f32::from(menu.position.y)
            .min(f32::from(window.viewport_size().height) - height - 8.)
            .max(8.);
        let mut panel = div()
            .id("tab-context-menu")
            .role(Role::Menu)
            .occlude()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(width))
            .max_h((window.viewport_size().height - px(16.)).max(px(1.)))
            .overflow_y_scroll()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.panel)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());
        if menu.colors {
            panel = panel.child(div().px_3().py_2().child("탭 색 변경"));
            panel = panel.child(
                div().px_3().pb_2().child(
                    settings_button("reset-tab-color", "기본 색으로 복원", theme)
                        .w_full()
                        .on_mouse_move(cx.listener(|this, _, _, cx| {
                            if let Some(menu) = &mut this.tab_menu
                                && menu.index != 0
                            {
                                menu.index = 0;
                                cx.notify();
                            }
                        }))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.set_tab_color(None, window, cx)),
                        ),
                ),
            );
            let current = self
                .tabs
                .iter()
                .find(|tab| tab.id == menu.id)
                .and_then(|tab| tab.color);
            for (row_index, colors) in COLORS.chunks(4).enumerate() {
                let mut row = div().flex().gap_2().px_3().py_1();
                for (column, &(name, color)) in colors.iter().enumerate() {
                    let index = row_index * 4 + column + 1;
                    row = row.child(
                        div()
                            .id(("tab-color", index))
                            .role(Role::Button)
                            .aria_label(name)
                            .aria_selected(current == Some(color))
                            .flex_1()
                            .h(px(28.))
                            .rounded_sm()
                            .bg(rgb(color))
                            .border_2()
                            .border_color(if menu.index == index || current == Some(color) {
                                theme.fg
                            } else {
                                theme.border
                            })
                            .cursor_pointer()
                            .tooltip(tip(name.into()))
                            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                                if let Some(menu) = &mut this.tab_menu
                                    && menu.index != index
                                {
                                    menu.index = index;
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_tab_color(Some(color), window, cx)
                            })),
                    );
                }
                panel = panel.child(row);
            }
        } else {
            for (index, action) in TabAction::ALL.into_iter().enumerate() {
                let enabled = self.tab_action_enabled(menu.id, action, cx);
                let shortcut = action
                    .shortcut()
                    .map(|action| self.shortcut_label(action))
                    .unwrap_or_default();
                panel = panel.child(
                    div()
                        .id(("tab-menu-action", index))
                        .role(Role::MenuItem)
                        .aria_label(action.label())
                        .flex()
                        .items_center()
                        .gap_3()
                        .h(px(34.))
                        .px_3()
                        .rounded_sm()
                        .cursor_default()
                        .text_color(if enabled {
                            theme.fg
                        } else {
                            theme.muted.opacity(0.5)
                        })
                        .when(enabled && menu.index == index, |row| row.bg(theme.hover))
                        .child(
                            div()
                                .size(px(16.))
                                .flex_shrink_0()
                                .children(action.icon().map(|name| icon(name, 16.))),
                        )
                        .child(div().flex_1().child(action.label()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme.muted)
                                .child(shortcut),
                        )
                        .when(enabled, |row| {
                            row.on_mouse_move(cx.listener(move |this, _, _, cx| {
                                if let Some(menu) = &mut this.tab_menu
                                    && menu.index != index
                                {
                                    menu.index = index;
                                    cx.notify();
                                }
                            }))
                            .on_click(cx.listener(
                                move |this, _, window, cx| this.run_tab_action(action, window, cx),
                            ))
                        }),
                );
            }
        }
        div()
            .id("tab-menu-backdrop")
            .occlude()
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.dismiss_tab_menu(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    this.dismiss_tab_menu(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, _, window, cx| {
                    this.dismiss_tab_menu(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(panel)
            .into_any_element()
    }
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::{Profile, SavedSession, Settings, Startup, TabAction, TerminalApp};
    use gpui::{TestAppContext, point, px};

    #[gpui::test]
    fn context_actions_preserve_target_and_export_handles_save_cancel_and_error(
        cx: &mut TestAppContext,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "edolterm-tab-menu-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let profile = Profile::new("test", "Test", "__edolterm_missing_test_shell__", &[]);
        let window = cx.add_window(|window, cx| {
            TerminalApp::new(
                dir.clone(),
                Settings {
                    profiles: vec![profile.clone()],
                    ..Settings::default()
                },
                Startup::Default,
                None,
                window,
                cx,
            )
        });
        window
            .update(cx, |app, window, cx| {
                app.create_terminal(profile.clone(), Some(dir.clone()), window, cx);
                app.create_terminal(profile, Some(dir.clone()), window, cx);
                let id = app.tabs[1].id;
                app.tabs[1].custom_title = Some("대상 🙂".into());
                app.show_tab_menu(id, point(px(100.), px(18.)), window, cx);
                app.run_tab_action(TabAction::Color, window, cx);
                app.set_tab_color(Some(0xe74856), window, cx);
                assert_eq!(
                    SavedSession::load(&dir).unwrap().tabs[1].color,
                    Some(0xe74856)
                );
                assert!(app.duplicate_tab(id, window, cx));
                let duplicate = app.tabs.last().unwrap();
                assert_eq!(duplicate.custom_title.as_deref(), Some("대상 🙂"));
                assert_eq!(duplicate.color, Some(0xe74856));
                assert_eq!(
                    duplicate
                        .terminal
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .working_directory(),
                    dir
                );
                app.show_tab_menu(id, point(px(100.), px(18.)), window, cx);
                app.run_tab_action(TabAction::CloseRight, window, cx);
                assert_eq!(app.tabs.len(), 2);
                assert_eq!(app.tabs[app.active].id, id);
                app.show_tab_menu(id, point(px(100.), px(18.)), window, cx);
                assert!(!app.tab_action_enabled(id, TabAction::CloseRight, cx));
                app.run_tab_action(TabAction::CloseOthers, window, cx);
                assert_eq!(app.tabs.len(), 1);
                assert_eq!(app.tabs[0].id, id);
                app.show_tab_menu(id, point(px(100.), px(18.)), window, cx);
                assert!(!app.tab_action_enabled(id, TabAction::CloseOthers, cx));
                app.set_tab_color(None, window, cx);
                assert_eq!(SavedSession::load(&dir).unwrap().tabs[0].color, None);
                app.settings_tab(window, cx);
                app.show_tab_menu(0, point(px(100.), px(18.)), window, cx);
                for action in [
                    TabAction::Color,
                    TabAction::Rename,
                    TabAction::Duplicate,
                    TabAction::Export,
                    TabAction::Find,
                ] {
                    assert!(!app.tab_action_enabled(0, action, cx));
                }
                app.export_text("첫 줄\n서버 🙂\n".into(), dir.clone(), cx);
            })
            .unwrap();
        assert!(cx.did_prompt_for_new_path());
        let path = dir.join("내보내기.txt");
        std::fs::write(&path, "old contents").unwrap();
        cx.simulate_new_path_selection(|_| Some(path.clone()));
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "첫 줄\n서버 🙂\n");

        window
            .update(cx, |app, _, cx| {
                app.export_text("discard".into(), dir.clone(), cx)
            })
            .unwrap();
        cx.simulate_new_path_selection(|_| None);
        cx.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "첫 줄\n서버 🙂\n");

        window
            .update(cx, |app, _, cx| {
                app.export_text("fail".into(), dir.clone(), cx)
            })
            .unwrap();
        cx.simulate_new_path_selection(|_| Some(path.join("file.txt")));
        cx.run_until_parked();
        window
            .update(cx, |app, _, cx| {
                assert!(
                    app.status
                        .as_deref()
                        .unwrap()
                        .contains("텍스트 내보내기 실패")
                );
                for tab in &app.tabs {
                    if let Some(terminal) = &tab.terminal {
                        terminal.update(cx, |view, _| view.shutdown());
                    }
                }
            })
            .unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join("session.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}

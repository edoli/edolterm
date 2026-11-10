use super::*;
use alacritty_terminal::grid::Scroll;
use keybindings::{Action, DEFINITIONS};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsPage {
    General,
    Shortcuts,
}

impl TerminalApp {
    pub(super) fn shortcut_label(&self, action: Action) -> String {
        DEFINITIONS
            .iter()
            .find(|d| d.action == action)
            .map(|d| {
                keybindings::keys(&self.settings.keybindings, d)
                    .iter()
                    .map(|key| keybindings::label(key))
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default()
    }

    pub(super) fn run_shortcut(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            Action::NewTab => self.new_default(window, cx),
            Action::DuplicateTab => {
                let Some(terminal) = self.active_terminal() else {
                    return false;
                };
                let view = terminal.read(cx);
                let profile = view.profile.clone();
                let cwd = view.working_directory();
                self.add_terminal(profile, Some(cwd), window, cx);
            }
            Action::NewWindow => {
                if let Err(error) = windows::open_default(cx) {
                    self.status = Some(error.to_string());
                }
            }
            Action::CloseTab => {
                if let Some(tab) = self.tabs.get(self.active) {
                    self.close_tab(tab.id, window, cx);
                }
            }
            Action::CloseWindow => {
                self.should_close(window, cx);
            }
            Action::NextTab | Action::PreviousTab => {
                let n = self.tabs.len();
                if n > 0 {
                    self.activate(
                        (self.active + if action == Action::NextTab { 1 } else { n - 1 }) % n,
                        window,
                        cx,
                    );
                }
            }
            Action::SwitchTab(index) => self.activate(index, window, cx),
            Action::LastTab => self.activate(self.tabs.len().saturating_sub(1), window, cx),
            Action::NewProfile(index) => {
                if let Some(profile) = self.profiles.get(index).cloned() {
                    self.add_terminal(profile, None, window, cx);
                }
            }
            Action::ProfileMenu => {
                self.menu = !self.menu;
                self.menu_index = 0;
                if self.menu {
                    self.focus.focus(window, cx);
                } else {
                    self.restore_focus(window, cx);
                }
            }
            Action::Settings => self.settings_tab(window, cx),
            Action::SettingsFile => {
                let path = self.dir.join("settings.toml");
                let result = if path.exists() {
                    Ok(())
                } else {
                    self.settings.save(&self.dir)
                }
                .and_then(|_| platform::open_file(&path));
                if let Err(error) = result {
                    self.status = Some(error.to_string());
                }
            }
            Action::Find => self.show_search(window, cx),
            Action::Fullscreen => window.toggle_fullscreen(),
            action => {
                let Some(terminal) = self.active_terminal() else {
                    return false;
                };
                // Leave text fields' clipboard and editing keys to their input handler.
                if !terminal.read(cx).focus.is_focused(window) {
                    return false;
                }
                let handled = terminal.update(cx, |view, cx| {
                    match action {
                        Action::Copy => {
                            let selected = view.session.as_ref().is_some_and(|s| {
                                s.terminal
                                    .lock()
                                    .selection_to_string()
                                    .is_some_and(|text| !text.is_empty())
                            });
                            if !selected {
                                return false;
                            }
                            view.copy(cx);
                            if let Some(s) = &view.session {
                                s.terminal.lock().selection = None;
                            }
                        }
                        Action::Paste => view.paste(cx),
                        Action::SelectAll => view.select_all(cx),
                        Action::ScrollUp
                        | Action::ScrollDown
                        | Action::PageUp
                        | Action::PageDown
                        | Action::ScrollTop
                        | Action::ScrollBottom => {
                            let Some(session) = &view.session else {
                                return false;
                            };
                            let scroll = match action {
                                Action::ScrollUp => Scroll::Delta(platform::keyboard_scroll_lines(
                                    session.size.rows,
                                )),
                                Action::ScrollDown => Scroll::Delta(
                                    -platform::keyboard_scroll_lines(session.size.rows),
                                ),
                                Action::PageUp => Scroll::PageUp,
                                Action::PageDown => Scroll::PageDown,
                                Action::ScrollTop => Scroll::Top,
                                _ => Scroll::Bottom,
                            };
                            session.terminal.lock().scroll_display(scroll);
                        }
                        Action::IncreaseFont => {
                            view.settings.font_size = (view.settings.font_size + 1.).min(48.)
                        }
                        Action::DecreaseFont => {
                            view.settings.font_size = (view.settings.font_size - 1.).max(8.)
                        }
                        Action::ResetFont => view.settings.font_size = self.settings.font_size,
                        _ => return false,
                    }
                    cx.notify();
                    true
                });
                if !handled {
                    return false;
                }
            }
        }
        cx.notify();
        true
    }

    pub(super) fn start_shortcut_capture(
        &mut self,
        id: &'static str,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shortcut_capture = Some((id, index));
        self.status = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn set_shortcut_keys(
        &mut self,
        id: &'static str,
        keys: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut overrides = self.draft.keybindings.clone();
        if let Some(keys) = keys {
            overrides.insert(id.into(), keys);
        } else {
            overrides.remove(id);
        }
        if let Err(error) =
            Bindings::new(&overrides).and_then(|b| b.validate_layout(cx.keyboard_mapper().as_ref()))
        {
            self.status = Some(error.to_string());
            cx.notify();
            return false;
        }
        self.draft.keybindings = overrides;
        self.status = Some("단축키를 변경했습니다. ‘설정 저장’을 누르면 적용됩니다.".into());
        cx.notify();
        true
    }

    pub(super) fn capture_shortcut(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
        if key.key == "escape" {
            self.shortcut_capture = None;
            cx.notify();
            return;
        }
        if key.is_modifier_key() {
            return;
        }
        let Some((id, index)) = self.shortcut_capture else {
            return;
        };
        let mapped = cx.keyboard_mapper().map_key_equivalent(key.clone(), false);
        let text = Keystroke {
            key: mapped.key().to_owned(),
            modifiers: *mapped.modifiers(),
            key_char: None,
        }
        .unparse(OperatingSystem::Windows);
        if let Err(error) = keybindings::parse(&text) {
            self.status = Some(error.to_string());
            cx.notify();
            return;
        }
        let definition = DEFINITIONS.iter().find(|d| d.id == id).unwrap();
        let mut keys = keybindings::keys(&self.draft.keybindings, definition);
        if let Some(index) = index {
            keys[index] = text;
        } else {
            keys.push(text);
        }
        if self.set_shortcut_keys(id, Some(keys), cx) {
            self.shortcut_capture = None;
        }
    }

    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let nav = div()
            .w(px(168.))
            .h_full()
            .flex_shrink_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(16.))
                    .font_weight(FontWeight::BOLD)
                    .child("설정"),
            )
            .children(
                [
                    (SettingsPage::General, "일반"),
                    (SettingsPage::Shortcuts, "단축키"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (page, label))| {
                    button(("settings-page", index), label, theme)
                        .justify_start()
                        .px_3()
                        .py_2()
                        .bg(if self.settings_page == page {
                            theme.hover
                        } else {
                            theme.bg
                        })
                        .aria_selected(self.settings_page == page)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings_page = page;
                            this.shortcut_capture = None;
                            cx.notify();
                        }))
                }),
            );
        let content = match self.settings_page {
            SettingsPage::General => self.general_settings_view(cx),
            SettingsPage::Shortcuts => self.shortcuts_view(cx),
        };
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(nav)
            .child(content)
            .into_any_element()
    }

    fn shortcuts_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let mut rows = div().flex().flex_col().gap_1();
        let mut group = "";
        for (row, definition) in DEFINITIONS.iter().enumerate() {
            if definition.group != group {
                group = definition.group;
                rows = rows.child(
                    div()
                        .pt_5()
                        .pb_2()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child(group),
                );
            }
            let id = definition.id;
            let keys = keybindings::keys(&self.draft.keybindings, definition);
            let mut chips = div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_end()
                .gap_2();
            if keys.is_empty() {
                chips = chips.child(div().text_color(theme.muted).child("지정 안 됨"));
            }
            for (index, key) in keys.iter().enumerate() {
                let recording = self.shortcut_capture == Some((id, Some(index)));
                chips = chips.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(30.))
                        .bg(theme.panel)
                        .rounded_md()
                        .overflow_hidden()
                        .border_1()
                        .border_color(if recording {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .child(
                            button(
                                ("shortcut-edit", row * 100 + index),
                                if recording {
                                    "키 입력 중…".into()
                                } else {
                                    keybindings::label(key)
                                },
                                theme,
                            )
                            .px_3()
                            .h_full()
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.start_shortcut_capture(id, Some(index), window, cx)
                                },
                            )),
                        )
                        .child(div().w(px(1.)).h(px(12.)).bg(theme.border))
                        .child(
                            div()
                                .id(("shortcut-remove", row * 100 + index))
                                .role(Role::Button)
                                .aria_label("단축키 삭제")
                                .tooltip(tip("단축키 삭제".into()))
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(28.))
                                .h_full()
                                .flex_shrink_0()
                                .cursor_pointer()
                                .bg(theme.danger.normal.bg)
                                .text_color(theme.danger.normal.fg)
                                .hover(move |s| {
                                    s.bg(theme.danger.hover.bg)
                                        .text_color(theme.danger.hover.fg)
                                })
                                .child(icon(Icon::Trash, 14.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let definition =
                                        DEFINITIONS.iter().find(|d| d.id == id).unwrap();
                                    let mut keys =
                                        keybindings::keys(&this.draft.keybindings, definition);
                                    keys.remove(index);
                                    this.shortcut_capture = None;
                                    this.set_shortcut_keys(id, Some(keys), cx);
                                })),
                        ),
                );
            }
            chips = chips.child(
                settings_button(
                    ("shortcut-add", row),
                    if self.shortcut_capture == Some((id, None)) {
                        "키 입력 중…"
                    } else {
                        "+ 추가"
                    },
                    theme,
                )
                .px_2()
                .h(px(30.))
                .text_color(theme.muted)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_shortcut_capture(id, None, window, cx)
                })),
            );
            if self.draft.keybindings.contains_key(id) {
                chips = chips.child(
                    settings_button(("shortcut-reset", row), "기본값", theme)
                        .px_2()
                        .h(px(30.))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.shortcut_capture = None;
                            this.set_shortcut_keys(id, None, cx);
                        })),
                );
            }
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .min_w(px(135.))
                            .flex_shrink_0()
                            .child(definition.label),
                    )
                    .child(chips),
            );
        }
        let content = div().w_full().max_w(px(1000.)).mx_auto().p_8()
            .child(div().text_size(px(25.)).font_weight(FontWeight::BOLD).child("단축키"))
            .child(div().pt_2().text_color(theme.muted).child("Windows Terminal의 기본 키를 사용합니다. 구현된 기능만 표시됩니다."))
            .child(div().pt_2().text_color(theme.muted).text_size(px(12.)).child("키를 클릭해 변경하거나 ‘+ 추가’를 누른 뒤 키 조합을 누르세요. Esc는 입력 취소입니다."))
            .child(div().pt_2().text_color(theme.muted).text_size(px(12.)).child("Enter 복사는 터미널에 선택 영역이 있을 때만 작동합니다. 글꼴 크기 단축키는 현재 탭에 적용됩니다."))
            .child(rows);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .id("shortcuts-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.shortcuts_scroll)
                    .child(content),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_6()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        settings_button("reset-shortcuts", "모두 기본값으로", theme).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.draft.keybindings.clear();
                                this.shortcut_capture = None;
                                this.status =
                                    Some("기본 단축키로 되돌렸습니다. 저장하면 적용됩니다.".into());
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        primary_settings_button("save-shortcuts", "설정 저장", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
                    ),
            )
            .into_any_element()
    }
}

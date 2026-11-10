use super::*;

const DRAG_THRESHOLD: f32 = 5.;
const SLIDE_DURATION: Duration = Duration::from_millis(140);

pub(super) struct TabRename {
    id: u64,
    pub(super) input: Entity<TextField>,
    _subscription: Subscription,
}

impl Tab {
    pub(super) fn title(&self, cx: &App) -> String {
        self.custom_title.clone().unwrap_or_else(|| {
            self.terminal
                .as_ref()
                .map(|terminal| terminal.read(cx).title.clone())
                .unwrap_or_else(|| "설정".into())
        })
    }
}

pub(super) struct TabDrag {
    pub id: u64,
    pub started: bool,
    start: Point<Pixels>,
    pointer: Point<Pixels>,
    grab_offset: f32,
    original_order: Vec<u64>,
    original_scroll: Point<Pixels>,
    last_frame: Instant,
}

pub(super) struct TabMotion {
    id: u64,
    offset: f32,
    started: Instant,
}

impl TabMotion {
    fn offset(&self, now: Instant) -> f32 {
        let progress = (now.duration_since(self.started).as_secs_f32()
            / SLIDE_DURATION.as_secs_f32())
        .clamp(0., 1.);
        self.offset * (1. - progress).powi(3)
    }
}

impl TerminalApp {
    pub(super) fn update_window_title(&self, window: &mut Window, cx: &App) {
        if let Some(tab) = self.tabs.get(self.active) {
            window.set_window_title(&format!("{} — edolterm", tab.title(cx)));
        }
    }

    fn begin_tab_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.id == id && tab.terminal.is_some())
        else {
            return;
        };
        self.finish_tab_drag(true, window, cx);
        self.activate(index, window, cx);
        let title = self.tabs[index].title(cx);
        let input = cx.new(|cx| TextField::new(title, "탭 이름", cx));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &FieldEvent, window, cx| match event {
                FieldEvent::Submit => this.finish_tab_rename(true, window, cx),
                FieldEvent::Cancel => this.finish_tab_rename(false, window, cx),
                FieldEvent::Changed => {}
            },
        );
        input.update(cx, |input, cx| input.select_all(window, cx));
        self.tab_rename = Some(TabRename {
            id,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn finish_tab_rename(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.tab_rename.take() else {
            return;
        };
        if save {
            let title = rename.input.read(cx).text.trim().to_owned();
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == rename.id) {
                tab.custom_title = (!title.is_empty()).then_some(title);
            }
            self.update_window_title(window, cx);
            self.persist(window, cx);
        }
        self.restore_focus(window, cx);
        cx.notify();
    }

    pub(super) fn tab_rename_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(rename) = &self.tab_rename else {
            return false;
        };
        if event.prefer_character_input || rename.input.read(cx).is_composing() {
            return true;
        }
        if event.keystroke.key == "escape" {
            self.finish_tab_rename(false, window, cx);
            cx.stop_propagation();
        }
        true
    }

    pub(super) fn tab_rename_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let rename = self.tab_rename.as_ref().expect("rename dialog is open");
        div()
            .id("rename-tab-backdrop")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.finish_tab_rename(false, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("rename-tab-dialog")
                    .flex()
                    .flex_col()
                    .gap_4()
                    .w(px(420.))
                    .max_w_full()
                    .p_6()
                    .rounded_lg()
                    .bg(theme.panel)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .child("탭 이름 변경"),
                    )
                    .child(rename.input.clone())
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.muted)
                            .child("이름을 비우면 셸이 제공하는 자동 제목을 사용합니다."),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                settings_button("cancel-rename-tab", "취소", theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.finish_tab_rename(false, window, cx)
                                    }),
                                ),
                            )
                            .child(
                                primary_settings_button("save-rename-tab", "저장", theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.finish_tab_rename(true, window, cx)
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn tab_inset() -> f32 {
        if cfg!(target_os = "macos") { 78. } else { 0. }
    }

    pub(super) fn begin_tab_drag(
        &mut self,
        id: u64,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let (_, width, _) = self.tab_dimensions(window);
        let scroll = self.tab_scroll.offset();
        let grab_offset = f32::from(event.position.x)
            - Self::tab_inset()
            - f32::from(scroll.x)
            - index as f32 * width
            - self.tab_slide_offset(id);
        self.activate(index, window, cx);
        self.tab_drag = Some(TabDrag {
            id,
            started: false,
            start: event.position,
            pointer: event.position,
            grab_offset: grab_offset.clamp(0., width),
            original_order: self.tabs.iter().map(|tab| tab.id).collect(),
            original_scroll: scroll,
            last_frame: Instant::now(),
        });
        cx.stop_propagation();
    }

    /// The floating tab stays inside the strip, even if the pointer leaves it.
    pub(super) fn dragged_tab_left(&self, window: &Window) -> f32 {
        let (_, width, extent) = self.tab_dimensions(window);
        let Some(drag) = &self.tab_drag else {
            return 0.;
        };
        (f32::from(drag.pointer.x) - Self::tab_inset() - drag.grab_offset)
            .clamp(0., (extent - width).max(0.))
    }

    pub(super) fn tab_slide_offset(&self, id: u64) -> f32 {
        self.tab_motions
            .iter()
            .find(|motion| motion.id == id)
            .map_or(0., |motion| motion.offset(Instant::now()))
    }

    fn update_tab_drag(&mut self, window: &Window) {
        let Some(drag) = self.tab_drag.as_ref().filter(|drag| drag.started) else {
            return;
        };
        let id = drag.id;
        let Some(from) = self.tabs.iter().position(|tab| tab.id == id) else {
            self.tab_drag = None;
            return;
        };
        let (_, width, _) = self.tab_dimensions(window);
        let content_left = self.dragged_tab_left(window) - f32::from(self.tab_scroll.offset().x);
        let target = ((content_left / width).round().max(0.) as usize)
            .min(self.tabs.len().saturating_sub(1));
        if from == target {
            return;
        }
        let now = Instant::now();
        let positions: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| (tab.id, index as f32 * width + self.tab_slide_offset(tab.id)))
            .collect();
        let active_id = self.tabs[self.active].id;
        let tab = self.tabs.remove(from);
        self.tabs.insert(target, tab);
        self.active = self
            .tabs
            .iter()
            .position(|tab| tab.id == active_id)
            .unwrap_or(0);
        self.tab_motions = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.id != id)
            .filter_map(|(index, tab)| {
                let old = positions.iter().find(|(id, _)| *id == tab.id)?.1;
                let offset = old - index as f32 * width;
                (offset.abs() > 0.5).then_some(TabMotion {
                    id: tab.id,
                    offset,
                    started: now,
                })
            })
            .collect();
    }

    pub(super) fn move_tab_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            self.finish_tab_drag(false, window, cx);
            return;
        }
        let Some(drag) = self.tab_drag.as_mut() else {
            return;
        };
        drag.pointer = event.position;
        if !drag.started {
            if f32::from(event.position.x - drag.start.x).abs() < DRAG_THRESHOLD {
                return;
            }
            drag.started = true;
            drag.last_frame = Instant::now();
            self.tab_motions.retain(|motion| motion.id != drag.id);
        }
        self.update_tab_drag(window);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn animate_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.tab_motions
            .retain(|motion| now.duration_since(motion.started) < SLIDE_DURATION);
        let (_, width, extent) = self.tab_dimensions(window);
        let max_scroll = (self.tabs.len() as f32 * width - extent).max(0.);
        let mut scrolling = false;
        if let Some(drag) = self.tab_drag.as_mut().filter(|drag| drag.started) {
            let dt = now.duration_since(drag.last_frame).as_secs_f32().min(0.05);
            drag.last_frame = now;
            let pointer = f32::from(drag.pointer.x) - Self::tab_inset();
            let edge = 40_f32.min(extent / 3.);
            let velocity = if pointer < edge {
                -600. * ((edge - pointer) / edge).clamp(0., 1.)
            } else if pointer > extent - edge {
                600. * ((pointer - (extent - edge)) / edge).clamp(0., 1.)
            } else {
                0.
            };
            let old = self.tab_scroll.offset();
            let next = (f32::from(old.x) - velocity * dt).clamp(-max_scroll, 0.);
            scrolling = velocity != 0. && next != f32::from(old.x);
            if scrolling {
                self.tab_scroll.set_offset(point(px(next), px(0.)));
                self.update_tab_drag(window);
            }
        }
        if cx.reduce_motion() {
            self.tab_motions.clear();
        }
        if scrolling || !self.tab_motions.is_empty() {
            window.request_animation_frame();
        }
    }

    pub(super) fn finish_tab_drag(
        &mut self,
        cancel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let left = self.dragged_tab_left(window) - f32::from(self.tab_scroll.offset().x);
        let Some(drag) = self.tab_drag.take() else {
            return;
        };
        if drag.started {
            if cancel {
                let active_id = self.tabs.get(self.active).map(|tab| tab.id);
                self.tabs.sort_by_key(|tab| {
                    drag.original_order
                        .iter()
                        .position(|id| *id == tab.id)
                        .unwrap_or(usize::MAX)
                });
                self.active = self
                    .tabs
                    .iter()
                    .position(|tab| Some(tab.id) == active_id)
                    .unwrap_or(0);
                self.tab_scroll.set_offset(drag.original_scroll);
                self.tab_motions.clear();
            } else if let Some(index) = self.tabs.iter().position(|tab| tab.id == drag.id) {
                let (_, width, _) = self.tab_dimensions(window);
                self.tab_motions.push(TabMotion {
                    id: drag.id,
                    offset: left - index as f32 * width,
                    started: Instant::now(),
                });
                self.persist(window, cx);
            }
            cx.stop_propagation();
        }
        cx.notify();
    }

    pub(super) fn tab_element(
        &self,
        tab: &Tab,
        index: usize,
        width: f32,
        floating: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let id = tab.id;
        let active = index == self.active;
        let title = tab.title(cx);
        let (leading, bell) = if let Some(terminal) = &tab.terminal {
            let terminal = terminal.read(cx);
            (
                icons::profiles::element(&terminal.profile.program, 16., cx),
                terminal.bell,
            )
        } else {
            (icon(Icon::Settings, 16.).into_any_element(), false)
        };
        div()
            .id(("tab", id))
            .role(Role::Tab)
            .aria_label(title.clone())
            .aria_selected(active)
            .relative()
            .flex()
            .items_center()
            .h_full()
            .w(px(width))
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .border_r_1()
            .border_color(theme.border)
            .bg(if active { theme.bg } else { theme.bar })
            .when(!floating, |tab| {
                let shortcuts = [
                    Some(keybindings::Action::SwitchTab(index)),
                    (index + 1 == self.tabs.len()).then_some(keybindings::Action::LastTab),
                ]
                .into_iter()
                .flatten()
                .map(|action| self.shortcut_label(action))
                .filter(|label| !label.is_empty())
                .collect::<Vec<_>>()
                .join(" / ");
                let tooltip = if shortcuts.is_empty() {
                    title.clone()
                } else {
                    format!("{title} · {shortcuts}")
                };
                tab.hover(move |s| s.bg(if active { theme.bg } else { theme.hover }))
                    .tooltip(tip(tooltip))
            })
            .when(floating, |tab| tab.shadow_md().border_l_1())
            .cursor_default()
            .text_size(px(12.))
            .text_color(if active { theme.fg } else { theme.muted })
            .when(active, |tab| {
                tab.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top_0()
                        .h(px(2.))
                        .bg(theme.accent),
                )
            })
            .child(
                div()
                    .relative()
                    .text_color(if bell { theme.bell } else { theme.accent })
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(16.))
                    .flex_shrink_0()
                    .child(leading)
                    .when(bell, |icon| {
                        icon.child(
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .size(px(4.))
                                .rounded_full()
                                .bg(theme.bell),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(title),
            )
            .child(
                div()
                    .id(("close-tab", id))
                    .role(Role::Button)
                    .aria_label("탭 닫기")
                    .when(!floating, |button| {
                        let shortcut = self.shortcut_label(keybindings::Action::CloseTab);
                        let tooltip = if shortcut.is_empty() {
                            "탭 닫기".into()
                        } else {
                            format!("탭 닫기 · {shortcut}")
                        };
                        button
                            .block_mouse_except_scroll()
                            .tooltip(tip(tooltip))
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.))
                    .flex_shrink_0()
                    .rounded_sm()
                    .hover(move |s| s.bg(theme.hover))
                    .child(icon(Icon::Close, 14.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, window, cx| {
                            this.begin_tab_rename(id, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, window, cx| {
                            this.close_tab(id, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_tab(id, window, cx);
                        cx.stop_propagation();
                    })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count == 2 && id != 0 {
                        this.begin_tab_rename(id, window, cx);
                        cx.stop_propagation();
                    } else {
                        this.begin_tab_drag(id, event, window, cx);
                    }
                }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.tab_rename.is_some() {
                    return;
                }
                if let Some(index) = this.tabs.iter().position(|tab| tab.id == id) {
                    this.activate(index, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, window, cx| {
                    this.begin_tab_rename(id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    this.close_tab(id, window, cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// Capture movement/release across the entire window, including outside the strip.
    pub(super) fn tab_drag_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let movement = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        movement.update(cx, |this, cx| this.move_tab_drag(event, window, cx));
                    }
                });
                let release = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        release.update(cx, |this, cx| this.finish_tab_drag(false, window, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}

//! Settings selectors. Font discovery is demand-driven, shared between windows,
//! and never scheduled by application or settings-view initialization.
use crate::{
    icons::{Icon, icon},
    text_field::{FieldEvent, TextField},
    theme::Theme,
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc, sync::Arc};

#[derive(Clone)]
pub struct Choice {
    pub value: String,
    pub label: SharedString,
    search: String,
    icon_program: Option<String>,
}
impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            value: value.into(),
            search: label.to_lowercase(),
            label,
            icon_program: None,
        }
    }
    pub fn with_profile_icon(mut self, program: String) -> Self {
        self.icon_program = Some(program);
        self
    }
}

#[derive(Default)]
struct FontCatalog {
    loading: bool,
    names: Option<Arc<[Choice]>>,
}
impl Global for FontCatalog {}

pub struct Selected(pub String);
pub struct Select {
    label: &'static str,
    items: Arc<[Choice]>,
    filtered: Vec<usize>,
    pub selected: String,
    pub open: bool,
    fonts: bool,
    focus: FocusHandle,
    pub search: Entity<TextField>,
    cursor: usize,
    scroll: UniformListScrollHandle,
    trigger: Rc<Cell<Bounds<Pixels>>>,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<Selected> for Select {}

impl Select {
    pub fn new(
        label: &'static str,
        items: Vec<Choice>,
        selected: String,
        fonts: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| TextField::new("", "글꼴 검색", cx));
        let focus = cx.focus_handle().tab_stop(true);
        let mut subscriptions = vec![
            cx.subscribe(&search, |this, _, event, cx| {
                if matches!(event, FieldEvent::Changed) {
                    this.refilter(cx);
                    cx.notify();
                }
            }),
            cx.on_focus_out(&focus, window, |this, _, window, cx| {
                // On the first opening the search field is not in the rendered
                // focus tree yet. Moving focus into it must keep the popup open.
                if !this.search.read(cx).focus.is_focused(window) {
                    this.close(cx);
                }
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.close(cx);
                }
            }),
        ];
        if fonts {
            subscriptions.push(cx.observe_global::<FontCatalog>(|this, cx| {
                if let Some(items) = cx
                    .try_global::<FontCatalog>()
                    .and_then(|catalog| catalog.names.clone())
                {
                    this.set_items(items, cx);
                    cx.notify();
                }
            }));
        }
        let mut this = Self {
            label,
            filtered: Vec::new(),
            items: items.into(),
            selected,
            open: false,
            fonts,
            focus,
            search,
            cursor: 0,
            scroll: UniformListScrollHandle::new(),
            trigger: Rc::new(Cell::new(Bounds::default())),
            _subscriptions: subscriptions,
        };
        this.refilter(cx);
        this
    }

    fn set_items(&mut self, items: Arc<[Choice]>, cx: &App) {
        self.items = if items.iter().any(|item| item.value == self.selected) {
            items
        } else {
            let mut items = items.to_vec();
            items.push(Choice::new(self.selected.clone(), self.selected.clone()));
            items.sort_by(|a, b| a.search.cmp(&b.search));
            items.into()
        };
        self.refilter(cx);
    }

    pub fn replace_choices(&mut self, items: Vec<Choice>, cx: &mut Context<Self>) {
        self.set_items(items.into(), cx);
        cx.notify();
    }

    fn refilter(&mut self, cx: &App) {
        let query = self.search.read(cx).text.trim().to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| item.search.contains(&query).then_some(i))
            .collect();
        self.cursor = self
            .filtered
            .iter()
            .position(|i| self.items[*i].value == self.selected)
            .unwrap_or(0);
        self.scroll.scroll_to_item(self.cursor, ScrollStrategy::Top);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(cx);
            self.focus.focus(window, cx);
            return;
        }
        self.open = true;
        self.focus.focus(window, cx);
        self.search.update(cx, |search, cx| {
            search.set_text("", window, cx);
            if self.fonts {
                search.focus.focus(window, cx);
            }
        });
        if self.fonts {
            self.load_fonts(cx);
        }
        self.refilter(cx);
        cx.notify();
    }

    fn load_fonts(&mut self, cx: &mut Context<Self>) {
        let catalog = cx.default_global::<FontCatalog>();
        if let Some(names) = catalog.names.clone() {
            self.set_items(names, cx);
            return;
        }
        if catalog.loading {
            return;
        }
        catalog.loading = true;
        let text_system = cx.text_system().clone();
        let task = cx.background_executor().spawn(async move {
            text_system
                .all_font_names()
                .into_iter()
                .map(|name| Choice::new(name.clone(), name))
                .collect::<Arc<[Choice]>>()
        });
        // Discovery belongs to the process cache, not the window that requested it.
        cx.spawn(async move |_, cx| {
            let names = task.await;
            cx.update(|cx| {
                cx.set_global(FontCatalog {
                    loading: false,
                    names: Some(names),
                })
            });
        })
        .detach();
    }

    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let Some(item) = self.filtered.get(index).map(|i| &self.items[*i]) else {
            return;
        };
        self.selected = item.value.clone();
        self.close(cx);
        self.focus.focus(window, cx);
        cx.emit(Selected(self.selected.clone()));
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.prefer_character_input || self.search.read(cx).is_composing() {
            return;
        }
        let search_focused = self.search.read(cx).focus.is_focused(window);
        match event.keystroke.key.as_str() {
            "escape" if self.open => {
                self.close(cx);
                self.focus.focus(window, cx);
            }
            "tab" if self.open => {
                self.close(cx);
                self.focus.focus(window, cx);
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
            }
            "enter" if self.open => self.choose(self.cursor, window, cx),
            "space" if self.open && !search_focused => self.choose(self.cursor, window, cx),
            "enter" | "space" | "up" | "down" if !self.open => self.toggle(window, cx),
            key @ ("up" | "down" | "pageup" | "pagedown") if self.open => {
                let last = self.filtered.len().saturating_sub(1);
                self.cursor = match key {
                    "up" => self.cursor.saturating_sub(1),
                    "down" => (self.cursor + 1).min(last),
                    "pageup" => self.cursor.saturating_sub(7),
                    _ => (self.cursor + 7).min(last),
                };
                self.scroll
                    .scroll_to_item(self.cursor, ScrollStrategy::Center);
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    #[cfg(all(debug_assertions, feature = "ui-tests"))]
    pub fn font_discovery_started(cx: &App) -> bool {
        cx.try_global::<FontCatalog>().is_some()
    }

    #[cfg(all(debug_assertions, feature = "ui-tests"))]
    pub fn font_count(cx: &App) -> usize {
        cx.try_global::<FontCatalog>()
            .and_then(|catalog| catalog.names.as_ref())
            .map_or(0, |names| names.len())
    }

    #[cfg(all(debug_assertions, feature = "ui-tests"))]
    pub fn visible_values(&self) -> Vec<String> {
        self.filtered
            .iter()
            .map(|i| self.items[*i].value.clone())
            .collect()
    }

    #[cfg(all(debug_assertions, feature = "ui-tests"))]
    pub fn trigger_bounds(&self) -> Bounds<Pixels> {
        self.trigger.get()
    }
}

impl Render for Select {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::current(cx);
        let label = self
            .items
            .iter()
            .find(|item| item.value == self.selected)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| self.selected.clone().into());
        let icon_program = self
            .items
            .iter()
            .find(|item| item.value == self.selected)
            .and_then(|item| item.icon_program.clone());
        let mut root = div()
            .id("select")
            .w_full()
            .track_focus(&self.focus)
            .tab_index(0)
            .tab_stop(true)
            .capture_key_down(cx.listener(Self::key))
            .child(
                canvas(
                    {
                        let trigger = self.trigger.clone();
                        move |bounds, _, _| trigger.set(bounds)
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .relative()
            .child(
                div()
                    .id("select-trigger")
                    .role(Role::Button)
                    .aria_label(self.label)
                    .aria_value(label.clone())
                    .aria_expanded(self.open)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .h(px(36.))
                    .w_full()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if self.open || self.focus.is_focused(window) {
                        theme.accent
                    } else {
                        theme.border
                    })
                    .bg(theme.hover)
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.panel))
                    .when_some(icon_program, |trigger, program| {
                        trigger.child(crate::icons::profiles::element(&program, 16., cx))
                    })
                    .child(div().flex_1().min_w_0().text_ellipsis().child(label))
                    .child(icon(Icon::ChevronDown, 14.))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx))),
            );
        if self.open {
            let loading = self.fonts
                && cx
                    .try_global::<FontCatalog>()
                    .is_some_and(|catalog| catalog.loading);
            let mut popup = div()
                .id("select-popup")
                .occlude()
                .w(self.trigger.get().size.width.max(px(160.)))
                .flex()
                .flex_col()
                .gap_1()
                .p_1()
                .rounded_md()
                .shadow_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.bar)
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    if !this.trigger.get().contains(&event.position) {
                        this.close(cx);
                    }
                }));
            if self.fonts {
                popup = popup.child(self.search.clone());
            }
            if loading {
                popup = popup.child(
                    div()
                        .p_2()
                        .text_color(theme.muted)
                        .child("글꼴 목록 불러오는 중…"),
                );
            }
            if self.filtered.is_empty() {
                popup = popup.child(
                    div()
                        .p_2()
                        .text_color(theme.muted)
                        .child("검색 결과가 없습니다."),
                );
            } else {
                popup = popup.child(
                    uniform_list(
                        "select-options",
                        self.filtered.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            let theme = Theme::current(cx);
                            range
                                .map(|index| {
                                    let item = &this.items[this.filtered[index]];
                                    div()
                                        .id(index)
                                        .w_full()
                                        .role(Role::Button)
                                        .aria_label(item.label.clone())
                                        .aria_selected(item.value == this.selected)
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .h(px(34.))
                                        .px_2()
                                        .rounded_sm()
                                        .overflow_hidden()
                                        .cursor_pointer()
                                        .when(index == this.cursor, |row| {
                                            row.bg(theme.input.selection)
                                        })
                                        .hover(move |style| style.bg(theme.hover))
                                        .when_some(item.icon_program.as_ref(), |row, program| {
                                            row.child(crate::icons::profiles::element(
                                                program, 16., cx,
                                            ))
                                        })
                                        .child(div().text_ellipsis().child(item.label.clone()))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.choose(index, window, cx)
                                        }))
                                })
                                .collect()
                        }),
                    )
                    .w_full()
                    .h(px((self.filtered.len().min(7) as f32) * 34.))
                    .track_scroll(&self.scroll),
                );
            }
            root = root.child(
                deferred(
                    anchored()
                        .attach(self.trigger.get(), Anchor::BottomLeft)
                        .offset(point(px(0.), px(4.)))
                        .snap_to_window_with_margin(px(8.))
                        .child(popup),
                )
                .with_priority(5),
            );
        }
        root
    }
}

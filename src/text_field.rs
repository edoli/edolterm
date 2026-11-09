use crate::{config::SharedSettings, theme::Theme};
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub enum FieldEvent {
    Changed,
    Submit,
    Cancel,
}

pub struct TextField {
    pub text: String,
    pub focus: FocusHandle,
    pub placeholder: String,
    selected: Range<usize>,
    marked: Option<Range<usize>>,
    layout: Option<ShapedLine>,
    bounds: Bounds<Pixels>,
    _theme_subscription: Subscription,
}
impl EventEmitter<FieldEvent> for TextField {}

pub fn utf8_offset(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= offset {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

impl TextField {
    pub fn new(text: impl Into<String>, placeholder: &str, cx: &mut Context<Self>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            text,
            focus: cx.focus_handle(),
            placeholder: placeholder.into(),
            selected: len..len,
            marked: None,
            layout: None,
            bounds: Bounds::default(),
            _theme_subscription: cx.observe_global::<SharedSettings>(|_, cx| cx.notify()),
        }
    }
    fn range(&self, r: Range<usize>) -> Range<usize> {
        utf8_offset(&self.text, r.start)..utf8_offset(&self.text, r.end)
    }
    fn utf16(&self, r: Range<usize>) -> Range<usize> {
        self.text[..r.start].encode_utf16().count()..self.text[..r.end].encode_utf16().count()
    }
    pub fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0..self.text.len();
        self.focus.focus(window, cx);
        cx.notify();
    }
    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = e.keystroke.modifiers;
        let command = if cfg!(target_os = "macos") {
            m.platform
        } else {
            m.control
        };
        let cursor = self.selected.end;
        let prev = self.text[..cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
        let next = self.text[cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.text.len(), |(i, _)| cursor + i);
        match e.keystroke.key.as_str() {
            "a" if command => self.selected = 0..self.text.len(),
            "c" | "x" if command => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.text[self.selected.clone()].into(),
                ));
                if e.keystroke.key == "x" {
                    self.replace_text_in_range(None, "", window, cx);
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    self.replace_text_in_range(None, &text, window, cx);
                }
            }
            "left" | "right" | "home" | "end" => {
                let to = match e.keystroke.key.as_str() {
                    "left" => prev,
                    "right" => next,
                    "home" => 0,
                    _ => self.text.len(),
                };
                self.selected = if m.shift {
                    self.selected.start.min(to)..self.selected.start.max(to)
                } else {
                    to..to
                };
            }
            "backspace" if self.marked.is_none() => {
                if self.selected.is_empty() {
                    self.selected = prev..cursor;
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "delete" if self.marked.is_none() => {
                if self.selected.is_empty() {
                    self.selected = cursor..next;
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "enter" if self.marked.is_none() => cx.emit(FieldEvent::Submit),
            "escape" if self.marked.is_none() => cx.emit(FieldEvent::Cancel),
            _ => return,
        }
        window.notify_text_input_changed(&self.focus);
        cx.stop_propagation();
        cx.notify();
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.range(r);
        *actual = Some(self.utf16(r.clone()));
        Some(self.text[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16(self.selected.clone()),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|r| self.utf16(r))
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = r
            .map(|r| self.range(r))
            .or_else(|| self.marked.clone())
            .unwrap_or(self.selected.clone());
        let text = text.replace(['\r', '\n', '\t'], " ");
        self.text.replace_range(r.clone(), &text);
        self.selected = r.start + text.len()..r.start + text.len();
        self.marked = None;
        window.notify_text_input_changed(&self.focus);
        cx.emit(FieldEvent::Changed);
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
            .map(|r| self.range(r))
            .or_else(|| self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.text.replace_range(r.clone(), text);
        self.marked = (!text.is_empty()).then_some(r.start..r.start + text.len());
        self.selected = selected
            .map(|s| r.start + utf8_offset(text, s.start)..r.start + utf8_offset(text, s.end))
            .unwrap_or(r.start + text.len()..r.start + text.len());
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
        let r = self.range(r);
        let line = self.layout.as_ref()?;
        Some(Bounds::new(
            self.bounds.origin + point(line.x_for_index(r.start), px(0.)),
            size(
                (line.x_for_index(r.end) - line.x_for_index(r.start)).max(px(2.)),
                self.bounds.size.height,
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let n = self
            .layout
            .as_ref()?
            .closest_index_for_x(p.x - self.bounds.left());
        Some(self.text[..n.min(self.text.len())].encode_utf16().count())
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Theme::current(cx).input;
        let a = cx.entity();
        let b = a.clone();
        div()
            .id("text-input")
            .role(Role::TextInput)
            .aria_label(self.placeholder.clone())
            .aria_value(self.text.clone())
            .track_focus(&self.focus)
            .overflow_hidden()
            .cursor_text()
            .h(px(30.))
            .w_full()
            .px_2()
            .py_1()
            .border_1()
            .border_color(colors.border)
            .rounded_sm()
            .bg(colors.surface.bg)
            .text_color(colors.surface.fg)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    if let Some(line) = &this.layout {
                        let n = line
                            .closest_index_for_x(e.position.x - this.bounds.left())
                            .min(this.text.len());
                        this.selected = if e.click_count > 1 {
                            0..this.text.len()
                        } else {
                            n..n
                        };
                    }
                    window.notify_text_selection_changed(&this.focus);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let this = a.read(cx);
                        let empty = this.text.is_empty();
                        let text: SharedString = if empty {
                            this.placeholder.clone().into()
                        } else {
                            this.text.clone().into()
                        };
                        let run = TextRun {
                            len: text.len(),
                            font: font(".SystemUIFont"),
                            color: if empty {
                                colors.placeholder
                            } else {
                                colors.surface.fg
                            },
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let line = window.text_system().shape_line(text, px(13.), &[run], None);
                        (bounds, line)
                    },
                    move |_, (bounds, line), window, cx| {
                        let this = b.read(cx);
                        let focus = this.focus.clone();
                        let cursor = this.selected.end;
                        let shift =
                            (line.x_for_index(cursor) - bounds.size.width + px(4.)).max(px(0.));
                        let origin = bounds.origin - point(shift, px(0.));
                        if !this.selected.is_empty() {
                            window.paint_quad(fill(
                                Bounds::new(
                                    origin + point(line.x_for_index(this.selected.start), px(0.)),
                                    size(
                                        line.x_for_index(this.selected.end)
                                            - line.x_for_index(this.selected.start),
                                        bounds.size.height,
                                    ),
                                ),
                                colors.selection,
                            ));
                        }
                        let _ = line.paint(
                            origin,
                            bounds.size.height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                        if focus.is_focused(window) {
                            window.paint_quad(fill(
                                Bounds::new(
                                    origin + point(line.x_for_index(cursor), px(0.)),
                                    size(px(1.), bounds.size.height),
                                ),
                                colors.cursor,
                            ));
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, b.clone()),
                                cx,
                            );
                        }
                        b.update(cx, |this, _| {
                            this.bounds = Bounds::new(origin, bounds.size);
                            this.layout = Some(line);
                        });
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::utf8_offset;
    #[test]
    fn utf16_offsets_do_not_split_surrogates() {
        let s = "a한😀z";
        assert_eq!(utf8_offset(s, 2), 4);
        assert_eq!(utf8_offset(s, 4), 8);
        assert!(s.is_char_boundary(utf8_offset(s, 3)));
        assert_eq!(utf8_offset(s, 100), s.len());
    }
}

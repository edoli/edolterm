use super::*;

const WIDTH: f32 = 12.;

impl TerminalApp {
    pub(super) fn settings_scroll_area(
        &self,
        id: &'static str,
        scroll: &ScrollHandle,
        content: impl IntoElement,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let measured_scroll = scroll.clone();
        let scroll = scroll.clone();
        let entity = cx.entity();
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id(id)
                    .flex_1()
                    .min_h_0()
                    .pr(px(WIDTH))
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .child(content),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        // Read after the scroller's prepaint, including its clamped offset.
                        let max = measured_scroll.max_offset().y;
                        let height = bounds.size.height;
                        if max <= px(0.) || height <= px(0.) {
                            return None;
                        }
                        let thumb_height = (height * (height / (height + max)))
                            .max(px(24.))
                            .min(height);
                        let travel = height - thumb_height;
                        let top = (-measured_scroll.offset().y / max).clamp(0., 1.) * travel;
                        let thumb = Bounds::new(
                            point(bounds.left() + px(3.), bounds.top() + top),
                            size(px(WIDTH - 6.), thumb_height),
                        );
                        let hitbox =
                            window.insert_hitbox(bounds, HitboxBehavior::BlockMouseExceptScroll);
                        Some((thumb, hitbox, max, travel))
                    },
                    move |bounds, state, window, cx| {
                        let Some((thumb, hitbox, max, travel)) = state else {
                            return;
                        };
                        let hovered = hitbox.is_hovered(window);
                        let dragging = entity.read(cx).settings_scroll_drag.is_some();
                        let mut quad = fill(
                            thumb,
                            if hovered || dragging {
                                theme.muted
                            } else {
                                theme.scrollbar
                            },
                        );
                        quad.corner_radii = Corners::all(px(3.));
                        window.paint_quad(quad);

                        let set_position =
                            move |position: Pixels, grab: Pixels, scroll: &ScrollHandle| {
                                if travel > px(0.) {
                                    let ratio =
                                        ((position - bounds.top() - grab) / travel).clamp(0., 1.);
                                    scroll.set_offset(point(px(0.), -max * ratio));
                                }
                            };
                        let pressed_entity = entity.clone();
                        let pressed_scroll = scroll.clone();
                        let pressed_hitbox = hitbox.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase.bubble()
                                && event.button == MouseButton::Left
                                && pressed_hitbox.is_hovered(window)
                            {
                                let on_thumb = thumb.contains(&event.position);
                                let grab = if on_thumb {
                                    event.position.y - thumb.top()
                                } else {
                                    thumb.size.height / 2.
                                };
                                pressed_entity
                                    .update(cx, |app, _| app.settings_scroll_drag = Some(grab));
                                if !on_thumb {
                                    set_position(event.position.y, grab, &pressed_scroll);
                                }
                                window.prevent_default();
                                window.refresh();
                                cx.stop_propagation();
                            }
                        });
                        let moved_entity = entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if !phase.capture() {
                                return;
                            }
                            if hovered != hitbox.is_hovered(window) {
                                window.refresh();
                            }
                            if let Some(grab) = moved_entity.read(cx).settings_scroll_drag {
                                if event.dragging() {
                                    set_position(event.position.y, grab, &scroll);
                                    cx.stop_propagation();
                                } else {
                                    moved_entity
                                        .update(cx, |app, _| app.settings_scroll_drag = None);
                                }
                                window.refresh();
                            }
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase.capture()
                                && event.button == MouseButton::Left
                                && entity.read(cx).settings_scroll_drag.is_some()
                            {
                                entity.update(cx, |app, _| app.settings_scroll_drag = None);
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .right_0()
                .w(px(WIDTH))
                .h_full(),
            )
    }
}

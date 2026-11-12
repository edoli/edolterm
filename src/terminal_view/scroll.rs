use gpui::{ScrollDelta, ScrollEvent, TouchPhase};

/// Fractional lines only; never queues motion or animates after input ends.
#[derive(Default)]
pub(super) struct ScrollLines {
    remainder: f32,
}

impl ScrollLines {
    pub(super) fn update(&mut self, event: &ScrollEvent, cell_height: f32) -> i32 {
        match event.touch_phase {
            TouchPhase::Started => self.remainder = 0.,
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.remainder = 0.;
                return 0;
            }
            TouchPhase::Moved => {}
        }
        let delta = match event.delta {
            // GPUI already applies the OS wheel-lines preference (e.g. 3/notch).
            ScrollDelta::Lines(p) => p.y,
            ScrollDelta::Pixels(p) => f32::from(p.y) / cell_height.max(1.),
        };
        if !delta.is_finite() || delta == 0. {
            return 0;
        }
        if self.remainder.signum() != delta.signum() {
            self.remainder = 0.;
        }
        self.remainder += delta;
        let lines = self.remainder as i32;
        self.remainder -= lines as f32;
        lines
    }
}

/// One font-size step per wheel event; accumulate precise trackpad movement.
#[derive(Default)]
pub(super) struct ScrollZoom(ScrollLines);

impl ScrollZoom {
    pub(super) fn update(&mut self, event: &ScrollEvent) -> i32 {
        let mut event = event.clone();
        if let ScrollDelta::Lines(ref mut delta) = event.delta {
            // Wheel lines include the OS scrolling preference. Zoom should not.
            if delta.y.is_finite() && delta.y != 0. {
                delta.y = delta.y.signum();
            }
        }
        self.0.update(&event, 40.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn zoom_wheel_steps_ignore_scroll_lines_preference_and_horizontal_input() {
        let mut zoom = ScrollZoom::default();
        let mut event = ScrollEvent {
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        };
        for (delta, expected) in [
            (1., 1),
            (3., 1),
            (120., 1),
            (-3., -1),
            (0., 0),
            (f32::NAN, 0),
        ] {
            event.delta = ScrollDelta::Lines(point(3., delta));
            assert_eq!(zoom.update(&event), expected);
        }
    }

    #[test]
    fn zoom_trackpad_accumulates_pixels_and_clears_on_reversal_and_end() {
        let mut zoom = ScrollZoom::default();
        let mut event = ScrollEvent {
            delta: ScrollDelta::Pixels(point(px(0.), px(20.))),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        };
        assert_eq!(zoom.update(&event), 0);
        event.touch_phase = TouchPhase::Moved;
        assert_eq!(zoom.update(&event), 1);
        assert_eq!(zoom.update(&event), 0);
        event.delta = ScrollDelta::Pixels(point(px(0.), px(-20.)));
        assert_eq!(zoom.update(&event), 0);
        assert_eq!(zoom.update(&event), -1);
        event.delta = ScrollDelta::Pixels(point(px(0.), px(20.)));
        assert_eq!(zoom.update(&event), 0);
        event.touch_phase = TouchPhase::Ended;
        assert_eq!(zoom.update(&event), 0);
        event.touch_phase = TouchPhase::Moved;
        assert_eq!(zoom.update(&event), 0);
    }

    #[test]
    fn wheel_respects_platform_lines_and_consumes_bursts_immediately() {
        let mut lines = ScrollLines::default();
        let event = ScrollEvent {
            delta: ScrollDelta::Lines(point(0., 3.)),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        };
        for _ in 0..120 {
            assert_eq!(lines.update(&event, 20.), 3);
        }
        assert_eq!(lines.remainder, 0.);
    }

    #[test]
    fn precise_scroll_preserves_fractions_but_resets_on_reverse_and_end() {
        let mut lines = ScrollLines::default();
        let mut event = ScrollEvent {
            delta: ScrollDelta::Pixels(point(px(0.), px(12.))),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        };
        assert_eq!(lines.update(&event, 20.), 0);
        event.touch_phase = TouchPhase::Moved;
        assert_eq!(lines.update(&event, 20.), 1);
        event.delta = ScrollDelta::Pixels(point(px(0.), px(-20.)));
        assert_eq!(lines.update(&event, 20.), -1);
        event.delta = ScrollDelta::Pixels(point(px(0.), px(12.)));
        assert_eq!(lines.update(&event, 20.), 0);
        event.touch_phase = TouchPhase::Ended;
        assert_eq!(lines.update(&event, 20.), 0);
        event.touch_phase = TouchPhase::Moved;
        assert_eq!(lines.update(&event, 20.), 0);
        event.touch_phase = TouchPhase::Cancelled;
        assert_eq!(lines.update(&event, 20.), 0);
        assert_eq!(lines.remainder, 0.);
    }
}

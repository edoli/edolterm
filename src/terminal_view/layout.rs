/// Vertical cell geometry, in logical pixels. Cell heights and the grid origin
/// land on physical pixels; incomplete rows are never exposed to the PTY.
pub(super) struct ViewportRows {
    pub height: f32,
    pub count: usize,
    pub top: f32,
}

impl ViewportRows {
    pub fn fit(view_height: f32, font_size: f32, line_height: f32, scale: f32) -> Self {
        let scale = scale.max(0.1);
        // The user controls leading as a font-size multiplier. Snap to device
        // pixels so glyphs, hit testing, backgrounds, and cursor share a grid.
        let cell_px = (font_size * line_height * scale).round().max(1.);
        let view_px = (view_height * scale).floor().max(1.);
        let padding_px = (6. * scale).ceil();
        let count = ((view_px - 2. * padding_px) / cell_px)
            .floor()
            .clamp(1., 4096.) as usize;
        Self {
            height: cell_px / scale,
            count,
            // Keep the bottom inset fixed. Put the incomplete row above the
            // grid so resizing cannot accumulate extra space below the prompt.
            top: (view_px - count as f32 * cell_px - padding_px).max(0.) / scale,
        }
    }
}

#[cfg(all(feature = "ui-tests", debug_assertions))]
impl super::TerminalView {
    /// Exercise the rendered grid and hit testing against the same live geometry.
    pub(crate) fn layout_probe(
        &mut self,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> anyhow::Result<serde_json::Value> {
        use alacritty_terminal::{grid::Scroll, vte::ansi::Processor};
        use gpui::{font, point, px};
        let session = self.session.as_ref().unwrap();
        let rows = session.size.rows;
        let bottom = self.grid_top + rows as f32 * self.cell_height;
        let viewport = f32::from(self.bounds.size.height);
        let scale = window.scale_factor();
        let expected_height = (self.settings.font_size * self.settings.line_height * scale)
            .round()
            .max(1.);
        anyhow::ensure!(
            (self.cell_height * scale - expected_height).abs() < 0.001,
            "row height does not match the configured multiplier"
        );
        let bottom_gap = viewport - bottom;
        anyhow::ensure!(
            (bottom_gap - 6.).abs() <= 2. / scale + 0.001,
            "bottom inset is not fixed: {bottom_gap}"
        );
        let last_row_center = self.bounds.origin
            + point(
                px(8. + self.cell_width / 2.),
                px(self.grid_top + (rows as f32 - 0.5) * self.cell_height),
            );
        anyhow::ensure!(
            self.grid_position(last_row_center).1 == rows - 1,
            "last row hit testing is offset"
        );
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font(self.settings.font_family.clone()));
        let font_height = f32::from(text_system.ascent(font_id, px(self.settings.font_size)))
            + f32::from(text_system.descent(font_id, px(self.settings.font_size))).abs();
        let old_height = (self.settings.font_size * 1.45).ceil();
        let metrics = serde_json::json!({
            "font": self.settings.font_family,
            "font_size_dip": self.settings.font_size,
            "line_height_multiplier": self.settings.line_height,
            "scale": scale,
            "viewport_height_dip": viewport,
            "font_height_dip": font_height,
            "row_height_dip": self.cell_height,
            "row_height_device_px": self.cell_height * scale,
            "rows": rows,
            "previous_row_height_dip": old_height,
            "previous_rows": ((viewport - 12.) / old_height).floor() as usize,
            "top_dip": self.grid_top,
            "bottom_gap_dip": viewport - bottom,
            "columns": session.size.cols,
        });
        let mut output = String::from("\x1b[0m\x1b[2J\x1b[H");
        for row in 0..rows {
            let bg = if row == rows - 1 {
                "48;2;45;65;80"
            } else if row % 2 == 0 {
                "48;2;24;24;24"
            } else {
                "49"
            };
            output.push_str(&format!(
                "\x1b[{};1H\x1b[{bg}m\x1b[2K{:02}  Agjpqy | 한글 받침 | 🙂 | 0123456789{}",
                row + 1,
                row + 1,
                if row == rows - 1 { "  LAST ROW" } else { "" }
            ));
        }
        output.push_str("\x1b[0m");
        let mut term = session.terminal.lock();
        let mut parser: Processor = Processor::new();
        parser.advance(&mut *term, output.as_bytes());
        term.scroll_display(Scroll::Bottom);
        self.search_match = None;
        cx.notify();
        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_size_multiplier_is_rounded_in_device_pixels() {
        for (font_size, multiplier, scale, device_height) in [
            (15., 1.2, 1., 18.),
            (15., 1.2, 1.5, 27.),
            (20., 1.5, 1.5, 45.),
            (15.5, 1.2, 1.25, 23.),
            (8., 0.5, 2., 8.),
        ] {
            let layout = ViewportRows::fit(600., font_size, multiplier, scale);
            assert!((layout.height * scale - device_height).abs() < 0.0001);
        }
    }

    #[test]
    fn resizing_keeps_full_rows_with_a_fixed_bottom_inset() {
        for scale in [1., 1.25, 1.5, 2.] {
            let mut previous = 0;
            for height in 100..900 {
                let layout = ViewportRows::fit(height as f32 + 0.25, 15.5, 1.2, scale);
                let bottom = layout.top + layout.count as f32 * layout.height;
                let bottom_gap = height as f32 + 0.25 - bottom;
                assert!(bottom_gap >= 6. - 0.001);
                let expected_gap = (6.0_f32 * scale).ceil() / scale;
                assert!((bottom_gap - expected_gap).abs() <= 1. / scale + 0.001);
                assert!(layout.top >= 6. - 0.001);
                assert!(layout.top < 6. + layout.height + 1. / scale);
                assert!((layout.height * scale - (layout.height * scale).round()).abs() < 0.001);
                assert!(layout.count >= previous);
                previous = layout.count;
            }
        }
    }

    #[test]
    fn tiny_viewports_still_have_a_valid_cell() {
        let layout = ViewportRows::fit(0., 15., 1.2, 1.5);
        assert_eq!(layout.count, 1);
        assert_eq!(layout.top, 0.);
    }
}

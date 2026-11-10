/// Place each ASCII glyph at its column, independent of the font's advances.
/// Keep shaped runs batched; adjusting their positions never scales the glyphs.
pub(super) fn align_ascii_cells(mut line: gpui::ShapedLine, cell_width: f32) -> gpui::ShapedLine {
    debug_assert!(line.text.is_ascii());
    let mut runs = line.runs.clone();
    for run in &mut runs {
        for glyph in &mut run.glyphs {
            glyph.position.x = gpui::px(glyph.index as f32 * cell_width);
        }
    }
    *line = std::sync::Arc::new(gpui::LineLayout {
        font_size: line.font_size,
        width: gpui::px(line.text.len() as f32 * cell_width),
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len,
    });
    line
}

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
            // Anchor the first row to the top inset; any incomplete row stays
            // below the grid. Only shrink the inset in exceptionally tiny views.
            top: padding_px.min((view_px - cell_px).max(0.)) / scale,
        }
    }
}

#[cfg(all(feature = "ui-tests", debug_assertions))]
impl super::TerminalView {
    /// Exercise the rendered grid and hit testing against the same live geometry.
    pub(crate) fn layout_probe(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> anyhow::Result<serde_json::Value> {
        use alacritty_terminal::{grid::Scroll, vte::ansi::Processor};
        use gpui::{EntityInputHandler, font, point, px};
        let session = self.session.as_ref().unwrap();
        let rows = session.size.rows;
        let bottom = self.grid_top + rows as f32 * self.cell_height;
        let viewport = f32::from(self.bounds.size.height);
        let scale = window.scale_factor();
        let expected_width = (self.settings.font_size * self.settings.cell_width).max(1.);
        anyhow::ensure!(
            (self.cell_width - expected_width).abs() < 0.001,
            "cell width does not match the configured multiplier"
        );
        for column in [0, session.size.cols / 2, session.size.cols - 1] {
            let position = self.bounds.origin
                + point(
                    px(8. + (column as f32 + 0.5) * self.cell_width),
                    px(self.grid_top + self.cell_height / 2.),
                );
            anyhow::ensure!(
                self.grid_position(position).0 == column,
                "column hit testing is offset"
            );
        }
        let expected_height = (self.settings.font_size * self.settings.line_height * scale)
            .round()
            .max(1.);
        anyhow::ensure!(
            (self.cell_height * scale - expected_height).abs() < 0.001,
            "row height does not match the configured multiplier"
        );
        let bottom_gap = viewport - bottom;
        let padding = (6. * scale).ceil() / scale;
        anyhow::ensure!(
            (self.grid_top - padding).abs() < 0.001,
            "top inset is not fixed: {}",
            self.grid_top
        );
        anyhow::ensure!(
            bottom_gap >= padding - 0.001 && bottom_gap < padding + self.cell_height + 1. / scale,
            "bottom inset does not contain the remaining space: {bottom_gap}"
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
            "cell_width_multiplier": self.settings.cell_width,
            "cell_width_dip": self.cell_width,
            "cell_width_device_px": self.cell_width * scale,
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
        drop(term);
        self.search_match = None;
        self.replace_and_mark_text_in_range(None, "A한🙂B", None, window, cx);
        let frame = self.prepare(self.bounds, window, cx);
        let mut ascii_glyphs = 0;
        for (_, line) in &frame.glyphs {
            if !line.text.is_ascii() {
                continue;
            }
            anyhow::ensure!(
                (f32::from(line.width) - line.text.len() as f32 * self.cell_width).abs() < 0.001,
                "ASCII decorations do not span the configured cells"
            );
            for run in &line.runs {
                for glyph in &run.glyphs {
                    anyhow::ensure!(
                        (f32::from(glyph.position.x) - glyph.index as f32 * self.cell_width).abs()
                            < 0.001,
                        "ASCII glyphs are not aligned to the configured cells"
                    );
                    ascii_glyphs += 1;
                }
            }
        }
        anyhow::ensure!(ascii_glyphs > 0, "missing rendered ASCII probe");
        anyhow::ensure!(frame.preedit.len() == 4, "composition lost a grapheme");
        for ((position, _), column) in frame.preedit.iter().zip([0., 1., 3., 5.]) {
            anyhow::ensure!(
                (f32::from(position.x - self.cursor_bounds.left()) - column * self.cell_width)
                    .abs()
                    < 0.001,
                "composition is not aligned to the configured cells"
            );
        }
        for (offset, column) in [(0, 0.), (1, 1.), (2, 3.), (4, 5.)] {
            let bounds = self
                .bounds_for_range(offset..offset, self.bounds, window, cx)
                .unwrap();
            anyhow::ensure!(
                (f32::from(bounds.left() - self.cursor_bounds.left()) - column * self.cell_width)
                    .abs()
                    < 0.001,
                "IME candidate anchor does not match the configured cells"
            );
        }
        self.unmark_text(window, cx);
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
    fn resizing_keeps_full_rows_with_a_fixed_top_inset() {
        for scale in [1., 1.25, 1.5, 2.] {
            let mut previous = 0;
            for height in 100..900 {
                let layout = ViewportRows::fit(height as f32 + 0.25, 15.5, 1.2, scale);
                let bottom = layout.top + layout.count as f32 * layout.height;
                let bottom_gap = height as f32 + 0.25 - bottom;
                assert!(bottom_gap >= 6. - 0.001);
                let expected_gap = (6.0_f32 * scale).ceil() / scale;
                assert!(bottom_gap < expected_gap + layout.height + 1. / scale);
                assert!((layout.top - expected_gap).abs() < 0.001);
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

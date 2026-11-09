//! Resolve OSC 8 and plain-text links using terminal cells, including soft wraps.
use alacritty_terminal::{
    Term,
    grid::Dimensions,
    index::{Column, Line, Point},
    term::cell::Flags,
};
use std::ops::RangeInclusive;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Link {
    pub uri: String,
    pub cells: RangeInclusive<Point>,
}

pub(super) fn activation(modifiers: gpui::Modifiers) -> bool {
    modifiers.control && !modifiers.shift && !modifiers.alt && !modifiers.platform
}

fn destination(uri: &str) -> Option<String> {
    if uri.chars().any(char::is_control) {
        return None;
    }
    let url = url::Url::parse(uri).ok()?;
    let supported = match url.scheme() {
        "http" | "https" | "ftp" => url.host_str().is_some(),
        "file" | "mailto" => !url.path().is_empty(),
        _ => false,
    };
    supported.then(|| url.into())
}

pub(super) fn at<T>(term: &Term<T>, mut point: Point) -> Option<Link> {
    let grid = term.grid();
    let cols = term.columns();
    if point.line < grid.topmost_line()
        || point.line > grid.bottommost_line()
        || point.column.0 >= cols
    {
        return None;
    }
    if grid[point].flags.contains(Flags::WIDE_CHAR_SPACER) && point.column.0 > 0 {
        point.column -= 1;
    }
    if grid[point]
        .flags
        .intersects(Flags::HIDDEN | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        return None;
    }
    if let Some(hyperlink) = grid[point].hyperlink() {
        let uri = destination(hyperlink.uri())?;
        let mut start = point;
        let mut end = point;
        // Bound lookup work even for a hyperlink covering the entire scrollback.
        for _ in 0..16_384 {
            let previous = if start.column.0 > 0 {
                Point::new(start.line, start.column - 1)
            } else if start.line > grid.topmost_line() {
                Point::new(start.line - 1, Column(cols - 1))
            } else {
                break;
            };
            if grid[previous].hyperlink().as_ref() != Some(&hyperlink) {
                break;
            }
            start = previous;
        }
        for _ in 0..16_384 {
            let next = if end.column.0 + 1 < cols {
                Point::new(end.line, end.column + 1)
            } else if end.line < grid.bottommost_line() {
                Point::new(end.line + 1, Column(0))
            } else {
                break;
            };
            if grid[next].hyperlink().as_ref() != Some(&hyperlink) {
                break;
            }
            end = next;
        }
        return Some(Link {
            uri,
            cells: start..=end,
        });
    }

    // Only join soft-wrapped rows: a hard newline must never join two URLs.
    let mut first = point.line;
    let mut last = point.line;
    let max_rows = (16_384 / cols).max(1) as i32;
    while first > grid.topmost_line()
        && grid[Line(first.0 - 1)][Column(cols - 1)]
            .flags
            .contains(Flags::WRAPLINE)
    {
        first -= 1;
        if point.line.0 - first.0 >= max_rows {
            return None;
        }
    }
    while last < grid.bottommost_line()
        && grid[last][Column(cols - 1)].flags.contains(Flags::WRAPLINE)
    {
        last += 1;
        if last.0 - first.0 >= max_rows {
            return None;
        }
    }
    let mut text = String::new();
    let mut positions = Vec::new();
    let mut hit = None;
    for row in first.0..=last.0 {
        for col in 0..cols {
            let p = Point::new(Line(row), Column(col));
            let cell = &grid[p];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            if p == point {
                hit = Some(text.len());
            }
            positions.push((text.len(), p));
            text.push(if cell.flags.contains(Flags::HIDDEN) {
                ' '
            } else {
                cell.c
            });
            if let Some(chars) = cell.zerowidth() {
                text.extend(chars);
            }
        }
    }
    let hit = hit?;
    for (start, _) in text.char_indices() {
        if start > hit {
            break;
        }
        let tail = &text[start..];
        if !["https://", "http://", "ftp://", "file://", "mailto:"]
            .iter()
            .any(|prefix| {
                tail.get(..prefix.len())
                    .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
            })
        {
            continue;
        }
        let length = tail
            .find(|c: char| {
                c.is_whitespace()
                    || c.is_control()
                    || matches!(c, '<' | '>' | '"' | '\'' | '`' | '“' | '”' | '。' | '，')
            })
            .unwrap_or(tail.len());
        let candidate = trim_punctuation(&tail[..length]);
        let end = start + candidate.len();
        if hit >= end {
            continue;
        }
        let uri = destination(candidate)?;
        let first = positions.iter().find(|(byte, _)| *byte == start)?.1;
        let mut last = positions.iter().rev().find(|(byte, _)| *byte < end)?.1;
        if grid[last].flags.contains(Flags::WIDE_CHAR) {
            last.column += 1;
        }
        return Some(Link {
            uri,
            cells: first..=last,
        });
    }
    None
}

fn trim_punctuation(mut text: &str) -> &str {
    loop {
        let Some(last) = text.chars().next_back() else {
            return text;
        };
        let trim = match last {
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            ')' | ']' | '}' => {
                let open = match last {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                text.chars().filter(|c| *c == last).count()
                    > text.chars().filter(|c| *c == open).count()
            }
            _ => false,
        };
        if !trim {
            return text;
        }
        text = &text[..text.len() - last.len_utf8()];
    }
}

impl super::TerminalView {
    pub(super) fn link_at(&self, position: gpui::Point<gpui::Pixels>) -> Option<Link> {
        let session = self.session.as_ref()?;
        let x = f32::from(position.x - self.bounds.left()) - 8.;
        let y = f32::from(position.y - self.bounds.top()) - self.grid_top;
        // Do not clamp padding or the scrollbar onto the nearest link cell.
        if x < 0.
            || y < 0.
            || x >= session.size.cols as f32 * self.cell_width
            || y >= session.size.rows as f32 * self.cell_height
        {
            return None;
        }
        let term = session.terminal.lock();
        at(
            &term,
            Point::new(
                Line((y / self.cell_height) as i32 - term.grid().display_offset() as i32),
                Column((x / self.cell_width) as usize),
            ),
        )
    }
}

#[cfg(all(feature = "ui-tests", debug_assertions))]
#[derive(Default)]
pub(super) struct Probe {
    pub opened: Vec<String>,
    pub mouse_reports: Vec<Vec<u8>>,
}

#[cfg(all(feature = "ui-tests", debug_assertions))]
impl super::TerminalView {
    pub(crate) fn links_probe(
        terminal: &gpui::Entity<Self>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> anyhow::Result<serde_json::Value> {
        use alacritty_terminal::vte::ansi::Processor;
        use gpui::*;
        terminal.update(cx, |view, cx| {
            view.link_probe = Some(Probe::default());
            let mut term = view.session.as_ref().unwrap().terminal.lock();
            let mut parser: Processor = Processor::new();
            parser.advance(&mut *term, b"\x1bc\x1b[2J\x1b[H\x1b]8;;https://example.com/osc8\x1b\\Edolterm link\x1b]8;;\x1b\\\r\nhttps://example.com/plain\r\n\x1b]8;;javascript:alert(1)\x1b\\Unsupported link\x1b]8;;\x1b\\\r\nOrdinary text\x1b[?1003h\x1b[?1006h");
            cx.notify();
        });
        window.refresh();
        window.draw(cx).clear(cx);
        let position = |row: usize, cx: &App| {
            terminal.read(cx).bounds.origin
                + point(
                    px(8. + terminal.read(cx).cell_width * 2.5),
                    px(terminal.read(cx).grid_top
                        + terminal.read(cx).cell_height * (row as f32 + 0.5)),
                )
        };
        let control = Modifiers {
            control: true,
            ..Default::default()
        };
        let click = |row, modifiers, window: &mut Window, cx: &mut App| {
            let p = position(row, cx);
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: p,
                    modifiers,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: p,
                    modifiers,
                    pressed_button: Some(MouseButton::Left),
                }),
                cx,
            );
            // Releasing Ctrl before the mouse must not leak a release to the CLI.
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: p,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        click(0, control, window, cx);
        click(1, control, window, cx);
        terminal.read_with(cx, |view, _| -> anyhow::Result<()> {
            let probe = view.link_probe.as_ref().unwrap();
            anyhow::ensure!(
                probe.opened == ["https://example.com/osc8", "https://example.com/plain"],
                "link opening dispatch: {:?}",
                probe.opened
            );
            anyhow::ensure!(
                probe.mouse_reports.is_empty(),
                "Ctrl-click leaked mouse events to CLI"
            );
            Ok(())
        })?;
        click(0, Modifiers::default(), window, cx); // CLI still owns ordinary clicks.
        click(2, control, window, cx); // Unsupported OSC 8 target does not launch.
        click(3, control, window, cx); // A Ctrl-click on non-link text is preserved.
        let reports = terminal.read_with(cx, |view, _| -> anyhow::Result<usize> {
            let probe = view.link_probe.as_ref().unwrap();
            anyhow::ensure!(
                probe.opened.len() == 2,
                "ordinary/unsupported click opened a URL"
            );
            anyhow::ensure!(
                probe.mouse_reports.len() == 9,
                "normal CLI press/release was lost: {:?}",
                probe.mouse_reports
            );
            anyhow::ensure!(
                view.link_at(view.bounds.origin).is_none(),
                "padding opened a link"
            );
            Ok(probe.mouse_reports.len())
        })?;
        // Shift continues to force terminal selection even in mouse-reporting mode.
        click(
            0,
            Modifiers {
                shift: true,
                ..control
            },
            window,
            cx,
        );
        terminal.read_with(cx, |view, _| -> anyhow::Result<()> {
            anyhow::ensure!(
                view.session
                    .as_ref()
                    .unwrap()
                    .terminal
                    .lock()
                    .selection
                    .is_some(),
                "Shift selection stopped working"
            );
            anyhow::ensure!(
                view.link_probe.as_ref().unwrap().opened.len() == 2,
                "Shift selection opened link"
            );
            Ok(())
        })?;
        // Also exercise the ordinary shell case without mouse-reporting mode.
        terminal.update(cx, |view, _| {
            let mut term = view.session.as_ref().unwrap().terminal.lock();
            let mut parser: Processor = Processor::new();
            parser.advance(&mut *term, b"\x1b[?1003l\x1b[?1006l");
        });
        click(0, control, window, cx);
        let result = terminal.update(cx, |view, cx| -> anyhow::Result<_> {
            let probe = view.link_probe.take().unwrap();
            anyhow::ensure!(probe.opened.len() == 3, "OSC 8 did not open without CLI mouse mode");
            anyhow::ensure!(probe.mouse_reports.len() == reports, "selection/link click leaked input");
            cx.notify();
            Ok(serde_json::json!({ "opened": probe.opened, "normal_cli_mouse_reports": reports, "duplicate_mouse_reports": 0, "shift_selection": true }))
        })?;
        terminal.update(cx, |view, cx| {
            view.session.as_ref().unwrap().terminal.lock().selection = None;
            cx.notify();
        });
        window.dispatch_event(
            PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                modifiers: control,
                capslock: Capslock { on: false },
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: position(0, cx),
                modifiers: control,
                pressed_button: None,
            }),
            cx,
        );
        window.refresh();
        window.draw(cx).clear(cx);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::GridSize;
    use alacritty_terminal::{event::VoidListener, vte::ansi::Processor};

    fn terminal(cols: usize, rows: usize, text: &str) -> Term<VoidListener> {
        let size = GridSize {
            cols,
            rows,
            ..Default::default()
        };
        let mut term = Term::new(Default::default(), &size, VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, text.as_bytes());
        term
    }

    #[test]
    fn osc8_opens_destination_instead_of_label_including_wide_cells() {
        let term = terminal(
            60,
            4,
            "\x1b]8;;https://example.com/real\x1b\\한글 label\x1b]8;;\x1b\\ https://example.com/visible",
        );
        for col in 0..10 {
            let link = at(&term, Point::new(Line(0), Column(col))).unwrap();
            assert_eq!(link.uri, "https://example.com/real");
        }
        assert!(at(&term, Point::new(Line(0), Column(10))).is_none());
        assert_eq!(
            at(&term, Point::new(Line(0), Column(15))).unwrap().uri,
            "https://example.com/visible"
        );
    }

    #[test]
    fn plain_urls_keep_balanced_parentheses_and_exclude_surrounding_punctuation() {
        let term = terminal(
            80,
            4,
            "See (https://example.com/a_(b)). https://second.test/?x=1&y=2",
        );
        let link = at(&term, Point::new(Line(0), Column(8))).unwrap();
        assert_eq!(link.uri, "https://example.com/a_(b)");
        assert_eq!(
            link.cells,
            Point::new(Line(0), Column(5))..=Point::new(Line(0), Column(29))
        );
        assert!(at(&term, Point::new(Line(0), Column(30))).is_none());
        assert_eq!(
            at(&term, Point::new(Line(0), Column(38))).unwrap().uri,
            "https://second.test/?x=1&y=2"
        );
    }

    #[test]
    fn soft_wrapped_urls_are_clickable_on_every_row() {
        let uri = "https://example.com/long/path?x=1";
        let term = terminal(12, 6, &format!("URL {uri} done"));
        for index in 4..4 + uri.len() {
            let point = Point::new(Line((index / 12) as i32), Column(index % 12));
            assert_eq!(at(&term, point).unwrap().uri, uri, "{point:?}");
        }
        assert!(at(&term, Point::new(Line(0), Column(2))).is_none());
    }

    #[test]
    fn unicode_wide_and_combining_characters_keep_cell_coordinates() {
        let term = terminal(80, 4, "한글 e\u{301} https://example.com/한글");
        for col in 7..31 {
            let link = at(&term, Point::new(Line(0), Column(col))).unwrap();
            assert_eq!(link.uri, "https://example.com/%ED%95%9C%EA%B8%80");
        }
        assert!(at(&term, Point::new(Line(0), Column(6))).is_none());
    }

    #[test]
    fn hard_newlines_and_scrollback_do_not_change_url_targets() {
        let term = terminal(
            40,
            3,
            "https://example.com/\r\nseparate-text\r\nline3\r\nline4\r\nline5",
        );
        assert!(term.grid().topmost_line() < Line(0));
        let link = at(&term, Point::new(Line(-2), Column(5))).unwrap();
        assert_eq!(link.uri, "https://example.com/");
        assert!(at(&term, Point::new(Line(-1), Column(5))).is_none());
    }

    #[test]
    fn only_supported_uri_schemes_are_opened() {
        for uri in [
            "https://example.com",
            "HTTP://example.com",
            "file:///C:/Users/test/a%20b.txt",
            "mailto:user@example.com",
            "ftp://example.com/a",
        ] {
            assert!(destination(uri).is_some(), "{uri}");
        }
        for uri in [
            "javascript:alert(1)",
            "data:text/html,test",
            "cmd.exe",
            "https://example.com/\ncmd",
            "mailto:",
            "https://",
        ] {
            assert!(destination(uri).is_none(), "{uri}");
        }
        let term = terminal(
            60,
            4,
            "\x1b]8;;javascript:alert(1)\x1b\\https://example.com\x1b]8;;\x1b\\",
        );
        assert!(at(&term, Point::new(Line(0), Column(5))).is_none());
    }
}

//! Mouse and rendering regression checks, compiled only into UI test builds.
use super::*;
use alacritty_terminal::vte::ansi::Processor;
use anyhow::{Result, ensure};

fn clipboard_text(cx: &App) -> Option<String> {
    // Windows clipboard listeners can briefly hold the clipboard after a write.
    for _ in 0..50 {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            return Some(text);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn set_clipboard_text(cx: &App, text: &str) -> Result<()> {
    cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
    // Pace fixture writes like user input, so clipboard-history listeners finish
    // before the next simulated copy/paste opens the native clipboard again.
    std::thread::sleep(Duration::from_millis(150));
    ensure!(
        clipboard_text(cx).as_deref() == Some(text),
        "clipboard fixture write failed"
    );
    Ok(())
}

impl TerminalView {
    pub(crate) fn selection_probe(
        terminal: &Entity<Self>,
        directory: &std::path::Path,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<()> {
        window.refresh();
        window.draw(cx).clear(cx);
        terminal.update(cx, |view, cx| {
            view.search_match = Some(TermPoint::new(Line(0), Column(0))..=TermPoint::new(Line(0), Column(5)));
            let mut term = view.session.as_ref().unwrap().terminal.lock();
            let mut parser: Processor = Processor::new();
            parser.advance(&mut *term, "\x1b[?1003l\x1b[?1006l\x1b[0m\x1b[2J\x1b[H\x1b[31mCOPY\x1b[2;7m_한글\x1b[0m\r\nUnselected text".as_bytes());
            term.scroll_display(Scroll::Bottom);
            cx.notify();
        });
        window.refresh();
        window.draw(cx).clear(cx);
        let position = |column: f32, cx: &App| {
            let view = terminal.read(cx);
            view.bounds.origin
                + point(
                    px(8. + view.cell_width * column),
                    px(view.grid_top + view.cell_height * 0.5),
                )
        };
        let start = position(0.1, cx);
        let end = position(8.9, cx);
        // Real dispatch exercises hit testing, drag selection and right-click handling.
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        ensure!(
            terminal
                .read(cx)
                .session
                .as_ref()
                .unwrap()
                .terminal
                .lock()
                .selection_to_string()
                .as_deref()
                == Some("COPY_한글"),
            "drag selection lost Unicode text"
        );
        let original = terminal.read(cx).settings.color_scheme.clone();
        for scheme in ["Campbell", "One Half Light", "VSCode Dark Modern"] {
            terminal.update(cx, |view, cx| -> Result<()> {
                view.settings.color_scheme = scheme.into();
                let frame = view.prepare(view.bounds, window, cx);
                let color = Theme::from_settings(&view.settings, cx)
                    .terminal
                    .selection
                    .bg;
                ensure!(
                    frame
                        .quads
                        .iter()
                        .filter(|q| q.background == solid_background(color))
                        .count()
                        >= 9,
                    "{scheme}: selection background missing over ANSI/search cells"
                );
                cx.notify();
                Ok(())
            })?;
            window.refresh();
            window.draw(cx).clear(cx);
            window
                .render_to_image()?
                .save(directory.join(format!("selection-{scheme}.png")))?;
        }
        terminal.update(cx, |view, cx| {
            view.settings.color_scheme = original;
            view.search_match = None;
            cx.notify();
        });
        // A copy must replace stale clipboard data and dismiss the highlight.
        set_clipboard_text(cx, "STALE_CLIPBOARD")?;
        let right_click = |modifiers, window: &mut Window, cx: &mut App| {
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Right,
                    position: end,
                    modifiers,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Right,
                    position: end,
                    modifiers,
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
        };
        right_click(Modifiers::default(), window, cx);
        ensure!(
            clipboard_text(cx).as_deref() == Some("COPY_한글"),
            "right-click did not copy selection"
        );
        ensure!(
            terminal
                .read(cx)
                .session
                .as_ref()
                .unwrap()
                .terminal
                .lock()
                .selection
                .is_none(),
            "right-click did not clear selection"
        );

        // Mouse-reporting applications keep ordinary clicks; Shift overrides them.
        terminal.update(cx, |view, _| {
            view.link_probe = Some(links::Probe::default());
            let mut term = view.session.as_ref().unwrap().terminal.lock();
            let mut parser: Processor = Processor::new();
            parser.advance(&mut *term, b"\x1b[?1003h\x1b[?1006h");
            let mut selection = Selection::new(
                SelectionType::Simple,
                TermPoint::new(Line(0), Column(0)),
                Side::Left,
            );
            selection.update(TermPoint::new(Line(0), Column(8)), Side::Right);
            term.selection = Some(selection);
        });
        set_clipboard_text(cx, "CLI_CLIPBOARD")?;
        right_click(Modifiers::default(), window, cx);
        ensure!(
            clipboard_text(cx).as_deref() == Some("CLI_CLIPBOARD"),
            "CLI right-click was intercepted"
        );
        right_click(
            Modifiers {
                shift: true,
                ..Default::default()
            },
            window,
            cx,
        );
        let copied = clipboard_text(cx);
        ensure!(
            copied.as_deref() == Some("COPY_한글"),
            "Shift+right-click did not copy in CLI mouse mode: {copied:?}"
        );
        terminal.update(cx, |view, _| -> Result<()> {
            let probe = view.link_probe.take().unwrap();
            ensure!(
                probe.mouse_reports.len() == 2,
                "CLI mouse press/release was lost or Shift copy leaked input"
            );
            let mut parser: Processor = Processor::new();
            parser.advance(
                &mut *view.session.as_ref().unwrap().terminal.lock(),
                b"\x1b[?1003l\x1b[?1006l",
            );
            Ok(())
        })?;
        // An empty single-click selection must fall through to paste.
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        set_clipboard_text(cx, "[Console]::WriteLine(('RIGHT_CLICK_'+'PASTE_한글_OK'))")?;
        right_click(Modifiers::default(), window, cx);
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        Ok(())
    }
}

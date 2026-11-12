use super::*;
use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point},
    vte::ansi::Processor,
};
use anyhow::{Result, ensure};

fn press(name: &'static str, window: &mut Window, cx: &mut App) -> Result<()> {
    let key = Keystroke::parse(name)?;
    window.dispatch_keystroke(key.clone(), cx);
    window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke: key }), cx);
    Ok(())
}

pub(super) fn run(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> Result<()> {
    // Rebuild the input dispatch tree after returning from the settings tab.
    window.refresh();
    window.draw(cx).clear(cx);
    let clipboard = cx.read_from_clipboard();
    let result = exercise(root, window, cx);
    cx.write_to_clipboard(clipboard.unwrap_or_else(|| ClipboardItem::new_string(String::new())));
    result
}

fn exercise(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> Result<()> {
    let initial = root.read(cx).active;
    let terminal = root.read(cx).active_terminal().unwrap();
    press("ctrl-alt-9", window, cx)?;
    ensure!(
        root.read(cx).active == root.read(cx).tabs.len() - 1,
        "Ctrl+Alt+9 must select the last tab"
    );
    press("ctrl-alt-1", window, cx)?;
    ensure!(root.read(cx).active == 0, "first tab shortcut");
    root.update(cx, |app, cx| app.activate(initial, window, cx));
    press("ctrl-shift-space", window, cx)?;
    ensure!(
        root.read(cx).menu,
        "Windows Terminal profile dropdown shortcut"
    );
    press("escape", window, cx)?;
    ensure!(!root.read(cx).menu, "close dropdown");

    let original_size = terminal.read(cx).settings.font_size;
    press("ctrl-+", window, cx)?;
    ensure!(
        terminal.read(cx).settings.font_size == original_size + 1.,
        "increase font"
    );
    press("ctrl--", window, cx)?;
    ensure!(
        terminal.read(cx).settings.font_size == original_size,
        "decrease font"
    );
    press("ctrl-+", window, cx)?;
    press("ctrl-0", window, cx)?;
    ensure!(
        terminal.read(cx).settings.font_size == original_size,
        "reset font"
    );

    terminal.update(cx, |view, cx| {
        let mut term = view.session.as_ref().unwrap().terminal.lock();
        let mut parser: Processor = Processor::new();
        for i in 0..100 {
            parser.advance(&mut *term, format!("\r\nSCROLL_{i:03}").as_bytes());
        }
        term.scroll_display(Scroll::Bottom);
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        selection.update(Point::new(Line(0), Column(5)), Side::Right);
        term.selection = Some(selection);
        cx.notify();
    });
    let unicode = format!("SELECT_{}", "한글🙂 abcdef ".repeat(20));
    terminal.update(cx, |view, _| {
        let mut term = view.session.as_ref().unwrap().terminal.lock();
        let mut parser: Processor = Processor::new();
        parser.advance(&mut *term, format!("\r\n{unicode}").as_bytes());
    });
    press("ctrl-shift-a", window, cx)?;
    let selected = terminal
        .read(cx)
        .session
        .as_ref()
        .unwrap()
        .terminal
        .lock()
        .selection_to_string()
        .unwrap_or_default();
    ensure!(
        selected.contains("SMOKE_한글_OK")
            && selected.contains("SCROLL_000")
            && selected.contains("SCROLL_099"),
        "select all omitted scrollback"
    );
    ensure!(
        selected.ends_with(unicode.trim_end()),
        "select all damaged wrapped Unicode or included unused rows: {selected:?}"
    );
    terminal.update(cx, |view, _| {
        let mut parser: Processor = Processor::new();
        parser.advance(
            &mut *view.session.as_ref().unwrap().terminal.lock(),
            b"\x1b[?1049hALT_ONLY",
        );
    });
    press("ctrl-shift-a", window, cx)?;
    let selected = terminal
        .read(cx)
        .session
        .as_ref()
        .unwrap()
        .terminal
        .lock()
        .selection_to_string()
        .unwrap_or_default();
    ensure!(
        selected.contains("ALT_ONLY") && !selected.contains("SCROLL_000"),
        "alternate screen selection included the main buffer"
    );
    terminal.update(cx, |view, _| {
        let mut parser: Processor = Processor::new();
        let mut term = view.session.as_ref().unwrap().terminal.lock();
        parser.advance(&mut *term, b"\x1b[?1049l");
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        selection.update(Point::new(Line(0), Column(5)), Side::Right);
        term.selection = Some(selection);
    });
    press("enter", window, cx)?;
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
        "Enter must copy and clear the selection"
    );
    let offset = |cx: &App| {
        terminal
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .terminal
            .lock()
            .grid()
            .display_offset()
    };
    press("ctrl-shift-up", window, cx)?;
    let lines =
        platform::keyboard_scroll_lines(terminal.read(cx).session.as_ref().unwrap().size.rows);
    ensure!(
        offset(cx) == lines as usize,
        "scroll up using the system wheel-lines preference"
    );
    press("ctrl-shift-pageup", window, cx)?;
    ensure!(offset(cx) > 1, "scroll up one page");
    press("ctrl-shift-home", window, cx)?;
    ensure!(
        offset(cx)
            == terminal
                .read(cx)
                .session
                .as_ref()
                .unwrap()
                .terminal
                .lock()
                .history_size(),
        "scroll to top"
    );
    press("ctrl-shift-end", window, cx)?;
    ensure!(offset(cx) == 0, "scroll to bottom");

    let count = root.read(cx).tabs.len();
    // Windows reports Shift+1 as '!' with Shift consumed by its layout mapper.
    press("ctrl-!", window, cx)?;
    ensure!(
        root.read(cx).tabs.len() == count + 1,
        "physical Shift+number profile shortcut"
    );
    root.update(cx, |app, cx| {
        let id = app.tabs[app.active].id;
        app.close_tab(id, window, cx);
        app.activate(initial, window, cx);
    });

    press("ctrl-,", window, cx)?;
    root.update(cx, |app, cx| {
        app.settings_page = super::super::preferences::SettingsPage::Shortcuts;
        app.start_shortcut_capture("newTab", Some(0), window, cx);
    });
    press("ctrl-shift-n", window, cx)?;
    ensure!(
        root.read(cx).shortcut_capture.is_some(),
        "conflicting chord should remain in capture"
    );
    ensure!(
        root.read(cx).draft.keybindings.is_empty(),
        "conflict changed the draft"
    );
    press("escape", window, cx)?;
    ensure!(
        root.read(cx).shortcut_capture.is_none(),
        "Escape cancels capture"
    );
    root.update(cx, |app, cx| {
        app.start_shortcut_capture("newTab", Some(0), window, cx)
    });
    press("ctrl-y", window, cx)?;
    ensure!(root.read(cx).shortcut_capture.is_none(), "captured chord");
    ensure!(
        root.read(cx).settings.keybindings.is_empty(),
        "draft applied before save"
    );
    root.update(cx, |app, cx| app.save_settings(cx));
    ensure!(
        Settings::load(&root.read(cx).dir)?
            .keybindings
            .get("newTab")
            == Some(&vec!["ctrl-y".into()]),
        "saved binding round trip"
    );
    let count = root.read(cx).tabs.len();
    press("ctrl-y", window, cx)?;
    ensure!(
        root.read(cx).tabs.len() == count + 1,
        "saved shortcut did not execute"
    );
    root.update(cx, |app, cx| {
        let id = app.tabs[app.active].id;
        app.close_tab(id, window, cx);
        app.set_shortcut_keys("newTab", Some(vec![]), cx);
        app.save_settings(cx);
        ensure!(
            app.shortcut_bindings
                .action(
                    &Keystroke::parse("ctrl-shift-t").unwrap(),
                    cx.keyboard_mapper().as_ref()
                )
                .is_none(),
            "disabled default still runs"
        );
        app.set_shortcut_keys("newTab", None, cx);
        app.save_settings(cx);
        app.reload_settings(cx);
        ensure!(
            app.settings.keybindings.is_empty(),
            "reset defaults did not persist"
        );
        app.activate(initial, window, cx);
        Ok(())
    })?;
    Ok(())
}

#[cfg(windows)]
pub(super) async fn duplicate_tab(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (original, original_id, profile, cwd) = root.read_with(cx, |app, cx| {
        let view = app.active_terminal().unwrap();
        let terminal = view.read(cx);
        (
            view.clone(),
            app.tabs[app.active].id,
            terminal.profile.clone(),
            terminal.session.as_ref().unwrap().working_directory(),
        )
    });
    let duplicate = handle.update(cx, |_, window, cx| -> Result<_> {
        let count = root.read(cx).tabs.len();
        press("ctrl-shift-d", window, cx)?;
        ensure!(
            root.read(cx).tabs.len() == count + 1,
            "duplicate did not open a tab"
        );
        let duplicate = root.read(cx).active_terminal().unwrap();
        ensure!(
            duplicate != original,
            "duplicate reused the original terminal"
        );
        ensure!(
            duplicate.read(cx).profile == profile,
            "duplicate changed profile"
        );
        ensure!(
            duplicate.read(cx).working_directory() == cwd,
            "duplicate changed directory"
        );
        // Submit before the background startup can attach its result to the UI.
        // This verifies that committed input survives the pending-session state.
        ensure!(
            duplicate.read(cx).session.is_none(),
            "duplicate started synchronously"
        );
        duplicate.update(cx, |view, cx| {
            view.replace_text_in_range(
                None,
                "[Console]::WriteLine(('DUP_CWD_' + $PWD.ProviderPath))\r",
                window,
                cx,
            );
        });
        Ok(duplicate)
    })??;
    let expected = format!("DUP_CWD_{}", cwd.display());
    let started = Instant::now();
    loop {
        let text = duplicate.read_with(cx, |view, _| {
            view.session
                .as_ref()
                .map(|s| s.plain_text())
                .unwrap_or_default()
        });
        if text.contains(&expected) {
            break;
        }
        ensure!(
            started.elapsed() < Duration::from_secs(12),
            "duplicate shell did not start in the source directory: {text}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(25))
            .await;
    }
    handle.update(cx, |_, window, cx| {
        root.update(cx, |app, cx| {
            let id = app.tabs[app.active].id;
            app.close_tab(id, window, cx);
            let index = app.tabs.iter().position(|t| t.id == original_id).unwrap();
            app.activate(index, window, cx);
        });
    })?;
    Ok(())
}

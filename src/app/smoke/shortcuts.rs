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
        app.settings_page = super::super::shortcuts::SettingsPage::Shortcuts;
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

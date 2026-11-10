//! Exercise real input dispatch, persistence, and the font discovery boundary.
use super::*;
use crate::select::Select;
use anyhow::{Result, ensure};

fn draw(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn press(key: &'static str, window: &mut Window, cx: &mut App) -> Result<()> {
    window.dispatch_keystroke(Keystroke::parse(key)?, cx);
    draw(window, cx);
    Ok(())
}

fn click(position: Point<Pixels>, window: &mut Window, cx: &mut App) {
    for input in [
        PlatformInput::MouseDown(MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
        PlatformInput::MouseUp(MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    ] {
        window.dispatch_event(input, cx);
        draw(window, cx);
    }
}

fn set_number(
    input: &Entity<super::super::preferences::NumberInput>,
    value: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let field = input.read(cx).input.clone();
    field.update(cx, |field, cx| {
        field.set_text(value, window, cx);
        field.focus.focus(window, cx);
    });
}

pub(super) async fn run(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (original, active) = root.read_with(cx, |app, _| (app.settings.clone(), app.active));
    handle.update(cx, |_, window, cx| -> Result<()> {
        root.update(cx, |app, cx| {
            app.hide_search(window, cx);
            app.settings_tab(window, cx);
        });
        draw(window, cx);
        ensure!(
            !Select::font_discovery_started(cx),
            "opening settings enumerated fonts"
        );
        let (profile, behavior, theme) = {
            let prefs = root.read(cx).preferences.as_ref().unwrap();
            (
                prefs.profile.clone(),
                prefs.behavior.clone(),
                prefs.theme.clone(),
            )
        };
        // Click the actual dropdown trigger, including a second click to close it.
        let position = profile.read(cx).trigger_bounds().center();
        click(position, window, cx);
        ensure!(profile.read(cx).open, "profile dropdown did not open");
        window
            .render_to_image()?
            .save(root.read(cx).dir.join("profile-dropdown.png"))?;
        click(position, window, cx);
        ensure!(
            !profile.read(cx).open,
            "trigger click reopened its dropdown"
        );
        for select in [&behavior, &theme] {
            select.update(cx, |select, cx| select.toggle(window, cx));
            draw(window, cx);
            press("down", window, cx)?;
            press("enter", window, cx)?;
            ensure!(!select.read(cx).open, "Enter did not confirm selection");
        }
        ensure!(
            !Select::font_discovery_started(cx),
            "non-font dropdown enumerated fonts"
        );
        Ok(())
    })??;
    handle.update(cx, |_, window, cx| -> Result<()> {
        let (font, size, history) = {
            let prefs = root.read(cx).preferences.as_ref().unwrap();
            (
                prefs.font.clone(),
                prefs.size.clone(),
                prefs.history.clone(),
            )
        };
        set_number(&size, "17.5", window, cx);
        press("up", window, cx)?;
        ensure!(
            size.read(cx).value(cx) == Some(18.5),
            "font size keyboard step"
        );
        set_number(&history, "12345", window, cx);
        root.update(cx, |app, cx| app.save_settings(cx));
        let saved = Settings::load(&root.read(cx).dir)?;
        ensure!(
            saved.font_size == 18.5 && saved.scrollback == 12345,
            "numeric settings did not persist"
        );
        ensure!(
            saved.theme == root.read(cx).draft.theme,
            "theme selection not saved"
        );
        ensure!(
            saved.windowing_behavior == root.read(cx).draft.windowing_behavior,
            "window behavior selection not saved"
        );
        for (input, invalid) in [
            (&size, "NaN"),
            (&size, "49"),
            (&history, "1000001"),
            (&history, "1.5"),
            (&history, "-1"),
        ] {
            let previous = input.read(cx).input.read(cx).text.clone();
            set_number(input, invalid, window, cx);
            root.update(cx, |app, cx| app.save_settings(cx));
            ensure!(
                Settings::load(&root.read(cx).dir)? == saved,
                "invalid input overwrote saved settings"
            );
            set_number(input, &previous, window, cx);
        }
        root.update(cx, |app, _| app.status = None);
        // Only this explicit font action may start discovery. Type before its
        // asynchronous completion to ensure the query survives the result.
        let position = font.read(cx).trigger_bounds().center();
        click(position, window, cx);
        ensure!(
            font.read(cx).open && Select::font_discovery_started(cx),
            "font discovery did not start on demand"
        );
        let search = font.read(cx).search.clone();
        search.update(cx, |search, cx| search.set_text("CoNsOlAs", window, cx));
        Ok(())
    })??;
    let started = Instant::now();
    while root.read_with(cx, |_, cx| Select::font_count(cx)) == 0 {
        ensure!(
            started.elapsed() < Duration::from_secs(10),
            "font discovery timed out"
        );
        cx.background_executor()
            .timer(Duration::from_millis(20))
            .await;
    }
    handle.update(cx, |_, window, cx| -> Result<()> {
        draw(window, cx);
        let font = root.read(cx).preferences.as_ref().unwrap().font.clone();
        ensure!(
            font.read(cx).visible_values() == vec!["Consolas"],
            "case insensitive font search or pending query failed"
        );
        window
            .render_to_image()?
            .save(root.read(cx).dir.join("font-search.png"))?;
        press("enter", window, cx)?;
        Ok(())
    })??;
    handle.update(cx, |_, window, cx| -> Result<()> {
        let font = root.read(cx).preferences.as_ref().unwrap().font.clone();
        ensure!(
            root.read(cx).draft.font_family == "Consolas",
            "font selection did not update draft"
        );
        root.update(cx, |app, cx| app.save_settings(cx));
        ensure!(
            Settings::load(&root.read(cx).dir)?.font_family == "Consolas",
            "font selection did not persist"
        );
        font.update(cx, |font, cx| font.toggle(window, cx));
        draw(window, cx);
        ensure!(
            font.read(cx).visible_values().len() > 1,
            "reopening did not clear the search"
        );
        window
            .render_to_image()?
            .save(root.read(cx).dir.join("font-dropdown.png"))?;
        press("escape", window, cx)?;
        ensure!(!font.read(cx).open, "Escape did not close font popup");
        let count = Select::font_count(cx);
        root.update(cx, |app, cx| {
            app.draft = original;
            app.preferences = None;
            app.save_settings(cx);
            app.ensure_preferences(window, cx);
        });
        draw(window, cx);
        let font = root.read(cx).preferences.as_ref().unwrap().font.clone();
        font.update(cx, |font, cx| font.toggle(window, cx));
        ensure!(
            font.read(cx).visible_values().len() >= count,
            "new selector failed to reuse the process font cache"
        );
        draw(window, cx);
        click(point(px(200.), px(80.)), window, cx);
        ensure!(!font.read(cx).open, "outside click did not dismiss popup");
        root.read(cx)
            .settings_scroll
            .set_offset(point(px(0.), -px(550.)));
        draw(window, cx);
        window
            .render_to_image()?
            .save(root.read(cx).dir.join("settings-bottom.png"))?;
        root.read(cx)
            .settings_scroll
            .set_offset(point(px(0.), px(0.)));
        root.update(cx, |app, cx| {
            app.activate(active, window, cx);
            app.status = None;
        });
        Ok(())
    })??;
    Ok(())
}

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

fn show_page(root: &Entity<TerminalApp>, page: SettingsPage, window: &mut Window, cx: &mut App) {
    root.update(cx, |app, cx| {
        app.settings_page = page;
        app.settings_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    });
    draw(window, cx);
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
    color_schemes(root, handle, cx).await?;
    handle.update(cx, |_, window, cx| -> Result<()> {
        root.update(cx, |app, cx| {
            app.hide_search(window, cx);
            app.settings_tab(window, cx);
        });
        draw(window, cx);
        pages_and_actions(root, window, cx)?;
        scrollbars(root, window, cx)?;
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
        show_page(root, SettingsPage::Behavior, window, cx);
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
        for (page, select) in [
            (SettingsPage::Behavior, &behavior),
            (SettingsPage::Appearance, &theme),
        ] {
            show_page(root, page, window, cx);
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
        let (font, size, line_height, cell_width, history) = {
            let prefs = root.read(cx).preferences.as_ref().unwrap();
            (
                prefs.font.clone(),
                prefs.size.clone(),
                prefs.line_height.clone(),
                prefs.cell_width.clone(),
                prefs.history.clone(),
            )
        };
        set_number(&size, "17.5", window, cx);
        press("up", window, cx)?;
        ensure!(
            size.read(cx).value(cx) == Some(18.5),
            "font size keyboard step"
        );
        show_page(root, SettingsPage::Behavior, window, cx);
        set_number(&history, "12345", window, cx);
        ensure!(
            line_height.read(cx).input.read(cx).text == "1.2",
            "default line height formatting"
        );
        show_page(root, SettingsPage::Appearance, window, cx);
        set_number(&line_height, "1.2", window, cx);
        press("up", window, cx)?;
        press("up", window, cx)?;
        ensure!(
            line_height.read(cx).input.read(cx).text == "1.4",
            "decimal step exposed rounding noise"
        );
        ensure!(
            cell_width.read(cx).input.read(cx).text == "0.6",
            "default cell width formatting"
        );
        set_number(&cell_width, "0.6", window, cx);
        press("up", window, cx)?;
        press("up", window, cx)?;
        ensure!(
            cell_width.read(cx).input.read(cx).text == "0.7",
            "cell width decimal step"
        );
        root.update(cx, |app, _| {
            app.draft.ctrl_scroll_zoom = false;
            app.draft.detect_urls = false;
        });
        root.update(cx, |app, cx| app.save_settings(cx));
        let saved = Settings::load(&root.read(cx).dir)?;
        ensure!(
            !root.read(cx).has_settings_changes(cx),
            "saving left the settings marked as changed"
        );
        ensure!(
            saved.font_size == 18.5
                && saved.scrollback == 12345
                && saved.line_height == 1.4
                && saved.cell_width == 0.7,
            "numeric settings did not persist"
        );
        ensure!(
            !saved.ctrl_scroll_zoom && !saved.detect_urls,
            "interaction settings did not persist"
        );
        ensure!(
            root.read(cx)
                .tabs
                .iter()
                .filter_map(|tab| tab.terminal.as_ref())
                .all(|terminal| !terminal.read(cx).settings.ctrl_scroll_zoom
                    && !terminal.read(cx).settings.detect_urls),
            "interaction settings did not apply to existing terminals"
        );
        ensure!(
            root.read(cx)
                .tabs
                .iter()
                .filter_map(|tab| tab.terminal.as_ref())
                .all(|terminal| terminal.read(cx).settings.line_height == 1.4),
            "line height did not apply to existing terminals"
        );
        ensure!(
            root.read(cx)
                .tabs
                .iter()
                .filter_map(|tab| tab.terminal.as_ref())
                .all(|terminal| terminal.read(cx).settings.cell_width == 0.7),
            "cell width did not apply to existing terminals"
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
            (&line_height, "NaN"),
            (&line_height, "inf"),
            (&line_height, "0.49"),
            (&line_height, "3.1"),
            (&cell_width, "NaN"),
            (&cell_width, "inf"),
            (&cell_width, "0.29"),
            (&cell_width, "2.01"),
            (&history, "1000001"),
            (&history, "1.5"),
            (&history, "-1"),
        ] {
            show_page(
                root,
                if input.entity_id() == history.entity_id() {
                    SettingsPage::Behavior
                } else {
                    SettingsPage::Appearance
                },
                window,
                cx,
            );
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
        show_page(root, SettingsPage::Appearance, window, cx);
        // Only this explicit font action may start discovery. Type before its
        // asynchronous completion to ensure the query survives the result.
        let scroll = root.read(cx).settings_scroll.clone();
        let offset = scroll.offset().y + scroll.bounds().center().y
            - font.read(cx).trigger_bounds().center().y;
        scroll.set_offset(point(px(0.), offset.clamp(-scroll.max_offset().y, px(0.))));
        draw(window, cx);
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
            font.read(cx).open && font.read(cx).search.read(cx).focus.is_focused(window),
            "font popup lost focus while loading (open={}, search_focus={}, settings_theme={}, draft_theme={})",
            font.read(cx).open,
            font.read(cx).search.read(cx).focus.is_focused(window),
            root.read(cx).settings.theme,
            root.read(cx).draft.theme,
        );
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
            .set_offset(point(px(0.), -px(10_000.)));
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
    profile_search(root, handle, cx).await?;
    narrow_layout(root, handle, cx).await?;
    languages(root, handle, cx).await?;
    Ok(())
}

async fn languages(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (original, active, page) = root.read_with(cx, |app, _| {
        (app.settings.clone(), app.active, app.settings_page)
    });
    let second = cx.update(windows::open_default)?;
    let second_entry = cx.update(|cx| {
        cx.global::<windows::Windows>()
            .entries
            .iter()
            .find(|entry| entry.root.upgrade().is_some_and(|entity| entity == second))
            .unwrap()
            .clone()
    });
    second_entry.handle.update(cx, |app, window, cx| {
        app.settings_tab(window, cx);
    })?;
    handle.update(cx, |_, window, _| window.activate_window())?;
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    let outcome = (|| -> Result<()> {
        for (index, code) in ["en", "ko", "system"].into_iter().enumerate() {
            let select = handle.update(cx, |_, window, cx| -> Result<Entity<Select>> {
                root.update(cx, |app, cx| app.settings_tab(window, cx));
                show_page(root, SettingsPage::Behavior, window, cx);
                let select = root.read(cx).preferences.as_ref().unwrap().language.clone();
                select.update(cx, |select, cx| select.toggle(window, cx));
                draw(window, cx);
                for _ in 0..3 {
                    press("up", window, cx)?;
                }
                let target = match code {
                    "en" => 1,
                    "ko" => 2,
                    _ => 0,
                };
                for _ in 0..target {
                    press("down", window, cx)?;
                }
                press("enter", window, cx)?;
                Ok(select)
            })??;
            handle.update(cx, |_, window, cx| -> Result<()> {
            ensure!(
                root.read(cx).draft.language == code,
                "language selector did not update draft (wanted={code}, draft={}, selected={}, open={})",
                root.read(cx).draft.language, select.read(cx).selected, select.read(cx).open
            );
            root.update(cx, |app, cx| app.save_settings(cx));
            draw(window, cx);
            ensure!(
                Settings::load(&root.read(cx).dir)?.language == code,
                "language did not persist"
            );
            if index < 2 {
                ensure!(
                    crate::i18n::tr("settings") == if code == "ko" { "설정" } else { "Settings" },
                    "UI language did not switch"
                );
            }
            window
                .render_to_image()?
                .save(root.read(cx).dir.join(format!("language-{code}.png")))?;
            Ok(())
        })??;
            second_entry
                .handle
                .update(cx, |app, window, cx| -> Result<()> {
                    app.ensure_preferences(window, cx);
                    ensure!(
                        app.settings.language == code && app.draft.language == code,
                        "language did not reach the second window"
                    );
                    ensure!(
                        app.preferences.as_ref().unwrap().language.read(cx).selected == code,
                        "second window selector was not rebuilt"
                    );
                    Ok(())
                })??;
        }
        handle.update(cx, |_, window, cx| -> Result<()> {
            root.update(cx, |app, cx| {
                app.draft.language = "ko".into();
                app.cancel_settings_changes(cx);
            });
            ensure!(
                root.read(cx).draft.language == "system",
                "cancel applied a language change"
            );
            root.update(cx, |app, cx| {
                app.draft = original;
                app.preferences = None;
                app.save_settings(cx);
                app.settings_page = page;
                app.activate(active, window, cx);
            });
            draw(window, cx);
            Ok(())
        })??;
        Ok(())
    })();
    let _ = second_entry
        .handle
        .update(cx, |app, window, cx| app.close_window(window, cx));
    outcome
}

async fn narrow_layout(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (original_size, active, page) = handle.update(cx, |_, window, cx| {
        let original = (
            window.viewport_size(),
            root.read(cx).active,
            root.read(cx).settings_page,
        );
        root.update(cx, |app, cx| app.settings_tab(window, cx));
        window.resize(size(px(560.), px(500.)));
        original
    })?;
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    handle.update(cx, |_, window, cx| -> Result<()> {
        for (page, name) in [
            (SettingsPage::Appearance, "appearance"),
            (SettingsPage::Profiles, "profiles"),
        ] {
            show_page(root, page, window, cx);
            window
                .render_to_image()?
                .save(root.read(cx).dir.join(format!("{name}-narrow.png")))?;
        }
        window.resize(original_size);
        root.update(cx, |app, cx| {
            app.settings_page = page;
            app.activate(active, window, cx);
        });
        Ok(())
    })??;
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    Ok(())
}

async fn profile_search(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let original_active = root.read_with(cx, |app, _| app.active);
    handle.update(cx, |_, window, cx| {
        root.update(cx, |app, cx| app.settings_tab(window, cx));
        draw(window, cx);
        let size = root.read(cx).preferences.as_ref().unwrap().size.clone();
        set_number(&size, "unsaved", window, cx);
        root.update(cx, |app, cx| app.search_profiles(cx));
        assert!(
            cx.global::<super::super::preferences::ProfileSearch>()
                .searching
        );
        // Repeated clicks/windows share the same in-flight operation.
        root.update(cx, |app, cx| app.search_profiles(cx));
    })?;
    let began = Instant::now();
    while cx.update(|cx| {
        cx.global::<super::super::preferences::ProfileSearch>()
            .searching
    }) {
        ensure!(
            began.elapsed() < Duration::from_secs(15),
            "manual profile search timed out"
        );
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    handle.update(cx, |_, window, cx| -> Result<()> {
        let app = root.read(cx);
        let saved = Settings::load(&app.dir)?;
        ensure!(
            !saved.discovered_profiles.is_empty(),
            "manual search did not persist discoveries"
        );
        ensure!(
            app.settings.discovered_profiles == saved.discovered_profiles,
            "search not shared with the window"
        );
        let prefs = app.preferences.as_ref().unwrap();
        ensure!(
            prefs.size.read(cx).input.read(cx).text == "unsaved",
            "search discarded unsaved settings input"
        );
        let values = prefs.profile.read(cx).visible_values();
        ensure!(
            saved
                .discovered_profiles
                .iter()
                .all(|p| values.contains(&p.id)),
            "profile selector did not update after search"
        );
        let size = prefs.size.clone();
        let screenshot = app.dir.join("profile-search.png");
        set_number(&size, "15", window, cx);
        window.refresh();
        window.draw(cx).clear(cx);
        window.render_to_image()?.save(screenshot)?;
        root.update(cx, |app, cx| app.activate(original_active, window, cx));
        Ok(())
    })??;
    Ok(())
}

/// Sample away from title glyphs, icons, borders, and the active indicator.
fn assert_tab_background(
    root: &Entity<TerminalApp>,
    index: usize,
    expected: Hsla,
    window: &Window,
    cx: &App,
) -> Result<()> {
    let app = root.read(cx);
    let (_, width, _) = app.tab_dimensions(window);
    let x = TerminalApp::tab_inset()
        + width * (index as f32 + 0.5)
        + f32::from(app.tab_scroll.offset().x);
    let scale = window.scale_factor();
    let image = window.render_to_image()?;
    let actual = image.get_pixel((x * scale) as u32, (6. * scale) as u32);
    let expected = expected.to_rgb();
    for (actual, expected) in actual.0[..3]
        .iter()
        .zip([expected.r, expected.g, expected.b])
    {
        ensure!(
            (*actual as f32 - expected * 255.).abs() <= 1.,
            "tab {index} background mismatch: pixel channel {actual}, expected {}",
            expected * 255.
        );
    }
    Ok(())
}

async fn color_schemes(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let original = root.read_with(cx, |app, _| app.settings.clone());
    let dir = root.read_with(cx, |app, _| app.dir.clone());
    handle.update(cx, |_, window, cx| -> Result<()> {
        root.update(cx, |app, cx| app.settings_tab(window, cx));
        draw(window, cx);
        show_page(root, SettingsPage::ColorSchemes, window, cx);
        window
            .render_to_image()?
            .save(dir.join("color-schemes-page.png"))?;
        // Click the second gallery card using the visible scroll viewport.
        let bounds = root.read(cx).settings_scroll.bounds();
        click(bounds.origin + point(px(100.), px(240.)), window, cx);
        ensure!(
            root.read(cx).draft.color_scheme == "Campbell Powershell",
            "gallery card click failed"
        );
        ensure!(
            root.read(cx).settings == original,
            "scheme applied before saving"
        );
        ensure!(
            root.read(cx).has_settings_changes(cx),
            "scheme change not detected"
        );
        // Gallery cards remain keyboard-selectable without the appearance dropdown.
        press("tab", window, cx)?;
        press("enter", window, cx)?;
        ensure!(
            root.read(cx).draft.color_scheme == "CGA",
            "gallery keyboard selection failed"
        );
        root.update(cx, |app, cx| app.choose_color_scheme("Solarized Dark", cx));
        Ok(())
    })??;
    cx.background_executor()
        .timer(Duration::from_millis(20))
        .await;
    handle.update(cx, |_, window, cx| -> Result<()> {
        ensure!(
            root.read(cx).draft.color_scheme == "Solarized Dark",
            "gallery selection failed"
        );
        root.update(cx, |app, cx| app.save_settings(cx));
        ensure!(
            Settings::load(&dir)?.color_scheme == "Solarized Dark",
            "scheme not persisted"
        );
        ensure!(
            root.read(cx)
                .tabs
                .iter()
                .filter_map(|tab| tab.terminal.as_ref())
                .all(|t| t.read(cx).settings.color_scheme == "Solarized Dark"),
            "scheme not applied to existing terminals"
        );
        let terminal_index = root
            .read(cx)
            .tabs
            .iter()
            .position(|tab| tab.terminal.is_some())
            .unwrap();
        root.update(cx, |app, cx| app.activate(terminal_index, window, cx));
        draw(window, cx);
        window
            .render_to_image()?
            .save(dir.join("solarized-dark-terminal.png"))?;
        for (scheme, app_theme, screenshot) in [
            ("Solarized Light", "dark", "solarized-light-terminal.png"),
            ("Ubuntu", "light", "ubuntu-light-ui-terminal.png"),
            ("Ubuntu", "dark", "ubuntu-terminal.png"),
        ] {
            root.update(cx, |app, cx| {
                app.settings_tab(window, cx);
                app.choose_color_scheme(scheme, cx);
                app.draft.theme = app_theme.into();
                app.save_settings(cx);
                app.activate(terminal_index, window, cx);
            });
            ensure!(
                Settings::load(&dir)?.color_scheme == scheme,
                "scheme not persisted: {scheme}"
            );
            draw(window, cx);
            let image = window.render_to_image()?;
            image.save(dir.join(screenshot))?;
            let scheme_background = crate::config::color_schemes::resolve(scheme).background;
            assert_tab_background(
                root,
                terminal_index,
                rgb(scheme_background).into(),
                window,
                cx,
            )?;
            // An inactive tab keeps the tab-row color even with a light terminal palette.
            if let Some(inactive) =
                root.read(cx)
                    .tabs
                    .iter()
                    .enumerate()
                    .find_map(|(index, tab)| {
                        (index != terminal_index && tab.terminal.is_some()).then_some(index)
                    })
            {
                let theme = Theme::from_setting(app_theme, cx);
                assert_tab_background(root, inactive, theme.bar, window, cx)?;
            }
            // Hovering an automatic active tab must not break its background match.
            let (_, width, _) = root.read(cx).tab_dimensions(window);
            let position = point(
                px(TerminalApp::tab_inset() + width * (terminal_index as f32 + 0.5)),
                px(6.),
            );
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    ..Default::default()
                }),
                cx,
            );
            draw(window, cx);
            assert_tab_background(
                root,
                terminal_index,
                rgb(scheme_background).into(),
                window,
                cx,
            )?;
            // Explicit tab colors retain their existing tint, then clearing restores the scheme.
            root.update(cx, |app, cx| {
                app.tabs[terminal_index].color = Some(0xe74856);
                cx.notify();
            });
            draw(window, cx);
            let theme = Theme::from_setting(app_theme, cx);
            let tint = theme.bg.blend(Hsla::from(rgb(0xe74856)).opacity(0.3));
            // Move off the tab so this assertion measures its normal, rather than hover, tint.
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: point(px(800.), px(300.)),
                    ..Default::default()
                }),
                cx,
            );
            draw(window, cx);
            assert_tab_background(root, terminal_index, tint, window, cx)?;
            root.update(cx, |app, cx| {
                app.tabs[terminal_index].color = None;
                cx.notify();
            });
            draw(window, cx);
            assert_tab_background(
                root,
                terminal_index,
                rgb(scheme_background).into(),
                window,
                cx,
            )?;
        }
        root.update(cx, |app, cx| {
            app.settings_tab(window, cx);
            app.choose_color_scheme("Vintage", cx);
            app.cancel_settings_changes(cx);
        });
        draw(window, cx);
        ensure!(
            root.read(cx).draft.color_scheme == "Ubuntu",
            "cancel did not revert scheme"
        );
        original.save(&dir)?;
        root.update(cx, |app, cx| app.reload_settings(cx));
        draw(window, cx);
        ensure!(root.read(cx).settings == original, "scheme reload failed");
        Ok(())
    })??;
    Ok(())
}

fn pages_and_actions(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> Result<()> {
    let original = root.read(cx).settings.clone();
    let dir = root.read(cx).dir.clone();
    let saved_file = std::fs::read(dir.join("settings.toml"))?;
    show_page(root, SettingsPage::Appearance, window, cx);
    let size = root.read(cx).preferences.as_ref().unwrap().size.clone();
    ensure!(
        !root.read(cx).has_settings_changes(cx),
        "settings initially marked as changed"
    );
    window
        .render_to_image()?
        .save(dir.join("save-button-clean.png"))?;
    set_number(&size, &format!("{:.1}", original.font_size), window, cx);
    ensure!(
        !root.read(cx).has_settings_changes(cx),
        "equivalent number formatting marked as changed"
    );
    set_number(&size, &(original.font_size + 1.).to_string(), window, cx);
    ensure!(
        root.read(cx).has_settings_changes(cx),
        "numeric edit not detected before saving"
    );
    draw(window, cx);
    window
        .render_to_image()?
        .save(dir.join("save-button-changed.png"))?;
    set_number(&size, &original.font_size.to_string(), window, cx);
    ensure!(
        !root.read(cx).has_settings_changes(cx),
        "reverting numeric edit retained change state"
    );
    set_number(&size, "unsaved", window, cx);
    ensure!(
        root.read(cx).has_settings_changes(cx),
        "invalid numeric edit not detected"
    );
    root.update(cx, |app, cx| {
        app.draft.confirm_close = !original.confirm_close;
        app.draft.ctrl_scroll_zoom = !original.ctrl_scroll_zoom;
        app.draft.detect_urls = !original.detect_urls;
        app.set_shortcut_keys("newTab", Some(vec!["ctrl-y".into()]), cx);
    });
    for (page, name) in [
        (SettingsPage::Behavior, "behavior"),
        (SettingsPage::Appearance, "appearance"),
        (SettingsPage::ColorSchemes, "color-schemes"),
        (SettingsPage::Profiles, "profiles"),
        (SettingsPage::Shortcuts, "shortcuts"),
    ] {
        show_page(root, page, window, cx);
        ensure!(
            size.read(cx).input.read(cx).text == "unsaved",
            "navigation discarded numeric input"
        );
        ensure!(
            root.read(cx).settings == original,
            "navigation applied unsaved settings"
        );
        window
            .render_to_image()?
            .save(dir.join(format!("{name}-page.png")))?;
    }
    // All pages share the footer. Click its cancel action after removing the status row.
    root.update(cx, |app, cx| {
        app.status = None;
        cx.notify();
    });
    draw(window, cx);
    let viewport = window.viewport_size();
    click(
        point(viewport.width - px(170.), viewport.height - px(29.)),
        window,
        cx,
    );
    ensure!(
        root.read(cx).draft == original,
        "cancel did not restore all settings pages"
    );
    ensure!(
        !root.read(cx).has_settings_changes(cx),
        "cancel retained change state"
    );
    ensure!(
        root.read(cx).shortcut_capture.is_none(),
        "cancel left shortcut capture active"
    );
    ensure!(
        root.read(cx)
            .preferences
            .as_ref()
            .unwrap()
            .size
            .read(cx)
            .value(cx)
            == Some(original.font_size as f64),
        "cancel retained invalid numeric input"
    );
    ensure!(
        std::fs::read(dir.join("settings.toml"))? == saved_file,
        "cancel wrote the settings file"
    );

    root.update(cx, |app, cx| {
        app.set_shortcut_keys("newTab", Some(vec!["ctrl-y".into()]), cx);
        app.status = None;
    });
    ensure!(
        root.read(cx).has_settings_changes(cx),
        "shortcut edit not detected"
    );
    draw(window, cx);
    click(
        point(viewport.width - px(295.), viewport.height - px(29.)),
        window,
        cx,
    );
    let defaults = Settings {
        profiles: original.profiles.clone(),
        discovered_profiles: original.discovered_profiles.clone(),
        allow_osc52: original.allow_osc52,
        ..Settings::default()
    };
    ensure!(
        root.read(cx).draft == defaults,
        "reset did not restore settings and shortcuts together"
    );
    ensure!(
        root.read(cx).settings == original,
        "reset applied before saving"
    );
    ensure!(
        std::fs::read(dir.join("settings.toml"))? == saved_file,
        "reset wrote before saving"
    );
    root.update(cx, |app, cx| app.cancel_settings_changes(cx));
    draw(window, cx);

    // Cancel uses the applied state; reload explicitly reads external file edits.
    let mut external = original.clone();
    external.font_size = if original.font_size == 20. { 21. } else { 20. };
    external.save(&dir)?;
    root.update(cx, |app, cx| app.cancel_settings_changes(cx));
    ensure!(
        root.read(cx).draft == original,
        "cancel unexpectedly loaded external edits"
    );
    root.update(cx, |app, cx| {
        app.status = None;
        cx.notify();
    });
    draw(window, cx);
    click(point(px(90.), viewport.height - px(29.)), window, cx);
    ensure!(
        root.read(cx).settings == external && root.read(cx).draft == external,
        "footer reload did not apply external edits"
    );
    original.save(&dir)?;
    root.update(cx, |app, cx| app.reload_settings(cx));
    draw(window, cx);
    Ok(())
}

fn scrollbars(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> Result<()> {
    for (page, name) in [
        (SettingsPage::Behavior, "behavior"),
        (SettingsPage::Shortcuts, "shortcuts"),
    ] {
        root.update(cx, |app, cx| {
            app.settings_page = page;
            cx.notify();
        });
        draw(window, cx);
        let scroll = match page {
            SettingsPage::Shortcuts => root.read(cx).shortcuts_scroll.clone(),
            _ => root.read(cx).settings_scroll.clone(),
        };
        let bounds = scroll.bounds();
        let max = scroll.max_offset().y;
        if max <= px(0.) && page != SettingsPage::Shortcuts {
            ensure!(
                scroll.offset().y == px(0.),
                "non-scrolling page retained an offset"
            );
            continue;
        }
        ensure!(
            max > px(0.),
            "{name} settings must overflow in the test window"
        );
        scroll.set_offset(point(px(0.), px(0.)));
        draw(window, cx);
        window
            .render_to_image()?
            .save(root.read(cx).dir.join(format!("{name}-scrollbar-top.png")))?;

        // Wheel input over the scrollbar still reaches the content behind it.
        let position = point(bounds.right() - px(6.), bounds.center().y);
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::Scroll(ScrollEvent {
                position,
                delta: ScrollDelta::Lines(point(0., -3.)),
                modifiers: Modifiers::default(),
                touch_phase: TouchPhase::Moved,
                source: ScrollSource::Pointer,
            }),
            cx,
        );
        draw(window, cx);
        ensure!(
            scroll.offset().y < px(0.),
            "{name} scrollbar blocked the wheel"
        );

        // Clicking the track jumps toward that position.
        click(point(position.x, bounds.bottom() - px(2.)), window, cx);
        ensure!(
            (scroll.offset().y + max).abs() < px(1.),
            "{name} scrollbar track click"
        );
        scroll.set_offset(point(px(0.), px(0.)));
        draw(window, cx);

        let height = bounds.size.height;
        let thumb = (height * (height / (height + max)))
            .max(px(24.))
            .min(height);
        let start = point(position.x, bounds.top() + thumb / 2.);
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: start,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position: start,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        draw(window, cx);
        ensure!(
            root.read(cx).settings_scroll_drag.is_some(),
            "{name} scrollbar thumb did not grab"
        );
        let outside = point(start.x - px(80.), bounds.bottom() + px(40.));
        window.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: outside,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            cx,
        );
        draw(window, cx);
        ensure!(
            (scroll.offset().y + max).abs() < px(1.),
            "{name} scrollbar drag outside the track"
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position: outside,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        draw(window, cx);
        ensure!(
            root.read(cx).settings_scroll_drag.is_none(),
            "{name} scrollbar did not release outside the track"
        );
        window.render_to_image()?.save(
            root.read(cx)
                .dir
                .join(format!("{name}-scrollbar-bottom.png")),
        )?;
        scroll.set_offset(point(px(0.), px(0.)));
    }
    root.update(cx, |app, cx| {
        app.settings_page = SettingsPage::Profiles;
        cx.notify();
    });
    draw(window, cx);
    Ok(())
}

use super::*;

fn draw(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn move_to(x: f32, y: f32, held: bool, window: &mut Window, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position: point(px(x), px(y)),
            pressed_button: held.then_some(MouseButton::Left),
            ..Default::default()
        }),
        cx,
    );
    draw(window, cx);
}

fn press(x: f32, window: &mut Window, cx: &mut App) {
    move_to(x, 18., false, window, cx);
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: point(px(x), px(18.)),
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    draw(window, cx);
}

fn release(x: f32, y: f32, window: &mut Window, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: point(px(x), px(y)),
            click_count: 1,
            ..Default::default()
        }),
        cx,
    );
    draw(window, cx);
}

fn order(root: &Entity<TerminalApp>, cx: &App) -> Vec<u64> {
    root.read(cx).tabs.iter().map(|tab| tab.id).collect()
}

fn screenshot(
    root: &Entity<TerminalApp>,
    name: &str,
    window: &mut Window,
    cx: &App,
) -> anyhow::Result<()> {
    window
        .render_to_image()?
        .save(root.read(cx).dir.join(name))?;
    Ok(())
}

pub(super) fn run(
    root: &Entity<TerminalApp>,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<()> {
    let original = order(root, cx);
    let active_id = root.read(cx).tabs[root.read(cx).active].id;
    root.update(cx, |app, cx| {
        app.hide_search(window, cx);
        app.tab_scroll.set_offset(point(px(0.), px(0.)));
    });
    draw(window, cx);
    let (_, width, _) = root.read(cx).tab_dimensions(window);
    let inset = if cfg!(target_os = "macos") { 78. } else { 0. };
    let start = inset + width * 0.35;
    press(start, window, cx);
    move_to(start + 3., 18., true, window, cx);
    anyhow::ensure!(
        order(root, cx) == original,
        "tiny pointer movement reordered tabs"
    );
    anyhow::ensure!(
        root.read(cx)
            .tab_drag
            .as_ref()
            .is_some_and(|drag| !drag.started),
        "drag threshold ignored"
    );
    release(start + 3., 18., window, cx);
    anyhow::ensure!(
        root.read(cx).tab_drag.is_none(),
        "click left stale drag state"
    );

    press(start, window, cx);
    let destination = start + width * 1.65;
    move_to(destination, 18., true, window, cx);
    anyhow::ensure!(
        root.read(cx).tabs[2].id == original[0],
        "tabs did not reorder while dragging"
    );
    anyhow::ensure!(
        root.read(cx).tabs[root.read(cx).active].id == original[0],
        "drag changed active tab identity"
    );
    anyhow::ensure!(
        !root.read(cx).tab_motions.is_empty(),
        "neighbor slide animation missing"
    );
    screenshot(root, "tabs-dragging.png", window, cx)?;
    move_to(start + width * 0.65, 18., true, window, cx);
    anyhow::ensure!(
        root.read(cx).tabs[1].id == original[0],
        "drag reversal did not follow pointer"
    );
    move_to(destination, 18., true, window, cx);
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(16));
        draw(window, cx);
    }
    screenshot(root, "tabs-dragging-settled.png", window, cx)?;
    let theme = root.read(cx).settings.theme.clone();
    root.update(cx, |app, cx| {
        app.settings.theme = "light".into();
        cx.notify();
    });
    draw(window, cx);
    screenshot(root, "tabs-dragging-light.png", window, cx)?;
    root.update(cx, |app, cx| {
        app.settings.theme = theme;
        cx.notify();
    });
    draw(window, cx);
    release(destination, 150., window, cx);
    anyhow::ensure!(
        root.read(cx).tab_drag.is_none(),
        "release outside strip left stale drag state"
    );
    anyhow::ensure!(
        root.read(cx).tabs[2].id == original[0],
        "release changed previewed order"
    );

    // Drag back and then cancel: restore the order at the start of this drag.
    let committed = order(root, cx);
    root.update(cx, |app, _| app.tab_motions.clear());
    draw(window, cx);
    press(inset + width * 2.35, window, cx);
    move_to(start, 18., true, window, cx);
    anyhow::ensure!(
        root.read(cx).tabs[0].id == original[0],
        "reverse drag did not reorder"
    );
    window.dispatch_keystroke(Keystroke::parse("escape")?, cx);
    draw(window, cx);
    anyhow::ensure!(
        order(root, cx) == committed,
        "Escape did not restore original order"
    );
    anyhow::ensure!(
        root.read(cx).tab_drag.is_none(),
        "Escape left stale drag state"
    );
    release(start, 18., window, cx);

    // Overflow with lightweight failed terminals, without starting extra shells.
    root.update(cx, |app, cx| {
        for i in 0..12 {
            app.add_terminal(
                Profile::new(
                    &format!("drag-{i}"),
                    &format!("Tab {}", i + 1),
                    "__edolterm_preview_missing_shell__",
                    &[],
                ),
                None,
                window,
                cx,
            );
        }
        app.activate(0, window, cx);
        app.tab_scroll.set_offset(point(px(0.), px(0.)));
        app.tab_motions.clear();
    });
    draw(window, cx);
    let (_, width, extent) = root.read(cx).tab_dimensions(window);
    press(inset + width * 0.35, window, cx);
    let dragged_id = root.read(cx).tabs[root.read(cx).active].id;
    move_to(inset + extent - 2., 18., true, window, cx);
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(16));
        draw(window, cx);
    }
    anyhow::ensure!(
        f32::from(root.read(cx).tab_scroll.offset().x) < -100.,
        "edge auto-scroll did not advance"
    );
    anyhow::ensure!(
        root.read(cx)
            .tabs
            .iter()
            .position(|tab| tab.id == dragged_id)
            .unwrap()
            > 3,
        "overflow drag did not reach hidden tabs"
    );
    screenshot(root, "tabs-overflow-dragging.png", window, cx)?;
    // Release outside the window as well as outside the strip.
    release(inset + extent, -20., window, cx);
    anyhow::ensure!(
        root.read(cx).tab_drag.is_none(),
        "outside-window release left stale drag state"
    );
    // Closing a tab must still work after a completed drag.
    root.update(cx, |app, cx| {
        app.tab_motions.clear();
        app.tab_scroll.set_offset(point(px(0.), px(0.)));
        app.activate(0, window, cx);
    });
    draw(window, cx);
    let close_id = root.read(cx).tabs[0].id;
    let close_x = inset + width - 19.;
    press(close_x, window, cx);
    release(close_x, 18., window, cx);
    anyhow::ensure!(
        !order(root, cx).contains(&close_id),
        "close button stopped working after drag"
    );
    root.update(cx, |app, cx| {
        app.tabs.retain(|tab| original.contains(&tab.id));
        app.tabs
            .sort_by_key(|tab| original.iter().position(|id| *id == tab.id).unwrap());
        app.tab_motions.clear();
        let index = app.tabs.iter().position(|tab| tab.id == active_id).unwrap();
        app.activate(index, window, cx);
        app.tab_scroll.set_offset(point(px(0.), px(0.)));
    });
    draw(window, cx);
    Ok(())
}

pub(super) async fn rename(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    let (active, terminal, shell_title, target, x) =
        handle.update(cx, |_, window, cx| -> anyhow::Result<_> {
            let active = root.read(cx).active;
            root.update(cx, |app, cx| {
                app.activate(0, window, cx);
                app.tab_scroll.set_offset(point(px(0.), px(0.)));
                app.tab_motions.clear();
            });
            draw(window, cx);
            let (_, width, _) = root.read(cx).tab_dimensions(window);
            let inset = if cfg!(target_os = "macos") { 78. } else { 0. };
            let x = inset + width * 1.35;
            let target = root.read(cx).tabs[1].id;
            let terminal = root.read(cx).tabs[1].terminal.clone().unwrap();
            let shell_title = terminal.read(cx).title.clone();
            click_tab(x, MouseButton::Right, 1, window, cx);
            anyhow::ensure!(
                root.read(cx).tab_menu.is_some(),
                "right click did not open context menu"
            );
            screenshot(root, "tab-context-menu.png", window, cx)?;
            window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
            draw(window, cx);
            anyhow::ensure!(
                root.read(cx)
                    .tab_menu
                    .as_ref()
                    .is_some_and(|menu| menu.colors),
                "color menu did not open"
            );
            screenshot(root, "tab-color-palette.png", window, cx)?;
            window.dispatch_keystroke(Keystroke::parse("down")?, cx);
            window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
            draw(window, cx);
            anyhow::ensure!(
                root.read(cx).tabs[1].color == Some(0xe74856),
                "palette did not apply tab color"
            );
            click_tab(x, MouseButton::Right, 1, window, cx);
            window.dispatch_keystroke(Keystroke::parse("down")?, cx);
            window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
            draw(window, cx);
            anyhow::ensure!(
                root.read(cx).active == 1,
                "right click did not select the target tab"
            );
            Ok((active, terminal, shell_title, target, x))
        })??;
    handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
        let input = root
            .read(cx)
            .tab_rename
            .as_ref()
            .expect("context menu opens rename")
            .input
            .clone();
        anyhow::ensure!(
            input.read(cx).focus.is_focused(window),
            "rename input is not focused"
        );
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "개발", Some(2..2), window, cx);
        });
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        Ok(())
    })??;
    handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
        anyhow::ensure!(
            root.read(cx).tab_rename.is_some(),
            "IME Enter submitted the dialog"
        );
        let input = root.read(cx).tab_rename.as_ref().unwrap().input.clone();
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "개발 서버 🙂", window, cx)
        });
        let before = root.read(cx).tabs.len();
        window.dispatch_keystroke(Keystroke::parse("ctrl-shift-t")?, cx);
        anyhow::ensure!(
            root.read(cx).tabs.len() == before,
            "app shortcut escaped rename dialog"
        );
        draw(window, cx);
        screenshot(root, "tabs-rename-dialog.png", window, cx)?;
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        Ok(())
    })??;
    handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
        draw(window, cx);
        anyhow::ensure!(
            root.read(cx).tab_rename.is_none(),
            "Enter did not save rename"
        );
        anyhow::ensure!(
            root.read(cx).tabs[1].custom_title.as_deref() == Some("개발 서버 🙂"),
            "Unicode rename failed"
        );
        anyhow::ensure!(
            terminal.read(cx).focus.is_focused(window),
            "rename did not restore terminal focus"
        );
        terminal.update(cx, |terminal, cx| {
            terminal.title = "새 자동 제목".into();
            cx.emit(TerminalEvent::Metadata);
        });
        Ok(())
    })??;
    handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
        draw(window, cx);
        anyhow::ensure!(
            root.read(cx).tabs[1].title(cx) == "개발 서버 🙂",
            "shell overwrote custom title"
        );
        assert_window_title(window, "개발 서버 🙂 — edolterm")?;
        screenshot(root, "tabs-renamed.png", window, cx)?;
        let saved = SavedSession::load(&root.read(cx).dir)?;
        anyhow::ensure!(
            saved.tabs[1].custom_title.as_deref() == Some("개발 서버 🙂"),
            "rename was not persisted"
        );

        // Build a restored app using the same session and verify its title immediately.
        let restored = cx.new(|cx| {
            let app = root.read(cx);
            TerminalApp::new(
                app.dir.clone(),
                app.settings.clone(),
                Startup::Restore(saved),
                None,
                window,
                cx,
            )
        });
        anyhow::ensure!(
            restored.read(cx).tabs[1].title(cx) == "개발 서버 🙂",
            "session restore lost custom title"
        );
        anyhow::ensure!(
            restored.read(cx).tabs[1].color == Some(0xe74856),
            "session restore lost tab color"
        );
        restored.update(cx, |app, cx| {
            for tab in &app.tabs {
                if let Some(terminal) = &tab.terminal {
                    terminal.update(cx, |terminal, _| terminal.shutdown());
                }
            }
        });
        drop(restored);
        root.update(cx, |app, cx| app.activate(1, window, cx));
        draw(window, cx);

        click_tab(x, MouseButton::Left, 2, window, cx);
        let input = root
            .read(cx)
            .tab_rename
            .as_ref()
            .expect("double click opens rename")
            .input
            .clone();
        input.update(cx, |input, cx| input.set_text("discard me", window, cx));
        window.dispatch_keystroke(Keystroke::parse("escape")?, cx);
        draw(window, cx);
        anyhow::ensure!(
            root.read(cx).tab_rename.is_none(),
            "Escape did not cancel rename"
        );
        anyhow::ensure!(
            root.read(cx).tabs[1].custom_title.as_deref() == Some("개발 서버 🙂"),
            "cancel modified the title"
        );

        click_tab(x, MouseButton::Right, 1, window, cx);
        window.dispatch_keystroke(Keystroke::parse("down")?, cx);
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        draw(window, cx);
        let input = root.read(cx).tab_rename.as_ref().unwrap().input.clone();
        input.update(cx, |input, cx| input.set_text("   ", window, cx));
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        Ok(())
    })??;
    handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
        draw(window, cx);
        anyhow::ensure!(
            root.read(cx).tabs[1].custom_title.is_none(),
            "blank name did not reset title"
        );
        assert_window_title(window, "새 자동 제목 — edolterm")?;
        anyhow::ensure!(
            SavedSession::load(&root.read(cx).dir)?.tabs[1]
                .custom_title
                .is_none(),
            "reset was not persisted"
        );
        terminal.update(cx, |terminal, cx| {
            terminal.title = shell_title;
            cx.emit(TerminalEvent::Metadata);
        });
        click_tab(x, MouseButton::Right, 1, window, cx);
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        window.dispatch_keystroke(Keystroke::parse("home")?, cx);
        window.dispatch_keystroke(Keystroke::parse("enter")?, cx);
        anyhow::ensure!(
            root.read(cx).tabs[1].color.is_none(),
            "palette reset failed"
        );
        root.update(cx, |app, cx| {
            anyhow::ensure!(app.tabs[1].id == target, "rename changed tab order");
            app.activate(active, window, cx);
            Ok(())
        })?;
        draw(window, cx);
        Ok(())
    })??;
    Ok(())
}

fn click_tab(x: f32, button: MouseButton, count: usize, window: &mut Window, cx: &mut App) {
    move_to(x, 18., false, window, cx);
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            position: point(px(x), px(18.)),
            button,
            click_count: count,
            ..Default::default()
        }),
        cx,
    );
    draw(window, cx);
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            position: point(px(x), px(18.)),
            button,
            click_count: count,
            ..Default::default()
        }),
        cx,
    );
    draw(window, cx);
}

fn assert_window_title(window: &Window, expected: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};
        let Ok(RawWindowHandle::Win32(handle)) =
            HasWindowHandle::window_handle(window).map(|handle| handle.as_raw())
        else {
            anyhow::bail!("expected Windows window handle");
        };
        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        let title = unsafe {
            let mut text = vec![0; GetWindowTextLengthW(hwnd) as usize + 1];
            let len = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
            String::from_utf16_lossy(&text[..len as usize])
        };
        anyhow::ensure!(
            title == expected,
            "window title: {title:?}, expected: {expected:?}"
        );
    }
    #[cfg(target_os = "macos")]
    anyhow::ensure!(window.window_title() == expected, "window title mismatch");
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = (window, expected);
    Ok(())
}

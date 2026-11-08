//! In-process GPUI/PTY integration checks; never inject OS keyboard or mouse input.
use super::*;
use alacritty_terminal::{
    index::Side,
    selection::{Selection, SelectionType},
};

pub fn directory(mode: &str) -> PathBuf {
    let mode = match mode {
        "single" | "terminal" | "menu" | "settings" | "overflow" | "find" | "confirm" => mode,
        _ => "terminal",
    };
    std::env::current_dir()
        .expect("workspace")
        .join("target/ui-smoke")
        .join(mode)
}

pub fn start(root: Entity<TerminalApp>, mode: String, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(800)).await;
        let mut failure = None;
        let result = handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
            // Exercise the actual GPUI keyboard dispatch tree after the first paint.
            let before = root.read(cx).tabs.len();
            window.dispatch_keystroke(Keystroke::parse("ctrl-shift-t")?, cx);
            anyhow::ensure!(root.read(cx).tabs.len() == before + 1, "new-tab shortcut");
            window.dispatch_keystroke(Keystroke::parse("ctrl-tab")?, cx);
            anyhow::ensure!(root.read(cx).active == 0, "tab cycling");
            root.update(cx, |app, cx| {
                let id = app.tabs[0].id;
                app.reorder(id, 1, window, cx);
                assert_eq!(app.tabs[app.active].id, id, "reorder preserves active terminal");
            });
            Ok(())
        });
        if let Err(e) = result.and_then(|r| r) { failure = Some(format!("{e:#}")); }
        cx.background_executor().timer(Duration::from_millis(350)).await;
        if failure.is_none() {
            let result = handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
                window.dispatch_keystroke(Keystroke::parse("ctrl-,")?, cx);
                anyhow::ensure!(root.read(cx).tabs[root.read(cx).active].id == 0, "settings shortcut");
                root.update(cx, |app, cx| {
                    app.draft.font_size = 16.; app.save_settings(cx);
                    assert_eq!(Settings::load(&app.dir).unwrap().font_size, 16.);
                    app.reload_settings(cx);
                    app.activate(0, window, cx);
                    let t = app.active_terminal().unwrap();
                    t.update(cx, |t, cx| {
                        t.replace_and_mark_text_in_range(None, "ㅎ", Some(1..1), window, cx);
                        t.replace_and_mark_text_in_range(None, "한", Some(1..1), window, cx);
                        assert_eq!(t.text_for_range(0..1, &mut None, window, cx).as_deref(), Some("한"));
                        t.unmark_text(window, cx);
                        // Unicode is committed through the same input handler as the OS IME.
                        t.replace_text_in_range(None, "Clear-Host; [Console]::WriteLine(('SMOKE_'+'한글_OK')); [Console]::WriteLine(([char]27).ToString()+'[32mANSI_OK'+([char]27).ToString()+'[0m')\r", window, cx);
                    });
                });
                Ok(())
            });
            if let Err(e) = result.and_then(|r| r) { failure = Some(format!("{e:#}")); }
        }
        let began = Instant::now();
        let mut output_ok = false;
        while failure.is_none() && began.elapsed() < Duration::from_secs(20) {
            output_ok = root.read_with(cx, |app, cx| app.active_terminal().and_then(|t| t.read(cx).session.as_ref().map(|s| s.plain_text().contains("SMOKE_한글_OK")))).unwrap_or(false);
            if output_ok { break; }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
        if !output_ok && failure.is_none() { failure = Some("Unicode PTY output timed out".into()); }
        let result = handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
            root.update(cx, |app, cx| {
                app.show_search(window, cx);
                app.search.update(cx, |field, cx| field.replace_text_in_range(None, "SMOKE_한글_OK", window, cx));
                app.find(false, false, cx);
                let terminal = app.active_terminal().unwrap();
                terminal.update(cx, |t, _| {
                    if let (Some(found), Some(s)) = (&t.search_match, &t.session) {
                        let mut term = s.terminal.lock();
                        let mut selection = Selection::new(SelectionType::Simple, *found.start(), Side::Left);
                        selection.update(*found.end(), Side::Right);
                        term.selection = Some(selection);
                        assert_eq!(term.selection_to_string().as_deref(), Some("SMOKE_한글_OK"));
                        term.selection = None;
                    }
                });
                let search_ok = terminal.read(cx).search_match.is_some();
                if !search_ok { failure.get_or_insert_with(|| "Search did not find Unicode output".into()); }
                match mode.as_str() {
                    "single" => {
                        app.hide_search(window, cx);
                        let keep = app.tabs[app.active].id;
                        let close: Vec<_> = app.tabs.iter().filter(|t| t.id != keep).map(|t| t.id).collect();
                        for id in close { app.close_tab(id, window, cx); }
                    }
                    "menu" => { app.hide_search(window, cx); app.menu = true; app.focus.focus(window, cx); }
                    "settings" => { app.hide_search(window, cx); app.settings_tab(window, cx); }
                    "overflow" => {
                        app.hide_search(window, cx);
                        // Failed profiles avoid spawning 12 real shells just for tab layout QA.
                        for i in 0..12 {
                            let p = Profile::new(&format!("preview-{i}"), &format!("Terminal {}", i + 1), "__edolterm_preview_missing_shell__", &[]);
                            app.add_terminal(p, None, window, cx);
                        }
                        app.activate(0, window, cx);
                    }
                    "find" => {}
                    "confirm" => { app.hide_search(window, cx); app.should_close(window, cx); }
                    _ => app.hide_search(window, cx),
                }
                app.persist(window, cx);
                app.status = failure.clone().map(|e| format!("SMOKE FAILURE: {e}"));
                let result = serde_json::json!({ "passed": failure.is_none() && output_ok && search_ok, "mode": mode, "unicode_pty": output_ok, "search": search_ok, "tabs": app.tabs.len(), "error": failure });
                let _ = config::atomic_write(&app.dir.join("result.json"), serde_json::to_string_pretty(&result).unwrap().as_bytes());
                cx.notify();
            });
            Ok(())
        });
        if let Err(e) = result.and_then(|r| r) { eprintln!("SMOKE FAILURE: {e:#}"); }
        #[cfg(feature = "ui-tests")]
        {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let _ = handle.update(cx, |_, window, cx| {
                // Hidden test windows do not receive presentation ticks from DWM.
                window.refresh();
                window.draw(cx).clear(cx);
                let path = root.read(cx).dir.join("screenshot.png");
                match window.render_to_image().and_then(|image| image.save(path).map_err(Into::into)) {
                    Ok(()) => {},
                    Err(e) => eprintln!("Screenshot failed: {e:#}"),
                }
            });
        }
        // A test runner can request graceful cleanup without automating the desktop.
        loop {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            let stop = root.read_with(cx, |app, _| app.dir.join("stop").exists());
            if stop {
                let _ = handle.update(cx, |_, window, cx| root.update(cx, |app, cx| app.close_window(window, cx)));
                break;
            }
        }
    }).detach();
}

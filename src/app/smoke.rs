//! In-process GPUI/PTY integration checks; never inject OS keyboard or mouse input.
use super::*;
use alacritty_terminal::{
    index::Side,
    selection::{Selection, SelectionType},
};

#[cfg(feature = "ui-tests")]
mod file_drop;
#[cfg(all(feature = "ui-tests", windows))]
mod instances;
#[cfg(all(feature = "ui-tests", windows))]
mod keys;
#[cfg(feature = "ui-tests")]
mod scroll;
#[cfg(feature = "ui-tests")]
mod settings_controls;
#[cfg(feature = "ui-tests")]
mod shortcuts;
#[cfg(feature = "ui-tests")]
mod tabs;
#[cfg(feature = "ui-tests")]
mod taskbar;
#[cfg(feature = "ui-tests")]
mod updates;

pub fn directory(mode: &str) -> PathBuf {
    let mode = match mode {
        "single" | "terminal" | "menu" | "settings" | "overflow" | "find" | "confirm"
        | "scroll" | "drop" | "tabs" | "instances" | "keys" | "shortcuts" | "layout" | "links"
        | "updates" | "updates-preview" => mode,
        _ => "terminal",
    };
    std::env::current_dir()
        .expect("workspace")
        .join("target/ui-smoke")
        .join(mode)
}

pub fn start(
    root: Entity<TerminalApp>,
    mode: String,
    requested_directory: Option<PathBuf>,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(feature = "ui-tests")]
    if mode == "updates-preview" {
        if let Err(error) = updates::preview(&root, window, cx) {
            eprintln!("Update preview: {error:#}");
        }
        return;
    }
    #[cfg(all(feature = "ui-tests", windows))]
    if mode == "instances" {
        instances::start(root, cx);
        return;
    }
    let handle = window.window_handle();
    #[cfg(feature = "ui-tests")]
    let taskbar_probe = (mode == "single").then(|| taskbar::probe(&root, window, cx));
    #[cfg(feature = "ui-tests")]
    if mode == "settings" {
        assert!(
            root.read(cx).preferences.is_none(),
            "settings controls allocated at startup"
        );
        assert!(
            !crate::select::Select::font_discovery_started(cx),
            "fonts enumerated at startup"
        );
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(800)).await;
        let mut failure = None;
        #[cfg(all(feature = "ui-tests", windows))]
        if let Err(error) = taskbar::profile_icons(&root, cx).await {
            failure = Some(format!("{error:#}"));
        }
        #[cfg(feature = "ui-tests")]
        if let Some(probe) = taskbar_probe
            && let Err(error) = taskbar::verify(probe, cx).await
        {
            failure = Some(format!("{error:#}"));
        }
        if let Some(directory) = requested_directory {
            let marker = format!("EXPLORER_START_{}", directory.display());
            root.read_with(cx, |app, cx| {
                if app.tabs.len() != 1 { failure = Some("Explorer launch restored unrelated tabs".into()); }
                if let Some(terminal) = app.active_terminal()
                    && let Some(session) = &terminal.read(cx).session
                {
                    session.input(b"[Console]::WriteLine(('EXPLORER_START_' + $PWD.ProviderPath))\r".to_vec());
                }
            });
            let began = Instant::now();
            let mut reached_directory = false;
            while failure.is_none() && began.elapsed() < Duration::from_secs(10) {
                reached_directory = root.read_with(cx, |app, cx| app.active_terminal().and_then(|terminal| terminal.read(cx).session.as_ref().map(|session| session.plain_text().contains(&marker)))).unwrap_or(false);
                if reached_directory { break; }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
            if !reached_directory { failure.get_or_insert_with(|| format!("Shell did not start in requested directory: {}", directory.display())); }
            let _ = root.read_with(cx, |app, _| config::atomic_write(&app.dir.join("launch.json"), serde_json::to_string_pretty(&serde_json::json!({"passed": failure.is_none(), "directory": directory, "error": failure})).unwrap().as_bytes()));
        }
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
                        let cwd = directory(&mode).join("한글 작업 폴더");
                        std::fs::create_dir_all(&cwd).unwrap();
                        let command = format!("Set-Location -LiteralPath '{}'; Clear-Host; [Console]::WriteLine(('SMOKE_'+'한글_OK')); [Console]::WriteLine(([char]27).ToString()+'[32mANSI_OK'+([char]27).ToString()+'[0m')\r", cwd.to_string_lossy().replace('\'', "''"));
                        t.replace_text_in_range(None, &command, window, cx);
                    });
                });
                Ok(())
            });
            if let Err(e) = result.and_then(|r| r) { failure = Some(format!("{e:#}")); }
        }
        let began = Instant::now();
        let mut output_ok = false;
        while failure.is_none() && began.elapsed() < Duration::from_secs(20) {
            output_ok = root.read_with(cx, |app, cx| app.active_terminal().and_then(|t| t.read(cx).session.as_ref().map(|s| s.plain_text().contains("SMOKE_한글_OK") && s.working_directory() == directory(&mode).join("한글 작업 폴더")))).unwrap_or(false);
            if output_ok { break; }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
        if !output_ok && failure.is_none() { failure = Some("Unicode PTY output timed out".into()); }
        #[cfg(feature = "ui-tests")]
        if mode == "updates" && failure.is_none()
            && let Err(error) = updates::run(&root, handle, cx).await
        {
            failure = Some(format!("{error:#}"));
        }
        #[cfg(feature = "ui-tests")]
        if mode == "settings" && failure.is_none()
            && let Err(e) = settings_controls::run(&root, handle, cx).await
        {
            eprintln!("Settings controls failure: {e:#}");
            let _ = handle.update(cx, |_, window, cx| root.update(cx, |app, cx| app.activate(0, window, cx)));
            failure = Some(format!("{e:#}"));
        }
        #[cfg(feature = "ui-tests")]
        if mode == "shortcuts" && failure.is_none() {
            let result = handle.update(cx, |_, window, cx| shortcuts::run(&root, window, cx));
            if let Err(e) = result.and_then(|r| r) { failure = Some(format!("{e:#}")); }
        }
        #[cfg(all(feature = "ui-tests", windows))]
        if mode == "shortcuts" && failure.is_none()
            && let Err(e) = shortcuts::duplicate_tab(&root, handle, cx).await
        {
            failure = Some(format!("{e:#}"));
        }
        #[cfg(all(feature = "ui-tests", windows))]
        if mode == "keys" && failure.is_none()
            && let Err(e) = keys::run(&root, handle, cx).await
        {
            failure = Some(format!("{e:#}"));
        }
        #[cfg(feature = "ui-tests")]
        if mode == "tabs" && failure.is_none()
            && let Err(e) = tabs::rename(&root, handle, cx).await
        {
            failure = Some(format!("{e:#}"));
        }
        #[cfg(feature = "ui-tests")]
        if mode == "tabs" && failure.is_none() {
            let result = handle.update(cx, |_, window, cx| tabs::run(&root, window, cx));
            if let Err(e) = result.and_then(|r| r) { failure = Some(format!("{e:#}")); }
        }
        #[cfg(feature = "ui-tests")]
        if mode == "drop" && failure.is_none() {
            // The output marker precedes PowerShell's prompt/ReadLine setup.
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let result = handle.update(cx, |_, window, cx| file_drop::start(&root, window, cx));
            match result.and_then(|r| r) {
                Err(e) => failure = Some(format!("{e:#}")),
                Ok((path, expected)) => {
                    cx.background_executor().timer(Duration::from_millis(250)).await;
                    if path.exists() { failure = Some("File drop submitted the command before Enter".into()); }
                    let result = handle.update(cx, |_, window, cx| window.dispatch_keystroke(Keystroke::parse("enter").unwrap(), cx));
                    if let Err(e) = result { failure = Some(format!("{e:#}")); }
                    let began = Instant::now();
                    while failure.is_none() && began.elapsed() < Duration::from_secs(10) {
                        if std::fs::read_to_string(&path).is_ok_and(|s| s == expected) { break; }
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                    }
                    if failure.is_none() && !std::fs::read_to_string(&path).is_ok_and(|s| s == expected) {
                        let _ = root.read_with(cx, |app, cx| std::fs::write(app.dir.join("drop-terminal.txt"), app.active_terminal().unwrap().read(cx).session.as_ref().unwrap().plain_text()));
                        failure = Some(format!("Dropped file arguments did not reach the PTY unchanged: {:?}", std::fs::read_to_string(&path)));
                    }
                }
            }
        }
        #[cfg(feature = "ui-tests")]
        if mode == "scroll" && failure.is_none() {
            let result = handle.update(cx, |_, window, cx| scroll::run(&root, window, cx));
            match result.and_then(|r| r) {
                Err(e) => failure = Some(format!("{e:#}")),
                Ok(expected) => {
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                    let actual = root.read_with(cx, |app, cx| {
                        app.active_terminal().unwrap().read(cx).session.as_ref().unwrap().terminal.lock().grid().display_offset()
                    });
                    if actual != expected { failure = Some(format!("Scroll continued after input stopped: {expected} -> {actual}")); }
                }
            }
        }
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
                    "settings" => {
                        app.hide_search(window, cx);
                        app.settings_tab(window, cx);
                        app.draft.theme = "system".into();
                        app.save_settings(cx);
                        assert_eq!(Settings::load(&app.dir).unwrap().theme, "system");
                        app.reload_settings(cx);
                        assert_eq!(app.settings.theme, "system");
                        assert_eq!(app.draft.theme, "system");
                    }
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
                    "shortcuts" => {
                        app.hide_search(window, cx);
                        app.settings_tab(window, cx);
                        app.settings_page = super::shortcuts::SettingsPage::Shortcuts;
                    }
                    "confirm" => { app.hide_search(window, cx); app.should_close(window, cx); }
                    _ => app.hide_search(window, cx),
                }
                app.persist(window, cx);
                let cwd_saved = SavedSession::load(&app.dir).is_ok_and(|saved| saved.tabs.iter().any(|tab| tab.cwd == directory(&mode).join("한글 작업 폴더")));
                if !cwd_saved { failure.get_or_insert_with(|| "Changed working directory was not saved".into()); }
                app.status = failure.clone().map(|e| format!("SMOKE FAILURE: {e}"));
                let result = serde_json::json!({ "passed": failure.is_none() && output_ok && search_ok, "mode": mode, "unicode_pty": output_ok, "cwd_saved": cwd_saved, "search": search_ok, "tabs": app.tabs.len(), "error": failure });
                let _ = config::atomic_write(&app.dir.join("result.json"), serde_json::to_string_pretty(&result).unwrap().as_bytes());
                cx.notify();
            });
            Ok(())
        });
        if let Err(e) = result.and_then(|r| r) { eprintln!("SMOKE FAILURE: {e:#}"); }
        #[cfg(feature = "ui-tests")]
        if mode == "layout" {
            let result = handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
                let terminal = root.read(cx).active_terminal().unwrap();
                terminal.update(cx, |view, cx| { view.settings.font_size = 15.; cx.notify(); });
                let mut line_heights = Vec::new();
                for multiplier in [1.0, 1.6, 1.2] {
                    terminal.update(cx, |view, cx| { view.settings.line_height = multiplier; cx.notify(); });
                    window.refresh();
                    window.draw(cx).clear(cx);
                    line_heights.push(terminal.update(cx, |view, cx| view.layout_probe(window, cx))?);
                }
                config::atomic_write(&root.read(cx).dir.join("line-heights.json"), serde_json::to_string_pretty(&line_heights)?.as_bytes())?;
                let mut cell_widths = Vec::new();
                for multiplier in [0.3, 0.6, 1.5, 2.0, 0.6] {
                    terminal.update(cx, |view, cx| { view.settings.cell_width = multiplier; cx.notify(); });
                    window.refresh();
                    window.draw(cx).clear(cx);
                    cell_widths.push(terminal.update(cx, |view, cx| view.layout_probe(window, cx))?);
                }
                config::atomic_write(&root.read(cx).dir.join("cell-widths.json"), serde_json::to_string_pretty(&cell_widths)?.as_bytes())?;
                window.refresh();
                window.draw(cx).clear(cx);
                let metrics = terminal.update(cx, |view, cx| view.layout_probe(window, cx))?;
                config::atomic_write(&root.read(cx).dir.join("layout.json"), serde_json::to_string_pretty(&metrics)?.as_bytes())?;
                Ok(())
            });
            if let Err(e) = result.and_then(|r| r) {
                let _ = root.read_with(cx, |app, _| config::atomic_write(&app.dir.join("result.json"), serde_json::to_string_pretty(&serde_json::json!({"passed": false, "error": format!("{e:#}")})).unwrap().as_bytes()));
            }
        }
        #[cfg(feature = "ui-tests")]
        if mode == "links" {
            let result = handle.update(cx, |_, window, cx| -> anyhow::Result<()> {
                let terminal = root.read(cx).active_terminal().unwrap();
                let metrics = TerminalView::links_probe(&terminal, window, cx)?;
                config::atomic_write(&root.read(cx).dir.join("links.json"), serde_json::to_string_pretty(&metrics)?.as_bytes())?;
                Ok(())
            });
            if let Err(e) = result.and_then(|r| r) {
                let _ = root.read_with(cx, |app, _| config::atomic_write(&app.dir.join("result.json"), serde_json::to_string_pretty(&serde_json::json!({"passed": false, "error": format!("{e:#}")})).unwrap().as_bytes()));
            }
        }
        #[cfg(feature = "ui-tests")]
        {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let _ = handle.update(cx, |_, window, cx| {
                // Hidden test windows do not receive presentation ticks from DWM.
                if matches!(mode.as_str(), "settings" | "shortcuts") {
                    for theme in ["light", "dark", "system"] {
                        root.update(cx, |app, cx| {
                            app.draft.theme = theme.into();
                            app.save_settings(cx);
                            assert_eq!(Settings::load(&app.dir).unwrap().theme, theme);
                            app.status = None;
                        });
                        window.refresh();
                        window.draw(cx).clear(cx);
                        let path = root.read(cx).dir.join(format!("theme-{theme}.png"));
                        window.render_to_image().unwrap().save(path).unwrap();
                    }
                }
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

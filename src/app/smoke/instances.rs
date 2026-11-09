//! Real subprocess launches use the production pipe and the production window registry.
use super::*;
use crate::config::WindowingBehavior;
use anyhow::{Result, ensure};
use std::path::Path;

async fn launch(cx: &mut AsyncApp, directory: Option<&Path>, new_window: bool) -> Result<()> {
    let mut command = platform::quiet_command(std::env::current_exe()?);
    command.arg("--smoke-test=instances");
    if let Some(directory) = directory {
        command.arg("-d").arg(directory);
    }
    if new_window {
        command.arg("--new-window");
    }
    let mut child = command.spawn()?;
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "launch client failed: {status}");
            return Ok(());
        }
        if began.elapsed() > Duration::from_secs(15) {
            child.kill()?;
            anyhow::bail!("launch client did not exit after forwarding");
        }
        cx.background_executor()
            .timer(Duration::from_millis(30))
            .await;
    }
}

fn entries(cx: &AsyncApp) -> Vec<windows::Entry> {
    cx.update(|cx| cx.global::<windows::Windows>().entries.clone())
}
fn tabs(entry: &windows::Entry, cx: &AsyncApp) -> usize {
    entry
        .root
        .upgrade()
        .unwrap()
        .read_with(cx, |app, _| app.tabs.len())
}
fn mode(entry: &windows::Entry, mode: WindowingBehavior, cx: &mut AsyncApp) -> Result<()> {
    entry.handle.update(cx, |app, _, cx| {
        app.draft.windowing_behavior = mode;
        app.save_settings(cx);
        ensure!(
            Settings::load(&app.dir)?.windowing_behavior == mode,
            "mode not saved"
        );
        Ok(())
    })?
}
fn activate(entry: &windows::Entry, cx: &mut AsyncApp) -> Result<()> {
    entry.handle.update(cx, |_, window, cx| {
        window.activate_window();
        windows::activated(window, cx);
    })?;
    Ok(())
}

async fn exercise(cx: &mut AsyncApp, dir: &Path) -> Result<()> {
    let first = entries(cx)[0].clone();
    first.handle.update(cx, |_, window, _| {
        ensure!(
            windows::current_desktop(windows::native_handle(window)),
            "virtual desktop lookup failed"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(
        cx.update(|cx| cx.global::<windows::SharedSettings>().0.windowing_behavior)
            == WindowingBehavior::CurrentDesktop,
        "wrong default"
    );
    launch(cx, None, false).await?;
    ensure!(
        entries(cx).len() == 1 && tabs(&first, cx) == 2,
        "default launch did not attach"
    );
    let cwd = dir.join("한글 폴더 it's $value & [1]");
    std::fs::create_dir_all(&cwd)?;
    launch(cx, Some(&cwd), false).await?;
    ensure!(tabs(&first, cx) == 3, "Explorer launch did not attach");
    let actual = first.root.upgrade().unwrap().read_with(cx, |app, cx| {
        app.active_terminal().unwrap().read(cx).working_directory()
    });
    ensure!(actual == cwd, "Explorer folder was changed: {actual:?}");
    launch(cx, None, true).await?;
    ensure!(
        entries(cx).len() == 2,
        "--new-window did not create a window"
    );
    let second = entries(cx)[1].clone();
    mode(&first, WindowingBehavior::NewWindow, cx)?;
    launch(cx, None, false).await?;
    ensure!(entries(cx).len() == 3, "useNew did not create a window");
    mode(&first, WindowingBehavior::AnyDesktop, cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(50))
        .await;
    ensure!(
        second
            .root
            .upgrade()
            .unwrap()
            .read_with(cx, |app, _| app.settings.windowing_behavior)
            == WindowingBehavior::AnyDesktop,
        "settings did not propagate"
    );
    activate(&second, cx)?;
    let before = tabs(&second, cx);
    launch(cx, None, false).await?;
    ensure!(
        entries(cx).len() == 3 && tabs(&second, cx) == before + 1,
        "MRU routing failed"
    );
    second
        .handle
        .update(cx, |_, window, _| window.minimize_window())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    // Minimizing may activate another Edolterm window. Select the minimized
    // target deterministically to exercise restoration independently of DWM.
    second
        .handle
        .update(cx, |_, window, cx| windows::activated(window, cx))?;
    launch(cx, None, false).await?;
    second.handle.update(cx, |_, window, _| {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;
        ensure!(
            unsafe { IsIconic(windows::native_handle(window) as _) } == 0,
            "target remained minimized"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    mode(&first, WindowingBehavior::CurrentDesktop, cx)?;
    activate(&first, cx)?;
    launch(cx, None, false).await?;
    ensure!(tabs(&first, cx) == 4, "current desktop MRU failed");
    // Re-delivery of a request cannot create duplicate tabs.
    let request = crate::instance::Request::new(Some(cwd), false)?;
    cx.update(|cx| windows::route(&request, cx))
        .map_err(anyhow::Error::msg)?;
    cx.update(|cx| windows::route(&request, cx))
        .map_err(anyhow::Error::msg)?;
    ensure!(tabs(&first, cx) == 5, "duplicate request was applied twice");
    first.handle.update(cx, |app, window, cx| {
        ensure!(app.persist(window, cx), "multi-window save failed");
        let saved = SavedSession::load(&app.dir)?;
        ensure!(
            saved.windows.len() == 3 && saved.active_window == 0,
            "incomplete window snapshot"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    AnyWindowHandle::from(first.handle).update(cx, |_, window, cx| {
        window.dispatch_keystroke(Keystroke::parse("ctrl-shift-n")?, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    ensure!(entries(cx).len() == 4, "new window shortcut failed");
    first
        .handle
        .update(cx, |app, window, cx| app.close_window(window, cx))?;
    let began = Instant::now();
    while entries(cx).len() != 3 && began.elapsed() < Duration::from_secs(5) {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    ensure!(
        entries(cx).len() == 3,
        "closing one window affected other windows"
    );
    activate(&second, cx)?;
    let before = tabs(&second, cx);
    launch(cx, None, false).await?;
    ensure!(
        tabs(&second, cx) == before + 1,
        "host stopped after first window closed"
    );
    // Simultaneous real launch processes must serialize into the same GUI host.
    let mut clients = Vec::new();
    for _ in 0..4 {
        clients.push(
            platform::quiet_command(std::env::current_exe()?)
                .arg("--smoke-test=instances")
                .spawn()?,
        );
    }
    let began = Instant::now();
    while !clients.is_empty() {
        for index in (0..clients.len()).rev() {
            if let Some(status) = clients[index].try_wait()? {
                ensure!(status.success(), "concurrent launch failed");
                clients.swap_remove(index).wait()?;
            }
        }
        if began.elapsed() > Duration::from_secs(15) {
            for child in &mut clients {
                let _ = child.kill();
            }
            anyhow::bail!("concurrent launch timed out");
        }
        cx.background_executor()
            .timer(Duration::from_millis(30))
            .await;
    }
    ensure!(
        entries(cx).len() == 3 && tabs(&second, cx) == before + 5,
        "concurrent clients did not attach exactly once"
    );
    second.handle.update(cx, |app, window, cx| {
        ensure!(app.persist(window, cx), "save failed");
        app.settings_tab(window, cx);
        Ok::<_, anyhow::Error>(())
    })??;
    let saved = SavedSession::load(dir)?;
    let snapshot = saved.windows[saved.active_window].clone();
    let expected = snapshot.tabs.len();
    let expected_tabs = snapshot.tabs.clone();
    let expected_active = snapshot.active;
    let restored =
        cx.update(|cx| windows::open(Startup::Restore(snapshot.clone()), snapshot, None, cx))?;
    ensure!(
        restored.read_with(cx, |app, _| app.tabs.len()) == expected,
        "multi-window tab restoration failed"
    );
    restored.read_with(cx, |app, cx| {
        ensure!(
            app.active == expected_active,
            "restore changed the selected tab"
        );
        ensure!(
            app.tabs
                .iter()
                .all(|tab| tab.terminal.as_ref().unwrap().read(cx).session.is_none()),
            "restoration blocked on shell creation"
        );
        let selected = app.tabs[app.active]
            .terminal
            .as_ref()
            .unwrap()
            .read(cx)
            .startup_sequence()
            .unwrap();
        for (index, tab) in app.tabs.iter().enumerate() {
            if index != app.active {
                ensure!(
                    tab.terminal
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .startup_sequence()
                        .unwrap()
                        > selected,
                    "a background tab was enqueued before the selected tab"
                );
            }
        }
        let saved = app.snapshot(app.last_bounds, cx);
        ensure!(
            saved.tabs.len() == expected_tabs.len(),
            "pending shells vanished from session snapshot"
        );
        for (actual, expected) in saved.tabs.iter().zip(&expected_tabs) {
            ensure!(
                actual.profile_id == expected.profile_id && actual.cwd == expected.cwd,
                "pending restore changed tab order or working directory"
            );
        }
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

pub(super) fn start(root: Entity<TerminalApp>, cx: &mut App) {
    let dir = root.read(cx).dir.clone();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(700)).await;
        let result = exercise(cx, &dir).await;
        let error = result.err().map(|error| format!("{error:#}"));
        let report = serde_json::json!({"passed": error.is_none(), "mode": "instances", "error": error, "gui_pid": std::process::id(), "windows": entries(cx).len()});
        let _ = config::atomic_write(&dir.join("result.json"), serde_json::to_string_pretty(&report).unwrap().as_bytes());
        cx.background_executor().timer(Duration::from_millis(300)).await;
        if let Some(entry) = entries(cx).first() {
            let _ = AnyWindowHandle::from(entry.handle).update(cx, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                if let Err(error) = window.render_to_image().and_then(|image| image.save(dir.join("screenshot.png")).map_err(Into::into)) { eprintln!("Capture failed: {error:#}"); }
            });
        }
        while !dir.join("stop").exists() {
            cx.background_executor().timer(Duration::from_millis(150)).await;
        }
        // Exercise updater-style shutdown: a complete snapshot survives all closes.
        cx.update(|cx| cx.global_mut::<windows::Windows>().preserve_session = true);
        let before = std::fs::read(dir.join("session.json")).ok();
        for entry in entries(cx) {
            let _ = entry.handle.update(cx, |app, window, cx| app.close_window(window, cx));
        }
        assert_eq!(before, std::fs::read(dir.join("session.json")).ok());
    }).detach();
}

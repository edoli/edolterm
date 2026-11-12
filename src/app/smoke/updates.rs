//! Exercise the production scheduler and badges without contacting GitHub.
use super::*;
use anyhow::{Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub(super) fn preview(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> Result<()> {
    let state = crate::update::State::Available(crate::update::Release {
        version: "99.0.0".into(),
        size: 1024,
        sha256: "ab".repeat(32),
        asset_name: "edolterm-windows.zip".into(),
    });
    cx.set_global(super::super::updates::Checks {
        state: state.clone(),
        started: true,
        preview: true,
    });
    root.update(cx, |app, cx| {
        let active = app.active;
        app.update_state = state;
        app.settings.confirm_close = false;
        app.draft.confirm_close = false;
        app.tabs[active].custom_title = Some("업데이트 UI 미리보기".into());
        app.settings_page = SettingsPage::Behavior;
        app.settings_tab(window, cx);
        app.activate(active, window, cx);
        app.menu = true;
        app.status = Some("업데이트 알림 미리보기 · 99.0.0은 표시 확인용입니다. 실제 다운로드·설치는 실행되지 않습니다.".into());
        cx.notify();
    });
    window.refresh();
    window.draw(cx).clear(cx);
    let dir = root.read(cx).dir.clone();
    window.render_to_image()?.save(dir.join("preview.png"))?;
    config::atomic_write(
        &dir.join("preview-ready.json"),
        &serde_json::to_vec(&serde_json::json!({
            "pid": std::process::id(), "update_available": root.read(cx).has_update(cx),
            "preview": true,
        }))?,
    )?;
    Ok(())
}

pub(super) async fn run(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    let active = root.read_with(cx, |app, _| app.active);
    let calls = Arc::new(AtomicUsize::new(0));
    let began = Instant::now();
    handle.update(cx, |_, window, cx| -> Result<()> {
        ensure!(
            matches!(root.read(cx).update_state, crate::update::State::Unchecked),
            "automatic check ran before the startup gate"
        );
        // Schedule twice to exercise deduplication across startup callers.
        for _ in 0..2 {
            let calls = calls.clone();
            super::super::updates::schedule_with(root.downgrade(), window, move |cx| {
                super::super::updates::check_with(cx, move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(crate::update::State::Available(crate::update::Release {
                        version: "99.0.0".into(),
                        size: 1024,
                        sha256: "ab".repeat(32),
                        asset_name: "edolterm-windows.zip".into(),
                    }))
                })
            });
        }
        ensure!(
            calls.load(Ordering::SeqCst) == 0,
            "release lookup ran synchronously"
        );
        window.simulate_next_frame(cx);
        window.refresh();
        window.draw(cx).clear(cx);
        window.simulate_next_frame(cx);
        ensure!(
            calls.load(Ordering::SeqCst) == 0,
            "release lookup ran before shell readiness and grace"
        );
        Ok(())
    })??;
    loop {
        if root.read_with(cx, |app, _| {
            matches!(app.update_state, crate::update::State::Available(_))
        }) {
            break;
        }
        ensure!(
            began.elapsed() < Duration::from_secs(20),
            "deferred release check did not complete"
        );
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    ensure!(
        began.elapsed() >= Duration::from_secs(2),
        "release lookup bypassed the post-startup quiet period"
    );
    ensure!(
        calls.load(Ordering::SeqCst) == 1,
        "automatic release lookup was duplicated"
    );

    let second = cx.update(windows::open_default)?;
    let entry = cx.update(|cx| {
        cx.global::<windows::Windows>()
            .entries
            .last()
            .unwrap()
            .clone()
    });
    let inherited = second.read_with(cx, |app, cx| app.has_update(cx));
    entry
        .handle
        .update(cx, |app, window, cx| app.close_window(window, cx))?;
    ensure!(inherited, "new window did not inherit the release result");

    handle.update(cx, |_, window, cx| -> Result<()> {
        ensure!(
            root.read(cx).has_update(cx),
            "release result did not enable notification markers"
        );
        let dir = root.read(cx).dir.clone();
        root.update(cx, |app, cx| {
            app.menu = true;
            cx.notify();
        });
        window.refresh();
        window.draw(cx).clear(cx);
        window
            .render_to_image()?
            .save(dir.join("update-menu.png"))?;
        root.update(cx, |app, cx| {
            app.settings_page = SettingsPage::Behavior;
            app.settings_tab(window, cx);
        });
        window.refresh();
        window.draw(cx).clear(cx);
        window
            .render_to_image()?
            .save(dir.join("update-settings.png"))?;
        for state in [
            crate::update::State::Current,
            crate::update::State::NoRelease,
            crate::update::State::Failed("offline".into()),
        ] {
            cx.set_global(super::super::updates::Checks {
                state,
                ..Default::default()
            });
            // Flush observers before inspecting the window's copy.
            window.refresh();
            window.draw(cx).clear(cx);
        }
        Ok(())
    })??;
    // Global observers settle at the enclosing update boundary.
    handle.update(cx, |_, window, cx| -> Result<()> {
        ensure!(
            !root.read(cx).has_update(cx),
            "failed/no-update result retained a notification marker"
        );
        root.update(cx, |app, cx| app.activate(active, window, cx));
        Ok(())
    })??;
    Ok(())
}

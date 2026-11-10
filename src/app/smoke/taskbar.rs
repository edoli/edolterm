//! Exercise the production startup gate without publishing a real Jump List.
use super::*;
use anyhow::{Result, ensure};
use std::{cell::Cell, rc::Rc};

pub(super) type Probe = Rc<Cell<bool>>;

pub(super) fn probe(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> Probe {
    let completed = Rc::new(Cell::new(false));
    let observed = completed.clone();
    let weak = root.downgrade();
    let began = Instant::now();
    super::super::taskbar::after_startup(weak.clone(), window, move |cx| {
        assert!(began.elapsed() >= super::super::taskbar::STARTUP_GRACE);
        let root = weak.upgrade().expect("test window still exists");
        let terminal = root.read(cx).active_terminal().expect("active shell");
        let terminal = terminal.read(cx);
        assert!(terminal.session.is_some() || terminal.error.is_some() || terminal.exited);
        observed.set(true);
    });
    assert!(
        !completed.get(),
        "Shell work ran synchronously during startup"
    );
    // A notification during startup must not bypass the first-frame/shell gate.
    root.update(cx, |_, cx| cx.notify());
    assert!(!completed.get());
    // The smoke runner launches hidden windows, without native frame delivery.
    // Advance the actual GPUI callbacks around a real GPU draw explicitly.
    window.simulate_next_frame(cx);
    assert!(!completed.get(), "Shell work ran before the first draw");
    window.refresh();
    window.draw(cx).clear(cx);
    window.simulate_next_frame(cx);
    assert!(
        !completed.get(),
        "Shell work bypassed the startup grace period"
    );
    completed
}

async fn wait(probe: &Probe, cx: &mut AsyncApp) -> Result<()> {
    let began = Instant::now();
    while !probe.get() && began.elapsed() < Duration::from_secs(10) {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    ensure!(probe.get(), "deferred Shell work never ran");
    Ok(())
}

pub(super) async fn verify(probe: Probe, cx: &mut AsyncApp) -> Result<()> {
    wait(&probe, cx).await?;
    // Failed shell creation must also release the gate, instead of leaving
    // the Jump List stale forever. This window uses an intentionally missing exe.
    let failed = cx.update(|cx| {
        windows::open(
            Startup::Launch {
                profile: Some(Profile::new(
                    "broken",
                    "Missing shell",
                    "__edolterm_missing_shell__.exe",
                    &[],
                )),
                directory: None,
            },
            SavedSession::default(),
            None,
            cx,
        )
    })?;
    let entry = cx.update(|cx| {
        cx.global::<windows::Windows>()
            .entries
            .last()
            .unwrap()
            .clone()
    });
    let failed_probe = AnyWindowHandle::from(entry.handle)
        .update(cx, |_, window, cx| self::probe(&failed, window, cx))?;
    let result = wait(&failed_probe, cx).await;
    failed.read_with(cx, |app, cx| {
        assert!(app.active_terminal().unwrap().read(cx).error.is_some());
    });
    entry
        .handle
        .update(cx, |app, window, cx| app.close_window(window, cx))?;
    result
}

#[cfg(windows)]
pub(super) async fn profile_icons(root: &Entity<TerminalApp>, cx: &mut AsyncApp) -> Result<()> {
    let (profiles, dir) = root.read_with(cx, |app, _| (app.profiles.clone(), app.dir.clone()));
    let images = crate::taskbar::load_icons(&profiles, &dir);
    cx.update(|cx| icons::profiles::receive(images, cx));
    let began = Instant::now();
    loop {
        let loaded = cx.update(|cx| {
            profiles
                .iter()
                .filter(|profile| matches!(profile.id.as_str(), "pwsh" | "powershell" | "cmd"))
                .all(|profile| icons::profiles::get(&profile.program, cx).is_some())
        });
        if loaded {
            break;
        }
        ensure!(
            began.elapsed() < Duration::from_secs(5),
            "native profile images did not reach the UI catalog"
        );
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    cx.update(|cx| {
        let report: Vec<_> = profiles
            .iter()
            .map(|profile| {
                serde_json::json!({
                    "profile": profile.id,
                    "image": icons::profiles::get(&profile.program, cx).map(|image| image.id.0),
                })
            })
            .collect();
        config::atomic_write(
            &dir.join("profile-icons.json"),
            &serde_json::to_vec_pretty(&report).unwrap(),
        )
    })?;
    Ok(())
}

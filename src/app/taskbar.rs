//! Keep Shell integration out of the first frame and foreground shell startup.
use super::*;

const STARTUP_GRACE: Duration = Duration::from_millis(100);

pub(super) fn start(root: WeakEntity<TerminalApp>, window: &Window) {
    after_startup(root, window, |cx| {
        // Read the current settings here: edits made during startup must not be
        // overwritten by the snapshot that was loaded before opening windows.
        update(cx);
        cx.observe_global::<windows::SharedSettings>(update)
            .detach();
    });
}

fn update(cx: &mut App) {
    let registry = cx.global::<windows::Windows>();
    if registry.preserve_session || registry.entries.is_empty() {
        return;
    }
    let profiles = platform::merge_profiles(
        registry.detected_profiles().to_vec(),
        &cx.global::<windows::SharedSettings>().0.profiles,
    );
    crate::taskbar::update(&profiles, &registry.dir);
}

pub(super) fn after_startup(
    root: WeakEntity<TerminalApp>,
    window: &Window,
    ready: impl FnOnce(&mut App) + 'static,
) {
    // GPUI runs next-frame callbacks before drawing that frame. Nest them to
    // ensure at least one frame was presented before considering Shell work.
    window.on_next_frame(move |window, _| {
        window.on_next_frame(move |_, cx| {
            cx.spawn(async move |cx| {
                loop {
                    let starting = cx.update(|cx| {
                        root.upgrade().is_some_and(|root| {
                            let app = root.read(cx);
                            !app.closing
                                && app.active_terminal().is_some_and(|terminal| {
                                    let terminal = terminal.read(cx);
                                    terminal.session.is_none()
                                        && terminal.error.is_none()
                                        && !terminal.exited
                                })
                        })
                    });
                    if !starting {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
                // Leave breathing room for the shell's first output and prompt.
                // No jump-list worker thread or icon I/O exists before this.
                cx.background_executor().timer(STARTUP_GRACE).await;
                cx.update(ready);
            })
            .detach();
        });
    });
}

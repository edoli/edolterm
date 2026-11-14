//! Capture the actual window at native GPU resolution, without desktop scaling.
use super::*;

pub(super) fn start(root: Entity<TerminalApp>, window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let began = Instant::now();
        let ready = loop {
            let ready = root.read_with(cx, |app, cx| {
                app.dir.join("ready").exists()
                    && app.active_terminal().is_some_and(|terminal| {
                        terminal.read(cx).session.as_ref().is_some_and(|session| {
                            session
                                .plain_text()
                                .contains("https://github.com/edoli/edolterm")
                        })
                    })
            });
            if ready || began.elapsed() > Duration::from_secs(30) {
                break ready;
            }
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        };
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        let _ = handle.update(cx, |_, window, cx| {
            let result = (|| -> anyhow::Result<serde_json::Value> {
                anyhow::ensure!(ready, "README terminal scene did not become ready");
                let app = root.read(cx);
                anyhow::ensure!(app.tabs.len() == 3, "Expected three terminal tabs");
                anyhow::ensure!(
                    app.tabs.iter().all(|tab| tab.color.is_none()),
                    "README tabs must use their default colors"
                );
                let dir = app.dir.clone();
                window.refresh();
                window.draw(cx).clear(cx);
                let image = window.render_to_image()?;
                anyhow::ensure!(
                    image.width() >= 1920,
                    "Capture must be at least 1920px wide"
                );
                let metadata = serde_json::json!({
                    "passed": true,
                    "width": image.width(),
                    "height": image.height(),
                    "scale_factor": window.scale_factor(),
                    "default_tab_colors": true,
                    "color_scheme": root.read(cx).settings.color_scheme,
                });
                image.save(dir.join("screenshot.png"))?;
                Ok(metadata)
            })();
            let result = result.unwrap_or_else(
                |error| serde_json::json!({"passed": false, "error": format!("{error:#}")}),
            );
            let dir = root.read(cx).dir.clone();
            let _ = config::atomic_write(
                &dir.join("result.json"),
                serde_json::to_string_pretty(&result).unwrap().as_bytes(),
            );
            root.update(cx, |app, cx| app.close_window(window, cx));
        });
    })
    .detach();
}

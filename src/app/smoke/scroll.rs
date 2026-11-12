use super::*;
use alacritty_terminal::{grid::Scroll, vte::ansi::Processor};

pub(super) fn run(
    root: &Entity<TerminalApp>,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<usize> {
    let terminal = root.read(cx).active_terminal().unwrap();
    terminal.update(cx, |view, cx| {
        let mut term = view.session.as_ref().unwrap().terminal.lock();
        let mut parser: Processor = Processor::new();
        for line in 0..2000 {
            parser.advance(&mut *term, format!("\r\n{line:04} abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 한글 테스트").as_bytes());
        }
        term.scroll_display(Scroll::Bottom);
        cx.notify();
    });
    window.refresh();
    window.draw(cx).clear(cx);
    zoom(root, window, cx)?;
    let event = ScrollEvent {
        position: point(px(200.), px(200.)),
        delta: ScrollDelta::Lines(point(0., 3.)),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    };
    let offset = |cx: &App| {
        terminal
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .terminal
            .lock()
            .grid()
            .display_offset()
    };
    window.dispatch_event(PlatformInput::Scroll(event.clone()), cx);
    let single_offset = offset(cx);
    let began = Instant::now();
    for _ in 0..120 {
        window.dispatch_event(PlatformInput::Scroll(event.clone()), cx);
    }
    let burst_ms = began.elapsed().as_secs_f64() * 1000.;
    let burst_offset = offset(cx);
    let began = Instant::now();
    for _ in 0..60 {
        window.dispatch_event(PlatformInput::Scroll(event.clone()), cx);
        window.draw(cx).clear(cx);
    }
    let frame_ms = began.elapsed().as_secs_f64() * 1000. / 60.;
    let final_offset = offset(cx);
    let mut reverse = event;
    reverse.delta = ScrollDelta::Lines(point(0., -3.));
    window.dispatch_event(PlatformInput::Scroll(reverse), cx);
    let reverse_offset = offset(cx);
    config::atomic_write(
        &root.read(cx).dir.join("scroll.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "single_offset": single_offset,
            "burst_offset": burst_offset,
            "burst_120_events_ms": burst_ms,
            "average_frame_ms": frame_ms,
            "final_offset": final_offset,
            "reverse_offset": reverse_offset,
        }))?
        .as_bytes(),
    )?;
    anyhow::ensure!(
        single_offset == 3,
        "wheel distance multiplied: {single_offset}"
    );
    anyhow::ensure!(
        burst_offset == 363,
        "burst input dropped or deferred: {burst_offset}"
    );
    anyhow::ensure!(
        final_offset == 543,
        "scroll advanced while drawing: {final_offset}"
    );
    anyhow::ensure!(
        reverse_offset == 540,
        "direction reversal delayed: {reverse_offset}"
    );
    Ok(reverse_offset)
}

fn zoom(root: &Entity<TerminalApp>, window: &mut Window, cx: &mut App) -> anyhow::Result<()> {
    let terminal = root.read(cx).active_terminal().unwrap();
    let original = terminal.read(cx).settings.clone();
    let original_cols = terminal.read(cx).session.as_ref().unwrap().size.cols;
    let mut event = ScrollEvent {
        position: point(px(200.), px(200.)),
        delta: ScrollDelta::Lines(point(0., 3.)),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    };
    terminal.update(cx, |view, _| view.settings.ctrl_scroll_zoom = true);
    window.dispatch_event(PlatformInput::Scroll(event.clone()), cx);
    anyhow::ensure!(
        terminal.read(cx).settings.font_size == original.font_size + 1.,
        "Ctrl-wheel did not zoom in one step"
    );
    anyhow::ensure!(
        root.read(cx).settings.font_size == original.font_size,
        "zoom changed saved font size"
    );
    window.refresh();
    window.draw(cx).clear(cx);
    anyhow::ensure!(
        terminal.read(cx).session.as_ref().unwrap().size.cols < original_cols,
        "zoom did not resize terminal grid"
    );
    event.delta = ScrollDelta::Lines(point(0., -3.));
    window.dispatch_event(PlatformInput::Scroll(event.clone()), cx);
    anyhow::ensure!(
        terminal.read(cx).settings.font_size == original.font_size,
        "Ctrl-wheel did not zoom out"
    );
    for (size, delta, expected) in [
        (48., 3., 49.),
        (127.5, 3., 128.),
        (128., 3., 128.),
        (128., -3., 127.),
        (8.5, -3., 8.),
        (8., -3., 8.),
    ] {
        terminal.update(cx, |view, _| view.settings.font_size = size);
        event.delta = ScrollDelta::Lines(point(0., delta));
        window.dispatch_event(PlatformInput::Scroll(event.clone()), cx);
        anyhow::ensure!(
            terminal.read(cx).settings.font_size == expected,
            "Ctrl-wheel exceeded font size bounds"
        );
    }
    terminal.update(cx, |view, cx| view.apply_settings(&original, cx));
    window.refresh();
    window.draw(cx).clear(cx);
    terminal.update(cx, |view, _| {
        view.settings.ctrl_scroll_zoom = false;
        view.session
            .as_ref()
            .unwrap()
            .terminal
            .lock()
            .scroll_display(Scroll::Bottom);
    });
    event.delta = ScrollDelta::Lines(point(0., 3.));
    window.dispatch_event(PlatformInput::Scroll(event), cx);
    anyhow::ensure!(
        terminal.read(cx).settings.font_size == original.font_size,
        "disabled Ctrl-wheel changed font size"
    );
    anyhow::ensure!(
        terminal
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .terminal
            .lock()
            .grid()
            .display_offset()
            == 3,
        "disabled Ctrl-wheel lost normal scroll"
    );
    terminal.update(cx, |view, cx| {
        view.apply_settings(&original, cx);
        view.session
            .as_ref()
            .unwrap()
            .terminal
            .lock()
            .scroll_display(Scroll::Bottom);
    });
    window.refresh();
    window.draw(cx).clear(cx);
    Ok(())
}

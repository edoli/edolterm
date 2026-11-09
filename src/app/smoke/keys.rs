//! Check Enter chords through GPUI's dispatch tree and the real PowerShell PTY.
use super::*;
use anyhow::{Result, ensure};

fn output(root: &Entity<TerminalApp>, cx: &AsyncApp) -> String {
    root.read_with(cx, |app, cx| {
        app.active_terminal()
            .unwrap()
            .read(cx)
            .session
            .as_ref()
            .unwrap()
            .plain_text()
    })
}

async fn wait_for(root: &Entity<TerminalApp>, marker: &str, cx: &mut AsyncApp) -> Result<()> {
    let began = Instant::now();
    loop {
        let text = output(root, cx);
        if text.contains(marker) {
            return Ok(());
        }
        ensure!(
            began.elapsed() < Duration::from_secs(10),
            "Waiting for {marker}: {text}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(30))
            .await;
    }
}

fn press(handle: AnyWindowHandle, name: &'static str, cx: &mut AsyncApp) -> Result<()> {
    let key = Keystroke::parse(name)?;
    handle.update(cx, |_, window, cx| {
        window.dispatch_keystroke(key.clone(), cx);
        window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke: key }), cx);
    })?;
    Ok(())
}

pub(super) async fn run(
    root: &Entity<TerminalApp>,
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
) -> Result<()> {
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    handle.update(cx, |_, window, cx| {
        let terminal = root.read(cx).active_terminal().unwrap();
        let focus = terminal.read(cx).focus.clone();
        focus.focus(window, cx);
        terminal.read(cx).session.as_ref().unwrap().input(b"[Console]::WriteLine(('KEYS_'+'READY')); for ($i = 0; $i -lt 4; $i++) { $k = [Console]::ReadKey($true); [Console]::WriteLine(('UIKEY_{0}:{1}:{2}:{3}' -f $i, [int]$k.Key, [int]$k.KeyChar, [int]$k.Modifiers)) }; [Console]::WriteLine(('KEYS_'+'DONE'))\r".to_vec());
    })?;
    wait_for(root, "KEYS_READY", cx).await?;
    for (i, (name, character, modifiers)) in [
        ("enter", 13, 0),
        ("shift-enter", 13, 2),
        ("ctrl-enter", 10, 4),
        ("ctrl-shift-enter", 10, 6),
    ]
    .into_iter()
    .enumerate()
    {
        press(handle, name, cx)?;
        wait_for(root, &format!("UIKEY_{i}:13:{character}:{modifiers}"), cx).await?;
    }
    wait_for(root, "KEYS_DONE", cx).await?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    handle.update(cx, |_, window, cx| {
        root.read(cx)
            .active_terminal()
            .unwrap()
            .update(cx, |view, cx| {
                view.replace_text_in_range(
                    None,
                    "[Console]::WriteLine(('FIRST_'+'LINE'))",
                    window,
                    cx,
                );
            });
    })?;
    press(handle, "shift-enter", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    ensure!(
        !output(root, cx).contains("FIRST_LINE"),
        "Shift+Enter submitted the command"
    );
    handle.update(cx, |_, window, cx| {
        root.read(cx)
            .active_terminal()
            .unwrap()
            .update(cx, |view, cx| {
                view.replace_text_in_range(
                    None,
                    "[Console]::WriteLine(('SECOND_'+'LINE'))",
                    window,
                    cx,
                );
            });
    })?;
    press(handle, "enter", cx)?;
    wait_for(root, "FIRST_LINE", cx).await?;
    wait_for(root, "SECOND_LINE", cx).await?;
    Ok(())
}

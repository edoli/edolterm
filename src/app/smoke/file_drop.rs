use super::*;

/// Exercise the same platform events used by Explorer drops, through GPUI and
/// the actual PowerShell PTY. No OS input or user's shell is touched.
pub(super) fn start(
    root: &Entity<TerminalApp>,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<(PathBuf, String)> {
    let dir = root.read(cx).dir.clone();
    let output = dir.join("dropped-args.txt");
    if output.exists() {
        std::fs::remove_file(&output)?;
    }
    let paths = [
        dir.join("한글 폴더").join("it's $value; & [1].png"),
        dir.join("second file.txt"),
    ];
    std::fs::create_dir_all(paths[0].parent().unwrap())?;
    for path in &paths {
        std::fs::write(path, b"test attachment")?;
    }
    let terminal = root.read(cx).active_terminal().unwrap();
    terminal.update(cx, |view, cx| {
        view.focus.focus(window, cx);
        let command = format!(
            "& {{ [IO.File]::WriteAllText('{}', [string]::Join(\"`n\", [string[]]$args)) }} ",
            output.to_string_lossy().replace('\'', "''")
        );
        view.replace_text_in_range(None, &command, window, cx);
    });
    window.refresh();
    window.draw(cx).clear(cx);
    let position = point(px(200.), px(200.));
    window.dispatch_event(
        PlatformInput::FileDrop(FileDropEvent::Entered {
            position,
            paths: ExternalPaths(paths.iter().cloned().collect()),
        }),
        cx,
    );
    window.draw(cx).clear(cx);
    window.dispatch_event(
        PlatformInput::FileDrop(FileDropEvent::Pending { position }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::FileDrop(FileDropEvent::Submit { position }),
        cx,
    );
    anyhow::ensure!(
        terminal.read(cx).focus.is_focused(window),
        "drop did not focus terminal"
    );
    let expected = paths
        .iter()
        .map(|p| p.to_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    Ok((output, expected))
}

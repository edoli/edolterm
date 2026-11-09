use alacritty_terminal::term::TermMode;
use gpui::Keystroke;

/// Return None for committed text: GPUI's input handler owns Unicode/IME input.
pub fn encode(key: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let m = key.modifiers;
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let csi = |suffix: char| {
        if modifier == 1 {
            format!(
                "\x1b{}{}",
                if mode.contains(TermMode::APP_CURSOR) {
                    'O'
                } else {
                    '['
                },
                suffix
            )
        } else {
            format!("\x1b[1;{modifier}{suffix}")
        }
    };
    let tilde = |n: u8| {
        if modifier == 1 {
            format!("\x1b[{n}~")
        } else {
            format!("\x1b[{n};{modifier}~")
        }
    };
    let text = match key.key.as_str() {
        "enter" => {
            if m.control {
                "\n".into()
            } else if mode.contains(TermMode::LINE_FEED_NEW_LINE) {
                "\r\n".into()
            } else {
                "\r".into()
            }
        }
        "tab" if m.shift => "\x1b[Z".into(),
        "tab" => "\t".into(),
        "escape" => "\x1b".into(),
        "backspace" => {
            if m.control {
                "\x08".into()
            } else {
                "\x7f".into()
            }
        }
        "up" => csi('A'),
        "down" => csi('B'),
        "right" => csi('C'),
        "left" => csi('D'),
        "home" => csi('H'),
        "end" => csi('F'),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" | "f2" | "f3" | "f4" => {
            let suffix = match key.key.as_str() {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                _ => 'S',
            };
            if modifier == 1 {
                format!("\x1bO{suffix}")
            } else {
                format!("\x1b[1;{modifier}{suffix}")
            }
        }
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        _ if m.control && !m.platform => {
            let ch = match key.key.as_str() {
                "space" | "@" | "2" => 0,
                "[" | "3" => 27,
                "\\" | "4" => 28,
                "]" | "5" => 29,
                "^" | "6" => 30,
                "_" | "-" | "7" => 31,
                "?" | "8" => 127,
                s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphabetic() => {
                    s.as_bytes()[0].to_ascii_uppercase() - b'@'
                }
                _ => return None,
            };
            let mut bytes = vec![ch];
            if m.alt {
                bytes.insert(0, 27);
            }
            return Some(bytes);
        }
        _ if m.alt && !m.platform => {
            return key
                .key_char
                .as_ref()
                .or_else(|| (key.key.len() == 1).then_some(&key.key))
                .map(|s| format!("\x1b{s}").into_bytes());
        }
        _ => return None,
    };
    let prefix = m.alt && matches!(key.key.as_str(), "enter" | "tab" | "backspace");
    Some(if prefix {
        format!("\x1b{text}").into_bytes()
    } else {
        text.into_bytes()
    })
}

/// Enter key events in Microsoft's negotiated win32-input-mode (DECSET 9001).
/// Use this for Enter to preserve Shift/Ctrl instead of being reduced to a CR
/// or LF by ConPTY. Unicode/IME text stays on GPUI's existing input path.
pub fn win32_enter(key: &Keystroke, down: bool) -> Option<Vec<u8>> {
    if key.key != "enter" {
        return None;
    }
    let m = key.modifiers;
    // GPUI does not distinguish left/right modifiers. Use the left-hand flags:
    // LEFT_ALT_PRESSED=2, LEFT_CTRL_PRESSED=8, SHIFT_PRESSED=16.
    let state = 2 * u8::from(m.alt) + 8 * u8::from(m.control) + 16 * u8::from(m.shift);
    let character = if m.control { 10 } else { 13 };
    // CSI Vk;Sc;Uc;Kd;Cs;Rc _ -- VK_RETURN=13, scan code=28.
    Some(format!("\x1b[13;28;{character};{};{state};1_", u8::from(down)).into_bytes())
}

pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Prevent pasted escapes from terminating bracketed paste prematurely.
    let text = text
        .replace('\x1b', "")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.replace('\n', "\r").into_bytes()
    }
}

pub fn mouse(
    button: u8,
    col: usize,
    row: usize,
    release: bool,
    sgr: bool,
    modifiers: gpui::Modifiers,
) -> Option<Vec<u8>> {
    let button = button
        + 4 * u8::from(modifiers.shift)
        + 8 * u8::from(modifiers.alt)
        + 16 * u8::from(modifiers.control);
    if sgr {
        Some(
            format!(
                "\x1b[<{button};{};{}{}",
                col + 1,
                row + 1,
                if release { 'm' } else { 'M' }
            )
            .into_bytes(),
        )
    } else if col < 223 && row < 223 {
        Some(vec![
            27,
            b'[',
            b'M',
            if release { 35 } else { 32 + button },
            (33 + col) as u8,
            (33 + row) as u8,
        ])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(text: &'static str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }
    #[test]
    fn control_navigation_and_unicode_ownership() {
        assert_eq!(encode(&key("ctrl-c"), TermMode::empty()).unwrap(), [3]);
        assert_eq!(encode(&key("up"), TermMode::APP_CURSOR).unwrap(), b"\x1bOA");
        assert_eq!(
            encode(&key("ctrl-left"), TermMode::empty()).unwrap(),
            b"\x1b[1;5D"
        );
        assert!(encode(&key("a"), TermMode::empty()).is_none());
        assert!(encode(&key("hangul"), TermMode::empty()).is_none());
    }
    #[test]
    fn enter_matches_windows_terminal_legacy_encoding() {
        for mode in [TermMode::empty(), TermMode::LINE_FEED_NEW_LINE] {
            let enter = if mode.is_empty() {
                &b"\r"[..]
            } else {
                &b"\r\n"[..]
            };
            for name in ["enter", "shift-enter"] {
                assert_eq!(encode(&key(name), mode).unwrap(), enter, "{name}");
            }
            for name in ["ctrl-enter", "ctrl-shift-enter"] {
                assert_eq!(encode(&key(name), mode).unwrap(), b"\n", "{name}");
            }
            assert_eq!(
                encode(&key("alt-enter"), mode).unwrap(),
                [b"\x1b", enter].concat()
            );
            assert_eq!(encode(&key("ctrl-alt-enter"), mode).unwrap(), b"\x1b\n");
        }
    }

    #[test]
    fn win32_enter_preserves_modifiers_and_key_releases() {
        for (name, character, state) in [
            ("enter", 13, 0),
            ("shift-enter", 13, 16),
            ("ctrl-enter", 10, 8),
            ("ctrl-shift-enter", 10, 24),
            ("alt-enter", 13, 2),
            ("ctrl-alt-shift-enter", 10, 26),
        ] {
            for down in [true, false] {
                assert_eq!(
                    win32_enter(&key(name), down).unwrap(),
                    format!("\x1b[13;28;{character};{};{state};1_", u8::from(down)).as_bytes(),
                    "{name}, down={down}"
                );
            }
        }
        assert!(win32_enter(&key("a"), true).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn conpty_delivers_enter_modifiers_to_windows_console_apps() {
        use crate::{
            config::{Profile, Settings},
            session::Session,
        };
        use alacritty_terminal::event::Event;
        use std::{
            sync::atomic::Ordering,
            time::{Duration, Instant},
        };

        let script = r#"[Console]::WriteLine('KEYS_READY'); for ($i = 0; $i -lt 8; $i++) { $k = [Console]::ReadKey($true); [Console]::WriteLine(('KEY_{0}:{1}:{2}:{3}' -f $i, [int]$k.Key, [int]$k.KeyChar, [int]$k.Modifiers)) }; Start-Sleep -Seconds 20"#;
        let profile = Profile::new(
            "keys",
            "keys",
            "powershell.exe",
            &["-NoLogo", "-NoProfile", "-Command", script],
        );
        let (session, events) = Session::start(profile, None, &Settings::default(), 0).unwrap();
        let wait_for = |marker: &str| {
            let start = Instant::now();
            loop {
                while let Ok(event) = events.try_recv() {
                    match event {
                        Event::PtyWrite(text) => session.write(text.into_bytes()),
                        Event::Wakeup => session.pending.store(false, Ordering::Release),
                        _ => {}
                    }
                }
                if session.plain_text().contains(marker) {
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(15),
                    "waiting for {marker}: {}",
                    session.plain_text()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        wait_for("KEYS_READY");
        assert!(
            session.win32_input_mode(),
            "ConPTY must negotiate DECSET 9001"
        );
        for (i, (name, character, modifiers)) in [
            ("enter", 13, 0),
            ("shift-enter", 13, 2),
            ("ctrl-enter", 10, 4),
            ("ctrl-shift-enter", 10, 6),
            ("alt-enter", 13, 1),
            ("alt-shift-enter", 13, 3),
            ("ctrl-alt-enter", 10, 5),
            ("ctrl-alt-shift-enter", 10, 7),
        ]
        .into_iter()
        .enumerate()
        {
            session.input(win32_enter(&key(name), true).unwrap());
            session.write(win32_enter(&key(name), false).unwrap());
            wait_for(&format!("KEY_{i}:13:{character}:{modifiers}"));
        }
        session.shutdown();
        let start = Instant::now();
        while session.completed.try_recv().is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "ConPTY did not close"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[test]
    fn bracketed_paste_and_mouse_coordinates() {
        assert_eq!(
            paste("a\r\nb\x1b[201~", true),
            b"\x1b[200~a\nb[201~\x1b[201~"
        );
        assert_eq!(
            mouse(0, 3, 4, true, true, Default::default()).unwrap(),
            b"\x1b[<0;4;5m"
        );
    }
}

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
            if mode.contains(TermMode::LINE_FEED_NEW_LINE) {
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

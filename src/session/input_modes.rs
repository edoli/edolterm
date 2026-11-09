//! Observe ConPTY's keyboard mode requests without altering the output stream.
use alacritty_terminal::vte::{Params, Perform};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Default)]
pub(super) struct InputModes {
    win32: Arc<AtomicBool>,
}

impl InputModes {
    pub fn win32(&self) -> bool {
        self.win32.load(Ordering::Acquire)
    }
}

impl Perform for InputModes {
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if !ignore && intermediates == b"?" && matches!(action, 'h' | 'l') {
            for param in params {
                if param == [9001] {
                    self.win32.store(action == 'h', Ordering::Release);
                }
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediates.is_empty() && byte == b'c' {
            self.win32.store(false, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::Parser;

    #[test]
    fn fragmented_mode_requests_are_shared_and_reset() {
        let mut modes = InputModes::default();
        let shared = modes.clone();
        let mut parser = Parser::new();
        assert!(!shared.win32());
        for byte in b"\x1b[?1;9001h" {
            parser.advance(&mut modes, &[*byte]);
        }
        assert!(shared.win32());
        parser.advance(&mut modes, b"\x1b[?9001l");
        assert!(!shared.win32());
        parser.advance(&mut modes, b"\x1b[?9001h\x1bc");
        assert!(!shared.win32());
    }

    #[test]
    fn unrelated_sequences_and_string_contents_cannot_change_mode() {
        let mut modes = InputModes::default();
        let mut parser = Parser::new();
        for bytes in [
            &b"\x1b[9001h"[..],
            b"\x1b[?9001:1h",
            b"\x1b[?9001$p",
            b"\x1b]0;title [?9001h\x07",
            b"\x1bPdata [?9001h\x1b\\",
        ] {
            parser.advance(&mut modes, bytes);
            assert!(!modes.win32());
        }
    }
}

use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{Color, Rgb},
};
use gpui::{Hsla, rgb};

// Windows Terminal's Campbell scheme, in ANSI order (not Windows console order).
const CAMPBELL: [usize; 16] = [
    0x0c0c0c, 0xc50f1f, 0x13a10e, 0xc19c00, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc, 0x767676,
    0xe74856, 0x16c60c, 0xf9f1a5, 0x3b78ff, 0xb4009e, 0x61d6d6, 0xf2f2f2,
];

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Hsla,
    pub fg: Hsla,
    pub bar: Hsla,
    pub panel: Hsla,
    pub border: Hsla,
    pub muted: Hsla,
    pub hover: Hsla,
    pub accent: Hsla,
}
impl Theme {
    pub fn new(light: bool) -> Self {
        if light {
            Self {
                bg: rgb(0xfafafa).into(),
                fg: rgb(0x202124).into(),
                bar: rgb(0xe9eaed).into(),
                panel: rgb(0xffffff).into(),
                border: rgb(0xd0d3d8).into(),
                muted: rgb(0x5d6572).into(),
                hover: rgb(0xdce0e7).into(),
                accent: rgb(0x3469bc).into(),
            }
        } else {
            Self {
                bg: rgb(0x0c0c0c).into(),
                fg: rgb(0xcccccc).into(),
                bar: rgb(0x2b2b2b).into(),
                panel: rgb(0x2c2c2c).into(),
                border: rgb(0x404040).into(),
                muted: rgb(0xb0b0b0).into(),
                hover: rgb(0x3c3c3c).into(),
                accent: rgb(0x85b6ff).into(),
            }
        }
    }
    // The terminal scheme is independent of the application's light/dark chrome.
    pub fn terminal_bg(self) -> Hsla {
        rgb(0x0c0c0c).into()
    }
    pub fn terminal_fg(self) -> Hsla {
        rgb(0xcccccc).into()
    }
    pub fn terminal_selection(self) -> Hsla {
        rgb(0x404040).into()
    }
    pub fn color(&self, color: Color, overrides: &Colors, bold: bool) -> Hsla {
        match color {
            Color::Spec(c) => {
                rgb((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)).into()
            }
            Color::Indexed(i) => self.indexed(i as usize, overrides),
            Color::Named(n) => {
                let mut i = n as usize;
                if bold && i < 8 {
                    i += 8;
                }
                self.indexed(i, overrides)
            }
        }
    }
    pub fn indexed(&self, i: usize, overrides: &Colors) -> Hsla {
        let c = self.rgb(i, overrides);
        rgb((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)).into()
    }
    pub fn rgb(&self, i: usize, overrides: &Colors) -> Rgb {
        if i < 269
            && let Some(c) = overrides[i]
        {
            return c;
        }
        let value = match i {
            0..=15 => CAMPBELL[i],
            16..=231 => {
                let n = i - 16;
                let scale = |c: usize| if c == 0 { 0 } else { 55 + c * 40 };
                (scale(n / 36) << 16) | (scale(n / 6 % 6) << 8) | scale(n % 6)
            }
            232..=255 => {
                let v = 8 + (i - 232) * 10;
                (v << 16) | (v << 8) | v
            }
            257 => 0x0c0c0c,
            258 => 0xffffff,
            259..=266 => {
                let v = CAMPBELL[i - 259];
                (((v >> 16 & 255) / 2) << 16) | (((v >> 8 & 255) / 2) << 8) | ((v & 255) / 2)
            }
            267 => 0xf2f2f2,
            268 => 0x666666,
            _ => 0xcccccc,
        };
        Rgb {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        }
    }
}

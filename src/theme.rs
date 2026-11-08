use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{Color, Rgb},
};
use gpui::{Hsla, rgb};

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
    pub selection: Hsla,
    pub light: bool,
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
                selection: rgb(0xc5d9fa).into(),
                light,
            }
        } else {
            Self {
                bg: rgb(0x101215).into(),
                fg: rgb(0xd9dce3).into(),
                bar: rgb(0x202329).into(),
                panel: rgb(0x262a32).into(),
                border: rgb(0x343942).into(),
                muted: rgb(0x929baa).into(),
                hover: rgb(0x343b47).into(),
                accent: rgb(0x85b6ff).into(),
                selection: rgb(0x334d71).into(),
                light,
            }
        }
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
        let ansi = [
            0x15191f, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xc8ccd4,
            0x6b7482, 0xff858f, 0xb1e18b, 0xffda8a, 0x85b6ff, 0xdda0f4, 0x79d6dc, 0xffffff,
        ];
        let value = match i {
            0..=15 => ansi[i],
            16..=231 => {
                let n = i - 16;
                let scale = |c: usize| if c == 0 { 0 } else { 55 + c * 40 };
                (scale(n / 36) << 16) | (scale(n / 6 % 6) << 8) | scale(n % 6)
            }
            232..=255 => {
                let v = 8 + (i - 232) * 10;
                (v << 16) | (v << 8) | v
            }
            257 => {
                if self.light {
                    0xfafafa
                } else {
                    0x101215
                }
            }
            258 => {
                if self.light {
                    0x3469bc
                } else {
                    0x85b6ff
                }
            }
            259..=266 => {
                let v = ansi[i - 259];
                (((v >> 16 & 255) / 2) << 16) | (((v >> 8 & 255) / 2) << 8) | ((v & 255) / 2)
            }
            _ => {
                if self.light {
                    0x202124
                } else {
                    0xd9dce3
                }
            }
        };
        Rgb {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        }
    }
}

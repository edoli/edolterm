//! UI palettes and terminal colors live here. Views use named color roles;
//! reusable widgets use `Theme::current` to follow the saved theme preference.
use crate::config::{Settings, SharedSettings};
use alacritty_terminal::{
    term::color::Colors,
    vte::ansi::{Color, Rgb},
};
use gpui::{App, Hsla, WindowAppearance, rgb, rgba};

// Windows Terminal's Campbell scheme, in ANSI order (not Windows console order).
const CAMPBELL: [usize; 16] = [
    0x0c0c0c, 0xc50f1f, 0x13a10e, 0xc19c00, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc, 0x767676,
    0xe74856, 0x16c60c, 0xf9f1a5, 0x3b78ff, 0xb4009e, 0x61d6d6, 0xf2f2f2,
];

/// Colors for a surface and the text displayed on it.
#[derive(Clone, Copy)]
pub struct SurfaceColors {
    pub bg: Hsla,
    pub fg: Hsla,
}

#[derive(Clone, Copy)]
pub struct InputColors {
    pub surface: SurfaceColors,
    pub border: Hsla,
    pub placeholder: Hsla,
    pub selection: Hsla,
    pub cursor: Hsla,
}

#[derive(Clone, Copy)]
pub struct InteractiveColors {
    pub normal: SurfaceColors,
    pub hover: SurfaceColors,
}

#[derive(Clone, Copy)]
pub struct WindowColors {
    pub inactive_fg: Hsla,
    pub close_hover: SurfaceColors,
}

/// Application colors. Define all light/dark UI colors here, not in views.
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
    pub unsaved: SurfaceColors,
    pub description: Hsla,
    pub bell: Hsla,
    pub overlay: Hsla,
    pub scrollbar: Hsla,
    pub input: InputColors,
    pub tooltip: SurfaceColors,
    pub danger: InteractiveColors,
    pub window: WindowColors,
    pub error: SurfaceColors,
    pub terminal: TerminalTheme,
}

impl Theme {
    pub fn current(cx: &App) -> Self {
        match cx.try_global::<SharedSettings>() {
            Some(settings) => Self::from_setting(&settings.0.theme, cx),
            None => Self::from_setting(&Settings::default().theme, cx),
        }
    }

    fn is_light(setting: &str, cx: &App) -> bool {
        match setting {
            "system" => matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
            "light" => true,
            _ => false,
        }
    }

    pub fn from_setting(setting: &str, cx: &App) -> Self {
        Self::new(Self::is_light(setting, cx))
    }

    pub fn new(light: bool) -> Self {
        let dark = Self {
            bg: rgb(0x0c0c0c).into(),
            fg: rgb(0xcccccc).into(),
            bar: rgb(0x2b2b2b).into(),
            panel: rgb(0x2c2c2c).into(),
            border: rgb(0x404040).into(),
            muted: rgb(0xb0b0b0).into(),
            hover: rgb(0x3c3c3c).into(),
            accent: rgb(0x85b6ff).into(),
            unsaved: SurfaceColors {
                bg: rgb(0x253246).into(),
                fg: rgb(0x85b6ff).into(),
            },
            description: rgb(0x929baa).into(),
            bell: rgb(0xe5c07b).into(),
            overlay: rgba(0x00000088).into(),
            scrollbar: Hsla::from(rgb(0xb0b0b0)).opacity(0.45),
            input: InputColors {
                surface: SurfaceColors {
                    bg: rgb(0x171b22).into(),
                    fg: rgb(0xd9dce3).into(),
                },
                border: rgb(0x4a5362).into(),
                placeholder: rgb(0x7e8999).into(),
                selection: rgb(0x334d71).into(),
                cursor: rgb(0x85b6ff).into(),
            },
            tooltip: SurfaceColors {
                bg: rgb(0x181818).into(),
                fg: rgb(0xffffff).into(),
            },
            danger: InteractiveColors {
                normal: SurfaceColors {
                    bg: rgba(0xe5737310).into(),
                    fg: rgb(0xd49a9a).into(),
                },
                hover: SurfaceColors {
                    bg: rgba(0xe573732b).into(),
                    fg: rgb(0xffb4ab).into(),
                },
            },
            window: WindowColors {
                inactive_fg: Hsla::from(rgb(0xcccccc)).opacity(0.4),
                close_hover: SurfaceColors {
                    bg: rgb(0xc42b1c).into(),
                    fg: rgb(0xffffff).into(),
                },
            },
            error: SurfaceColors {
                bg: rgb(0x48282c).into(),
                fg: rgb(0xffb4bc).into(),
            },
            terminal: TerminalTheme::default(),
        };
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
                unsaved: SurfaceColors {
                    bg: rgb(0xeaf1fc).into(),
                    fg: rgb(0x3469bc).into(),
                },
                description: rgb(0x5d6572).into(),
                scrollbar: Hsla::from(rgb(0x5d6572)).opacity(0.45),
                input: InputColors {
                    surface: SurfaceColors {
                        bg: rgb(0xffffff).into(),
                        fg: rgb(0x202124).into(),
                    },
                    border: rgb(0xd0d3d8).into(),
                    placeholder: rgb(0x5d6572).into(),
                    selection: rgb(0xdce0e7).into(),
                    cursor: rgb(0x3469bc).into(),
                },
                tooltip: SurfaceColors {
                    bg: rgb(0xe9eaed).into(),
                    fg: rgb(0x202124).into(),
                },
                danger: InteractiveColors {
                    normal: SurfaceColors {
                        fg: rgb(0xb65050).into(),
                        ..dark.danger.normal
                    },
                    hover: SurfaceColors {
                        fg: rgb(0xa83d3d).into(),
                        ..dark.danger.hover
                    },
                },
                window: WindowColors {
                    inactive_fg: Hsla::from(rgb(0x202124)).opacity(0.4),
                    ..dark.window
                },
                error: SurfaceColors {
                    bg: rgb(0xfce8e6).into(),
                    fg: rgb(0xa62b36).into(),
                },
                ..dark
            }
        } else {
            dark
        }
    }
}

/// The terminal scheme is independent of the application's light/dark chrome.
#[derive(Clone, Copy)]
pub struct TerminalTheme {
    pub bg: Hsla,
    pub fg: Hsla,
    pub selection: Hsla,
    pub search_match: SurfaceColors,
    pub dim_opacity: f32,
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self {
            bg: rgb(0x0c0c0c).into(),
            fg: rgb(0xcccccc).into(),
            selection: rgb(0x404040).into(),
            search_match: SurfaceColors {
                bg: rgb(0x886629).into(),
                fg: rgb(0xffffff).into(),
            },
            dim_opacity: 0.6,
        }
    }
}

impl TerminalTheme {
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

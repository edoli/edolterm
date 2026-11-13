//! Built-in terminal palettes. Microsoft Terminal defaults are MIT licensed.
//! Source: https://github.com/microsoft/terminal/blob/main/src/cascadia/TerminalSettingsModel/defaults.json
//! Ubuntu matches the Microsoft.WSL fragment supplied by the installed Ubuntu 24.04 distribution.
//! Colors are stored in ANSI order, independently of the application UI theme.

#[derive(Clone, Copy, Debug)]
pub struct ColorScheme {
    pub name: &'static str,
    pub background: u32,
    pub foreground: u32,
    pub cursor: u32,
    pub selection: u32,
    pub ansi: [u32; 16],
}

pub const DEFAULT_SCHEME: &str = "Campbell";

pub fn find(name: &str) -> Option<&'static ColorScheme> {
    COLOR_SCHEMES.iter().find(|scheme| scheme.name == name)
}

pub fn resolve(name: &str) -> &'static ColorScheme {
    find(name).unwrap_or_else(|| find(DEFAULT_SCHEME).expect("default color scheme"))
}

pub const COLOR_SCHEMES: &[ColorScheme] = &[
    ColorScheme {
        name: "Campbell",
        background: 0x0c0c0c,
        foreground: 0xcccccc,
        cursor: 0xffffff,
        selection: 0x404040,
        ansi: [
            0x0c0c0c, 0xc50f1f, 0x13a10e, 0xc19c00, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc,
            0x767676, 0xe74856, 0x16c60c, 0xf9f1a5, 0x3b78ff, 0xb4009e, 0x61d6d6, 0xf2f2f2,
        ],
    },
    ColorScheme {
        name: "Campbell Powershell",
        background: 0x012456,
        foreground: 0xcccccc,
        cursor: 0xffffff,
        selection: 0x404040,
        ansi: [
            0x0c0c0c, 0xc50f1f, 0x13a10e, 0xc19c00, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc,
            0x767676, 0xe74856, 0x16c60c, 0xf9f1a5, 0x3b78ff, 0xb4009e, 0x61d6d6, 0xf2f2f2,
        ],
    },
    ColorScheme {
        name: "CGA",
        background: 0x000000,
        foreground: 0xaaaaaa,
        cursor: 0x00aa00,
        selection: 0xffffff,
        ansi: [
            0x000000, 0xaa0000, 0x00aa00, 0xaa5500, 0x0000aa, 0xaa00aa, 0x00aaaa, 0xaaaaaa,
            0x555555, 0xff5555, 0x55ff55, 0xffff55, 0x5555ff, 0xff55ff, 0x55ffff, 0xffffff,
        ],
    },
    ColorScheme {
        name: "Dark+",
        background: 0x1e1e1e,
        foreground: 0xcccccc,
        cursor: 0x808080,
        selection: 0xffffff,
        ansi: [
            0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5,
            0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xe5e5e5,
        ],
    },
    ColorScheme {
        name: "Dimidium",
        background: 0x141414,
        foreground: 0xbab7b6,
        cursor: 0x37e57b,
        selection: 0x8db8e5,
        ansi: [
            0x000000, 0xcf494c, 0x60b442, 0xdb9c11, 0x0575d8, 0xaf5ed2, 0x1db6bb, 0xbab7b6,
            0x817e7e, 0xff643b, 0x37e57b, 0xfccd1a, 0x688dfd, 0xed6fe9, 0x32e0fb, 0xdee3e4,
        ],
    },
    ColorScheme {
        name: "IBM 5153",
        background: 0x000000,
        foreground: 0xaaaaaa,
        cursor: 0x00aa00,
        selection: 0xffffff,
        ansi: [
            0x000000, 0xaa0000, 0x00aa00, 0xc47e00, 0x0000aa, 0xaa00aa, 0x00aaaa, 0xaaaaaa,
            0x555555, 0xff5555, 0x55ff55, 0xffff55, 0x5555ff, 0xff55ff, 0x55ffff, 0xffffff,
        ],
    },
    ColorScheme {
        name: "One Half Dark",
        background: 0x282c34,
        foreground: 0xdcdfe4,
        cursor: 0xffffff,
        selection: 0x404040,
        ansi: [
            0x282c34, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xdcdfe4,
            0x5a6374, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xdcdfe4,
        ],
    },
    ColorScheme {
        name: "One Half Light",
        background: 0xfafafa,
        foreground: 0x383a42,
        cursor: 0x4f525d,
        selection: 0x383a42,
        ansi: [
            0x383a42, 0xe45649, 0x50a14f, 0xc18301, 0x0184bc, 0xa626a4, 0x0997b3, 0xfafafa,
            0x4f525d, 0xdf6c75, 0x98c379, 0xe4c07a, 0x61afef, 0xc577dd, 0x56b5c1, 0xffffff,
        ],
    },
    ColorScheme {
        name: "Ottosson",
        background: 0x000000,
        foreground: 0xbebebe,
        cursor: 0xffffff,
        selection: 0x92a4fd,
        ansi: [
            0x000000, 0xbe2c21, 0x3fae3a, 0xbe9a4a, 0x204dbe, 0xbb54be, 0x00a7b2, 0xbebebe,
            0x808080, 0xff3e30, 0x58ea51, 0xffc944, 0x2f6aff, 0xfc74ff, 0x00e1f0, 0xffffff,
        ],
    },
    ColorScheme {
        name: "Solarized Dark",
        background: 0x002b36,
        foreground: 0x839496,
        cursor: 0xffffff,
        selection: 0x404040,
        ansi: [
            0x002b36, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x073642, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ],
    },
    ColorScheme {
        name: "Solarized Light",
        background: 0xfdf6e3,
        foreground: 0x657b83,
        cursor: 0x002b36,
        selection: 0x2c4d57,
        ansi: [
            0x002b36, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
            0x073642, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
        ],
    },
    ColorScheme {
        name: "Tango Dark",
        background: 0x000000,
        foreground: 0xd3d7cf,
        cursor: 0xffffff,
        selection: 0x404040,
        ansi: [
            0x000000, 0xcc0000, 0x4e9a06, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf,
            0x555753, 0xef2929, 0x8ae234, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
        ],
    },
    ColorScheme {
        name: "Tango Light",
        background: 0xffffff,
        foreground: 0x555753,
        cursor: 0x000000,
        selection: 0x141414,
        ansi: [
            0x000000, 0xcc0000, 0x4e9a06, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf,
            0x555753, 0xef2929, 0x8ae234, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
        ],
    },
    ColorScheme {
        name: "Ubuntu",
        background: 0x300a24,
        foreground: 0xffffff,
        cursor: 0xffffff,
        selection: 0xffffff,
        ansi: [
            0x171421, 0xc21a23, 0x26a269, 0xa2734c, 0x0037da, 0x881798, 0x3a96dd, 0xcccccc,
            0x767676, 0xc01c28, 0x26a269, 0xa2734c, 0x08458f, 0xa347ba, 0x2c9fb3, 0xf2f2f2,
        ],
    },
    ColorScheme {
        name: "Vintage",
        background: 0x000000,
        foreground: 0xc0c0c0,
        cursor: 0xffffff,
        selection: 0x404040,
        ansi: [
            0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xc0c0c0,
            0x808080, 0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, 0xffffff,
        ],
    },
    ColorScheme {
        name: "VSCode Dark Modern",
        background: 0x1f1f1f,
        foreground: 0xcccccc,
        cursor: 0xffffff,
        selection: 0x264f78,
        ansi: [
            0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5,
            0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xe5e5e5,
        ],
    },
    ColorScheme {
        name: "VSCode Light Modern",
        background: 0xffffff,
        foreground: 0x3b3b3b,
        cursor: 0x000000,
        selection: 0xadd6ff,
        ansi: [
            0x000000, 0xcd3131, 0x00bc00, 0x949800, 0x0451a5, 0xbc05bc, 0x0598bc, 0x555555,
            0x666666, 0xcd3131, 0x14ce14, 0xb5ba00, 0x0451a5, 0xbc05bc, 0x0598bc, 0xa5a5a5,
        ],
    },
];

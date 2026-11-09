//! Monochrome SVG assets use a fixed view box instead of font glyph baselines.
use gpui::{prelude::*, *};
use std::borrow::Cow;

macro_rules! icons {
    ($($name:ident => $file:literal),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub enum Icon { $($name),+ }
        impl Icon {
            fn path(self) -> &'static str { match self { $(Self::$name => concat!("icons/", $file, ".svg")),+ } }
        }
        pub struct Assets;
        impl AssetSource for Assets {
            fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
                let bytes: &'static [u8] = match path { $(concat!("icons/", $file, ".svg") => include_bytes!(concat!("../assets/icons/", $file, ".svg"))),+, _ => return Ok(None) };
                Ok(Some(Cow::Borrowed(bytes)))
            }
            fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
                Ok(vec![$(concat!("icons/", $file, ".svg")),+].into_iter().filter(|p| p.starts_with(path)).map(Into::into).collect())
            }
        }
    }
}

icons! {
    Close => "close", Trash => "trash", Plus => "plus", Minus => "minus", ChevronDown => "chevron-down",
    Terminal => "terminal", Settings => "settings",
    ArrowUp => "arrow-up", ArrowDown => "arrow-down", Square => "square", CheckSquare => "check-square",
}

#[derive(IntoElement)]
pub struct IconElement {
    name: Icon,
    size: f32,
    style: StyleRefinement,
}

impl Styled for IconElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for IconElement {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let mut element = svg()
            .path(self.name.path())
            .size(px(self.size))
            .flex_shrink_0()
            .text_color(window.text_style().color);
        element.style().refine(&self.style);
        element
    }
}

pub fn icon(name: Icon, size: f32) -> IconElement {
    IconElement {
        name,
        size,
        style: StyleRefinement::default(),
    }
}

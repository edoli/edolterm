//! Extract native icons off the UI thread, including legacy icons with AND masks.
use super::{cache::Task, executable::FileStamp};
use crate::icons::profiles::LoadedIcon;
use anyhow::{Result, bail, ensure};
use gpui::RenderImage;
use std::{
    collections::{BTreeMap, BTreeSet},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::Arc,
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON, PrivateExtractIconsW},
};

const SIDE: i32 = 64;
const BYTES: usize = SIDE as usize * SIDE as usize * 4;

#[derive(Default)]
pub(super) struct BitmapCache {
    images: BTreeMap<PathBuf, (Option<FileStamp>, Option<Arc<RenderImage>>)>,
}

impl BitmapCache {
    pub fn load(&mut self, tasks: &[Task]) -> Vec<LoadedIcon> {
        let mut programs = BTreeSet::new();
        let mut active = BTreeSet::new();
        let mut loaded = Vec::new();
        for task in tasks {
            if !programs.insert(&task.program) {
                continue;
            }
            active.insert(task.icon.clone());
            let entry = self
                .images
                .entry(task.icon.clone())
                .or_insert_with(|| (task.stamp.clone(), extract(&task.icon).ok()));
            if entry.0 != task.stamp {
                *entry = (task.stamp.clone(), extract(&task.icon).ok());
            }
            loaded.push(LoadedIcon {
                program: task.program.clone(),
                image: entry.1.clone(),
            });
        }
        self.images.retain(|path, _| active.contains(path));
        loaded
    }
}

struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}

struct Canvas {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
}
impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
impl Canvas {
    fn new() -> Result<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIDE,
                biHeight: -SIDE,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: all GDI objects stay on this worker. The selected bitmap is
        // restored before deletion and its pixel pointer never escapes Canvas.
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            ensure!(!dc.is_null(), "CreateCompatibleDC failed");
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                DeleteDC(dc);
                bail!("CreateDIBSection failed");
            }
            let previous = SelectObject(dc, bitmap);
            if previous.is_null() || previous as isize == -1 {
                DeleteObject(bitmap);
                DeleteDC(dc);
                bail!("SelectObject failed");
            }
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast(),
            })
        }
    }

    fn draw(&mut self, icon: &Icon, background: u8) -> Result<Vec<u8>> {
        unsafe {
            std::slice::from_raw_parts_mut(self.bits, BYTES).fill(background);
            ensure!(
                DrawIconEx(self.dc, 0, 0, icon.0, SIDE, SIDE, 0, null_mut(), DI_NORMAL) != 0,
                "DrawIconEx failed"
            );
            // Synchronize GDI before accessing the DIB memory directly.
            ensure!(GdiFlush() != 0, "GdiFlush failed");
            Ok(std::slice::from_raw_parts(self.bits, BYTES).to_vec())
        }
    }
}

fn extract(path: &Path) -> Result<Arc<RenderImage>> {
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut handle = null_mut();
    let mut resource = 0;
    let count = unsafe {
        PrivateExtractIconsW(
            wide.as_ptr(),
            0,
            SIDE,
            SIDE,
            &mut handle,
            &mut resource,
            1,
            0,
        )
    };
    ensure!(!handle.is_null(), "executable has no icon");
    let icon = Icon(handle);
    ensure!(count == 1, "icon extraction failed");
    let mut canvas = Canvas::new()?;
    let mut black = canvas.draw(&icon, 0)?;
    let white = canvas.draw(&icon, 255)?;
    // Reconstruct straight alpha from black/white composites. This handles both
    // modern alpha icons and old icons whose transparency lives in an AND mask.
    for (pixel, white) in black.chunks_exact_mut(4).zip(white.chunks_exact(4)) {
        let transparency: u32 = (0..3)
            .map(|c| u32::from(white[c].saturating_sub(pixel[c])))
            .sum();
        let alpha = 255 - (transparency + 1) / 3;
        for color in &mut pixel[..3] {
            *color = if alpha == 0 {
                0
            } else {
                ((u32::from(*color) * 255 + alpha / 2) / alpha).min(255) as u8
            };
        }
        pixel[3] = alpha as u8;
    }
    // GPUI's RenderImage expects straight-alpha BGRA, matching this top-down DIB.
    let pixels =
        image::RgbaImage::from_raw(SIDE as u32, SIDE as u32, black).expect("fixed DIB size");
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_icon_pixels_have_transparency_and_reuse_the_same_image() {
        let executable = std::env::current_exe().unwrap();
        let profiles = [crate::config::Profile::new(
            "one",
            "One",
            executable.to_string_lossy(),
            &[],
        )];
        let tasks = super::super::cache::IconCache::default().tasks(&profiles, &executable);
        let mut cache = BitmapCache::default();
        let first = cache.load(&tasks).pop().unwrap().image.unwrap();
        let second = cache.load(&tasks).pop().unwrap().image.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let pixels = first.as_bytes(0).unwrap();
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }
}

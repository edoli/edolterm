//! Shared, already-decoded profile images. Rendering never queries the filesystem.
use super::{Icon, icon};
use gpui::{prelude::*, *};
use std::{collections::BTreeMap, sync::Arc};

pub struct LoadedIcon {
    pub program: String,
    pub image: Option<Arc<RenderImage>>,
}

#[derive(Default)]
struct Catalog {
    generation: u64,
    images: BTreeMap<String, Arc<RenderImage>>,
}
impl Global for Catalog {}

pub fn get(program: &str, cx: &App) -> Option<Arc<RenderImage>> {
    cx.try_global::<Catalog>()?.images.get(program).cloned()
}

pub fn element(program: &str, size: f32, cx: &App) -> AnyElement {
    match get(program, cx) {
        Some(image) => img(image).size(px(size)).flex_shrink_0().into_any_element(),
        None => icon(Icon::Terminal, size).into_any_element(),
    }
}

pub fn receive(receiver: async_channel::Receiver<Vec<LoadedIcon>>, cx: &mut App) {
    let catalog = cx.default_global::<Catalog>();
    catalog.generation += 1;
    let generation = catalog.generation;
    cx.spawn(async move |cx| {
        let Ok(images) = receiver.recv().await else {
            return;
        };
        cx.update(|cx| {
            let catalog = cx.global_mut::<Catalog>();
            if catalog.generation != generation {
                return;
            }
            let mut changed = false;
            for loaded in images {
                match loaded.image {
                    Some(image) => {
                        changed |= catalog
                            .images
                            .get(&loaded.program)
                            .is_none_or(|old| old.id != image.id);
                        catalog.images.insert(loaded.program, image);
                    }
                    None => changed |= catalog.images.remove(&loaded.program).is_some(),
                }
            }
            if changed {
                cx.refresh_windows();
            }
        });
    })
    .detach();
}

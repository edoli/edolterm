use std::{env, fs, path::PathBuf};

fn main() {
    const SOURCE: &str = "assets/app-icon/edolterm-icon.svg";
    println!("cargo:rerun-if-changed={SOURCE}");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    let svg = fs::read(SOURCE).expect("failed to read the app icon SVG");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())
        .expect("failed to parse the app icon SVG");
    let mut icon = ico::IconDir::new(ico::ResourceType::Icon);

    // Render each size from the vector source, including common Windows DPI scales.
    for size in [16, 20, 24, 32, 40, 48, 64, 128, 256] {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
            .expect("failed to allocate the app icon image");
        let scale = (size as f32 / tree.size().width()).min(size as f32 / tree.size().height());
        let transform = resvg::tiny_skia::Transform::from_translate(
            (size as f32 - tree.size().width() * scale) / 2.,
            (size as f32 - tree.size().height() * scale) / 2.,
        )
        .pre_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        // tiny-skia stores premultiplied colors; ICO expects straight-alpha RGBA.
        let rgba = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        icon.add_entry(
            ico::IconDirEntry::encode(&image).expect("failed to encode an app icon size"),
        );
    }

    let icon_path = out_dir.join("edolterm-icon.ico");
    icon.write(fs::File::create(&icon_path).expect("failed to create the app icon ICO"))
        .expect("failed to write the app icon ICO");

    // GPUI's Windows platform loads resource 1 for its native window class icon.
    let resource_path = out_dir.join("edolterm-icon.rc");
    fs::write(
        &resource_path,
        format!(
            "1 ICON \"{}\"\n",
            icon_path.to_string_lossy().replace('\\', "/")
        ),
    )
    .expect("failed to write the app icon resource script");
    embed_resource::compile_for(&resource_path, ["edolterm"], embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the app icon in edolterm.exe");
}

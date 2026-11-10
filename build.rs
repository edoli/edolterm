use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=EDOLTERM_RELEASE_REPOSITORY");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let parts: Vec<_> = version.split('.').collect();
    assert!(
        parts.len() == 3
            && parts.iter().all(|s| !s.is_empty()
                && s.bytes().all(|c| c.is_ascii_digit())
                && (s.len() == 1 || !s.starts_with('0'))
                && s.parse::<u16>().is_ok()),
        "version must be major.minor.patch (0..65535)"
    );
    let repository =
        env::var("EDOLTERM_RELEASE_REPOSITORY").unwrap_or_else(|_| "edoli/edolterm".into());
    assert!(
        repository.split('/').count() == 2
            && repository.split('/').all(|s| !s.is_empty()
                && s != "."
                && s != ".."
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))),
        "EDOLTERM_RELEASE_REPOSITORY must be owner/repository"
    );
    println!("cargo:rustc-env=EDOLTERM_RELEASE_REPOSITORY={repository}");
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
    for size in [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256] {
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
            r#"1 ICON "{}"
1 VERSIONINFO
FILEVERSION {},{},{},0
PRODUCTVERSION {},{},{},0
FILEOS 0x40004
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "CompanyName", "Edoli\0"
   VALUE "FileDescription", "Edolterm\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "Edolterm\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "edolterm.exe\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0409, 1200
 END
END
"#,
            icon_path.to_string_lossy().replace('\\', "/"),
            parts[0],
            parts[1],
            parts[2],
            parts[0],
            parts[1],
            parts[2],
        ),
    )
    .expect("failed to write the app icon resource script");
    embed_resource::compile_for(&resource_path, ["edolterm"], embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the app icon in edolterm.exe");
}

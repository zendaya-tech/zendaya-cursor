//! Generate the canonical app icon from Zendaya Cursor's own Rust artwork.
#[allow(dead_code)]
#[path = "../src/model.rs"]
mod model;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = model::Settings {
        size: 96,
        ..Default::default()
    };
    let rgba = model::raster(&settings).map_err(std::io::Error::other)?;
    let source = image::RgbaImage::from_raw(96, 96, rgba).ok_or("invalid icon pixels")?;
    let icon = image::imageops::resize(&source, 128, 128, image::imageops::FilterType::Lanczos3);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/app-icon/hicolor/128x128/apps/ai.storyteller.zendaya-cursor.png");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    icon.save(&path)?;
    println!("{}", path.display());
    Ok(())
}

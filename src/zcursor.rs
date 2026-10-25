//! Bounded, read-only ZCursor v1 loader. Archives are never extracted or executed.
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read, Seek},
    path::Path,
};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub role: String,
    pub image: String,
    pub layout: String,
    pub frame_size: Dimensions,
    pub frame_count: u32,
    pub frame_durations_ms: Vec<u32>,
    pub hotspot: Hotspot,
    pub scale: f64,
    #[serde(default)]
    pub source: Source,
}
#[derive(Default, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub state_id: Option<u32>,
}
#[derive(Deserialize, Clone, Copy)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}
#[derive(Deserialize, Clone, Copy)]
pub struct Hotspot {
    pub x: u32,
    pub y: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    format_version: u32,
    name: String,
    author: Option<String>,
    #[serde(default)]
    source: PackageSource,
    #[serde(default)]
    cursors: Vec<Entry>,
    #[serde(default)]
    effects: Vec<Entry>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageSource {
    format: Option<String>,
    png_orientation: Option<String>,
}
impl PackageSource {
    fn legacy_bottom_up(&self) -> bool {
        self.format.as_deref() == Some("CursorFX") && self.png_orientation.is_none()
    }
}
pub struct Sprite {
    pub entry: Entry,
    pub pixels: image::RgbaImage,
}
pub struct Package {
    pub name: String,
    pub author: Option<String>,
    pub cursors: Vec<Sprite>,
    pub effects: Vec<Sprite>,
}

fn safe_path(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':', '\0'])
        && !name.starts_with('/')
        && name.split('/').all(|part| !matches!(part, "" | "." | ".."))
}
fn read_member<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    cap: u64,
) -> Result<Vec<u8>, String> {
    if !safe_path(name) {
        return Err("Chemin d'archive invalide.".into());
    }
    let mut file = zip.by_name(name).map_err(|e| format!("{name} : {e}"))?;
    if file.size() > cap {
        return Err(format!("{name} dépasse la limite de taille."));
    }
    let mut data = Vec::new();
    file.by_ref()
        .take(cap + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > cap {
        return Err("Membre ZIP trop volumineux.".into());
    }
    Ok(data)
}
impl Package {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
            return Err("Archive limitée à 64 Mo.".into());
        }
        Self::read(file)
    }
    fn read<R: Read + Seek>(reader: R) -> Result<Self, String> {
        let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("Archive ZCursor : {e}"))?;
        if zip.len() > 2048 {
            return Err("Trop de fichiers dans le thème.".into());
        }
        let mut names = HashSet::new();
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(|e| e.to_string())?;
            if !safe_path(file.name().trim_end_matches('/'))
                || !names.insert(file.name().to_owned())
            {
                return Err("Chemin dangereux ou nom dupliqué dans l'archive.".into());
            }
        }
        let manifest: Manifest =
            serde_json::from_slice(&read_member(&mut zip, "theme.json", 2 * 1024 * 1024)?)
                .map_err(|e| format!("theme.json : {e}"))?;
        if manifest.format != "zcursor" || manifest.format_version != 1 {
            return Err("Format ZCursor non pris en charge (version 1 requise).".into());
        }
        if manifest.name.trim().is_empty()
            || manifest.name.len() > 256
            || manifest.cursors.len() + manifest.effects.len() > 512
        {
            return Err("Nom ou nombre de curseurs invalide.".into());
        }
        let legacy_bottom_up = manifest.source.legacy_bottom_up();
        let mut budget = 128 * 1024 * 1024u64;
        let mut ids = HashSet::new();
        let mut load = |entries: Vec<Entry>| -> Result<Vec<Sprite>, String> {
            entries
                .into_iter()
                .map(|e| {
                    let (w, h, n) = (e.frame_size.width, e.frame_size.height, e.frame_count);
                    if !ids.insert(e.id.clone())
                        || e.layout != "horizontal-strip"
                        || !(1..=256).contains(&w)
                        || !(1..=256).contains(&h)
                        || !(1..=256).contains(&n)
                        || e.frame_durations_ms.len() != n as usize
                        || e.frame_durations_ms
                            .iter()
                            .any(|d| !(1..=60_000).contains(d))
                        || e.hotspot.x >= w
                        || e.hotspot.y >= h
                        || !e.scale.is_finite()
                        || !(0.25..=8.0).contains(&e.scale)
                    {
                        return Err(format!("Paramètres invalides pour {}.", e.id));
                    }
                    let bytes = u64::from(w) * u64::from(h) * u64::from(n) * 4;
                    budget = budget
                        .checked_sub(bytes)
                        .ok_or("Le thème décompressé dépasse 128 Mo.")?;
                    let data = read_member(&mut zip, &e.image, 32 * 1024 * 1024)?;
                    let decoder = image::ImageReader::with_format(
                        Cursor::new(&data),
                        image::ImageFormat::Png,
                    );
                    if decoder.into_dimensions().map_err(|e| e.to_string())? != (w * n, h) {
                        return Err(format!("Dimensions PNG incohérentes : {}.", e.image));
                    }
                    let mut reader =
                        image::ImageReader::with_format(Cursor::new(data), image::ImageFormat::Png);
                    let mut limits = image::Limits::default();
                    limits.max_alloc = Some(128 * 1024 * 1024);
                    reader.limits(limits);
                    let mut pixels = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
                    if legacy_bottom_up {
                        image::imageops::flip_vertical_in_place(&mut pixels);
                    }
                    Ok(Sprite { entry: e, pixels })
                })
                .collect()
        };
        let cursors = load(manifest.cursors)?;
        let effects = load(manifest.effects)?;
        if cursors.is_empty() && effects.is_empty() {
            return Err("Ce thème ne contient aucune image.".into());
        }
        Ok(Self {
            name: manifest.name,
            author: manifest.author,
            cursors,
            effects,
        })
    }
    /// One native Windows cursor per role, preferring normal state and nearest DPI variant.
    pub fn roles(&self, scale: f64) -> Vec<&Sprite> {
        let mut roles: Vec<&Sprite> = Vec::new();
        for sprite in &self.cursors {
            if windows_role(&sprite.entry.role).is_none() {
                continue;
            }
            let score = |s: &Sprite| {
                (
                    (s.entry.scale - scale).abs(),
                    s.entry.source.state_id.unwrap_or(1) != 1,
                )
            };
            if let Some(i) = roles.iter().position(|s| s.entry.role == sprite.entry.role) {
                if score(sprite) < score(roles[i]) {
                    roles[i] = sprite;
                }
            } else {
                roles.push(sprite);
            }
        }
        roles
    }
}
pub fn windows_role(role: &str) -> Option<u32> {
    Some(match role {
        "normal" => 32512,
        "text" => 32513,
        "alternate" => 32516,
        "busy" => 32514,
        "precision" => 32515,
        "resize-nwse" => 32642,
        "resize-nesw" => 32643,
        "resize-ew" => 32644,
        "resize-ns" => 32645,
        "move" => 32646,
        "unavailable" => 32648,
        "link" => 32649,
        "working" => 32650,
        "help" => 32651,
        _ => return None,
    })
}
pub fn role_label(role: &str) -> &str {
    match role {
        "normal" => "Normal",
        "text" => "Texte",
        "help" => "Aide",
        "working" => "Travail en arrière-plan",
        "busy" => "Occupé",
        "precision" => "Précision",
        "handwriting" => "Écriture",
        "unavailable" => "Indisponible",
        "resize-ns" => "Redimensionnement vertical",
        "resize-ew" => "Redimensionnement horizontal",
        "resize-nesw" => "Diagonale ↗",
        "resize-nwse" => "Diagonale ↖",
        "move" => "Déplacement",
        "alternate" => "Sélection alternative",
        "link" => "Lien",
        "cursorfx-trail" => "Traînée",
        "cursorfx-effect" => "Effet au clic",
        _ => role,
    }
}
impl Sprite {
    /// Effects often start on a transparent frame; use their most visible frame for a card.
    pub fn preview_frame(&self) -> u32 {
        (0..self.entry.frame_count)
            .max_by_key(|i| {
                self.pixels
                    .view(
                        i * self.entry.frame_size.width,
                        0,
                        self.entry.frame_size.width,
                        self.entry.frame_size.height,
                    )
                    .pixels()
                    .map(|(_, _, p)| u64::from(p[3]))
                    .sum::<u64>()
            })
            .unwrap_or(0)
    }
    pub fn duration_ms(&self) -> u64 {
        self.entry
            .frame_durations_ms
            .iter()
            .map(|n| u64::from(*n))
            .sum()
    }
    pub fn frame_at(&self, elapsed_ms: u64) -> u32 {
        let mut t = elapsed_ms % self.duration_ms();
        for (i, ms) in self.entry.frame_durations_ms.iter().enumerate() {
            if t < u64::from(*ms) {
                return i as u32;
            }
            t -= u64::from(*ms);
        }
        0
    }
    pub fn frame(&self, index: u32) -> image::RgbaImage {
        self.pixels
            .view(
                (index % self.entry.frame_count) * self.entry.frame_size.width,
                0,
                self.entry.frame_size.width,
                self.entry.frame_size.height,
            )
            .to_image()
    }
    pub fn cur(&self, index: u32) -> Vec<u8> {
        let image = self.frame(index);
        let (w, h) = image.dimensions();
        let stride = w.div_ceil(32) * 4;
        let size = 40 + w * h * 4 + stride * h;
        let mut b = vec![0, 0, 2, 0, 1, 0, w as u8, h as u8, 0, 0];
        for n in [self.entry.hotspot.x, self.entry.hotspot.y] {
            b.extend_from_slice(&(n as u16).to_le_bytes());
        }
        for n in [size, 22, 40, w, h * 2] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&[0; 24]);
        for y in (0..h).rev() {
            for x in 0..w {
                let p = image.get_pixel(x, y).0;
                b.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        for y in (0..h).rev() {
            let mut mask = vec![0; stride as usize];
            for x in 0..w {
                if image.get_pixel(x, y)[3] == 0 {
                    mask[x as usize / 8] |= 0x80 >> (x % 8);
                }
            }
            b.extend(mask);
        }
        b
    }
    #[cfg(test)]
    pub fn windows_file(&self) -> Vec<u8> {
        self.windows_file_at(1.0)
    }
    pub fn windows_file_at(&self, speed: f32) -> Vec<u8> {
        if self.entry.frame_count == 1 {
            return self.cur(0);
        }
        fn chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut b = tag.to_vec();
            b.extend_from_slice(&(data.len() as u32).to_le_bytes());
            b.extend(data);
            if data.len() % 2 == 1 {
                b.push(0);
            }
            b
        }
        let n = self.entry.frame_count;
        let mut header = Vec::new();
        for value in [36, n, n, 0, 0, 32, 1, 1, 1] {
            header.extend_from_slice(&value.to_le_bytes());
        }
        let mut rates = Vec::new();
        for ms in &self.entry.frame_durations_ms {
            rates.extend_from_slice(
                &((*ms as f64 * 0.06 / f64::from(speed.clamp(0.25, 3.0))).round() as u32)
                    .max(1)
                    .to_le_bytes(),
            );
        }
        let mut frames = b"fram".to_vec();
        for i in 0..n {
            frames.extend(chunk(b"icon", &self.cur(i)));
        }
        let mut body = b"ACON".to_vec();
        body.extend(chunk(b"anih", &header));
        body.extend(chunk(b"rate", &rates));
        body.extend(chunk(b"LIST", &frames));
        chunk(b"RIFF", &body)
    }
}
use image::GenericImageView;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn fixture(edit: impl FnOnce(&mut serde_json::Value), path: &str) -> Vec<u8> {
        let mut m = serde_json::json!({"format":"zcursor","formatVersion":1,"name":"Test","cursors":[{
            "id":"normal","role":"normal","image":"cursors/test.png","layout":"horizontal-strip",
            "frameSize":{"width":2,"height":3},"frameCount":2,"frameDurationsMs":[50,150],
            "hotspot":{"x":1,"y":2},"scale":1.0}]});
        edit(&mut m);
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_fn(4, 3, |x, y| image::Rgba([10 * y as u8, x as u8, 30, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("theme.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
        zip.start_file(path, options).unwrap();
        zip.write_all(&png.into_inner()).unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn animation_hotspot_and_windows_container() {
        let p = Package::read(Cursor::new(fixture(|_| {}, "cursors/test.png"))).unwrap();
        let s = &p.cursors[0];
        assert_eq!(
            [
                s.frame_at(49),
                s.frame_at(50),
                s.frame_at(199),
                s.frame_at(200)
            ],
            [0, 1, 1, 0]
        );
        let cur = s.cur(0);
        assert_eq!(&cur[6..14], &[2, 3, 0, 0, 1, 0, 2, 0]);
        let ani = s.windows_file();
        assert_eq!(&ani[..4], b"RIFF");
        assert_eq!(&ani[8..12], b"ACON");
        assert_eq!(&ani[56..60], b"rate");
        assert_eq!(&ani[64..72], &[3, 0, 0, 0, 9, 0, 0, 0]);
        assert_eq!(
            u32::from_le_bytes(ani[4..8].try_into().unwrap()) as usize,
            ani.len() - 8
        );
        crate::native::check_package(&p).unwrap();
    }
    #[test]
    fn windows_roles_prefer_the_selected_scale_and_normal_state() {
        let p = Package::read(Cursor::new(fixture(
            |m| {
                let mut pressed = m["cursors"][0].clone();
                pressed["id"] = "pressed".into();
                pressed["source"] = serde_json::json!({"stateId":2});
                let mut large = m["cursors"][0].clone();
                large["id"] = "large".into();
                large["scale"] = 1.5.into();
                m["cursors"].as_array_mut().unwrap().insert(0, pressed);
                m["cursors"].as_array_mut().unwrap().push(large);
            },
            "cursors/test.png",
        )))
        .unwrap();
        assert_eq!(p.roles(1.0)[0].entry.id, "normal");
        assert_eq!(p.roles(1.5)[0].entry.id, "large");
    }
    #[test]
    fn orientation_migrates_legacy_archives_without_moving_hotspots() {
        for (source, expected) in [
            (serde_json::Value::Null, 0),
            (serde_json::json!({"format":"CursorFX"}), 20),
            (
                serde_json::json!({"format":"CursorFX","pngOrientation":"top-down"}),
                0,
            ),
        ] {
            let p = Package::read(Cursor::new(fixture(
                |m| {
                    if !source.is_null() {
                        m["source"] = source;
                    }
                },
                "cursors/test.png",
            )))
            .unwrap();
            let s = &p.cursors[0];
            assert_eq!(s.pixels.get_pixel(0, 0)[0], expected);
            assert_eq!((s.entry.hotspot.x, s.entry.hotspot.y), (1, 2));
            let cur = s.cur(0);
            assert_eq!(cur[64], s.pixels.get_pixel(0, 2)[0]);
        }
    }
    #[test]
    fn animation_speed_updates_the_native_frame_rates() {
        let p = Package::read(Cursor::new(fixture(|_| {}, "cursors/test.png"))).unwrap();
        let ani = p.cursors[0].windows_file_at(2.0);
        assert_eq!(&ani[64..72], &[2, 0, 0, 0, 5, 0, 0, 0]);
    }
    #[test]
    fn hostile_metadata_and_paths_are_rejected() {
        for (key, value) in [
            ("frameCount", serde_json::json!(0)),
            ("frameDurationsMs", serde_json::json!([0, 150])),
            ("hotspot", serde_json::json!({"x":2,"y":0})),
            ("image", serde_json::json!("../test.png")),
        ] {
            assert!(
                Package::read(Cursor::new(fixture(
                    |m| m["cursors"][0][key] = value,
                    "cursors/test.png"
                )))
                .is_err()
            );
        }
        assert!(
            Package::read(Cursor::new(fixture(
                |m| m["formatVersion"] = 2.into(),
                "cursors/test.png"
            )))
            .is_err()
        );
        assert!(Package::read(Cursor::new(fixture(|_| {}, "../test.png"))).is_err());
        assert!(
            Package::read(Cursor::new(fixture(
                |m| m["cursors"][0]["frameSize"]["width"] = 3.into(),
                "cursors/test.png"
            )))
            .is_err()
        );
    }
}

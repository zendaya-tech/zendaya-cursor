use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Shape {
    #[default]
    Arrow,
    Diamond,
    Cross,
    Ring,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: usize,
    pub shape: Shape,
    pub color: [u8; 3],
    pub size: u32,
    pub shadow: bool,
    pub trail: bool,
    pub trail_length: f32,
    pub clicks: bool,
    pub effect: usize,
    pub strength: f32,
    pub custom_png: Option<String>,
    pub hotspot: [u32; 2],
    pub zcursor: Option<String>,
    pub speed: f32,
    pub resolution: f64,
    pub profiles: std::collections::BTreeMap<String, ThemePreferences>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePreferences {
    pub speed: f32,
    pub resolution: f64,
    pub trail: bool,
    pub clicks: bool,
    pub trail_length: f32,
    pub strength: f32,
}
impl Default for ThemePreferences {
    fn default() -> Self {
        Self {
            speed: 1.0,
            resolution: 1.0,
            trail: true,
            clicks: true,
            trail_length: 0.35,
            strength: 1.0,
        }
    }
}
impl Settings {
    pub fn remember_theme(&mut self) {
        if let Some(path) = &self.zcursor {
            self.profiles.insert(
                path.clone(),
                ThemePreferences {
                    speed: self.speed,
                    resolution: self.resolution,
                    trail: self.trail,
                    clicks: self.clicks,
                    trail_length: self.trail_length,
                    strength: self.strength,
                },
            );
        }
    }
    pub fn theme_preferences(&mut self, path: &str) {
        let p = self.profiles.get(path).cloned().unwrap_or_default();
        self.speed = p.speed;
        self.resolution = p.resolution;
        self.trail = p.trail;
        self.clicks = p.clicks;
        self.trail_length = p.trail_length;
        self.strength = p.strength;
        self.sanitize();
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: 0,
            shape: Shape::Arrow,
            color: [167, 139, 250],
            size: 40,
            shadow: true,
            trail: true,
            trail_length: 0.35,
            clicks: true,
            effect: 0,
            strength: 1.0,
            custom_png: None,
            hotspot: [0, 0],
            zcursor: None,
            speed: 1.0,
            resolution: 1.0,
            profiles: Default::default(),
        }
    }
}
impl Settings {
    pub fn sanitize(&mut self) {
        self.speed = if self.speed.is_finite() {
            self.speed.clamp(0.25, 3.0)
        } else {
            1.0
        };
        self.resolution = if self.resolution.is_finite() {
            self.resolution.clamp(0.25, 8.0)
        } else {
            1.0
        };
        self.size = self.size.clamp(24, 96);
        self.theme = self.theme.min(THEMES.len() - 1);
        self.effect = self.effect.min(2);
        self.trail_length = if self.trail_length.is_finite() {
            self.trail_length.clamp(0.1, 0.8)
        } else {
            0.35
        };
        self.strength = if self.strength.is_finite() {
            self.strength.clamp(0.5, 2.0)
        } else {
            1.0
        };
        self.hotspot = self.hotspot.map(|v| v.min(self.size - 1));
    }
    pub fn preset(&mut self, i: usize) {
        let t = &THEMES[i.min(THEMES.len() - 1)];
        self.theme = i.min(THEMES.len() - 1);
        self.shape = t.shape;
        self.color = t.color;
        self.custom_png = None;
        self.zcursor = None;
        self.hotspot = [0, 0];
    }
}
pub struct Theme {
    pub shape: Shape,
    pub color: [u8; 3],
}
pub const THEMES: [Theme; 6] = [
    Theme {
        shape: Shape::Arrow,
        color: [167, 139, 250],
    },
    Theme {
        shape: Shape::Arrow,
        color: [103, 232, 249],
    },
    Theme {
        shape: Shape::Diamond,
        color: [251, 146, 110],
    },
    Theme {
        shape: Shape::Cross,
        color: [190, 242, 100],
    },
    Theme {
        shape: Shape::Ring,
        color: [249, 168, 212],
    },
    Theme {
        shape: Shape::Arrow,
        color: [226, 232, 240],
    },
];

pub fn polygon(shape: Shape) -> &'static [[f32; 2]] {
    match shape {
        Shape::Arrow => &[[0.12, 0.08], [0.81, 0.58], [0.49, 0.62], [0.36, 0.91]],
        Shape::Diamond => &[[0.16, 0.08], [0.79, 0.43], [0.52, 0.54], [0.43, 0.84]],
        _ => &[],
    }
}
fn inside(x: f32, y: f32, p: &[[f32; 2]]) -> bool {
    let mut yes = false;
    for i in 0..p.len() {
        let a = p[i];
        let b = p[(i + 1) % p.len()];
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            yes = !yes;
        }
    }
    yes
}
fn hit(shape: Shape, x: f32, y: f32) -> bool {
    match shape {
        Shape::Arrow | Shape::Diamond => inside(x, y, polygon(shape)),
        Shape::Ring => {
            let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt();
            (0.22..0.34).contains(&d)
        }
        Shape::Cross => {
            ((x - 0.5).abs() < 0.045 && (y - 0.5).abs() < 0.34)
                || ((y - 0.5).abs() < 0.045 && (x - 0.5).abs() < 0.34)
        }
    }
}
/// Supersampled original cursor art; RGBA is straight alpha for PNG and native conversion.
pub fn raster(settings: &Settings) -> Result<Vec<u8>, String> {
    if !(24..=96).contains(&settings.size) {
        return Err("Taille de curseur invalide.".into());
    }

    if let Some(path) = &settings.custom_png {
        let meta = std::fs::metadata(path).map_err(|e| format!("Image introuvable : {e}"))?;
        if meta.len() > 20 * 1024 * 1024 {
            return Err("Image trop volumineuse (20 Mo maximum).".into());
        }
        let reader = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return Err("L'image doit mesurer entre 1 et 4096 pixels par côté.".into());
        }
        let img = image::open(path).map_err(|e| e.to_string())?.resize_exact(
            settings.size,
            settings.size,
            image::imageops::FilterType::Lanczos3,
        );
        return Ok(img.into_rgba8().into_raw());
    }
    let n = settings.size as usize;
    if !(24..=96).contains(&n) {
        return Err("Taille de curseur invalide.".into());
    }
    let mut out = vec![0; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let mut count = 0;
            let mut edge = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.0) / n as f32;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.0) / n as f32;
                    if hit(settings.shape, px, py) {
                        count += 1;
                    }
                    if settings.shadow && hit(settings.shape, px - 0.025, py - 0.035) {
                        edge += 1;
                    }
                }
            }
            let i = (y * n + x) * 4;
            if count > 0 {
                out[i..i + 3].copy_from_slice(&settings.color);
                out[i + 3] = (count * 255 / 16) as u8;
            } else if edge > 0 {
                out[i..i + 3].copy_from_slice(&[9, 10, 18]);
                out[i + 3] = (edge * 150 / 16) as u8;
            }
        }
    }
    Ok(out)
}
pub fn hotspot(s: &Settings) -> [u32; 2] {
    if s.custom_png.is_some() {
        s.hotspot.map(|v| v.min(s.size - 1))
    } else if matches!(s.shape, Shape::Cross | Shape::Ring) {
        [s.size / 2, s.size / 2]
    } else {
        [(s.size as f32 * 0.12) as u32, (s.size as f32 * 0.08) as u32]
    }
}
/// Windows CUR with a 32-bit DIB and an AND mask (no proprietary theme format).
pub fn cur_bytes(s: &Settings, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let n = s.size as usize;
    if !(24..=96).contains(&n) || rgba.len() != n * n * 4 {
        return Err("Pixels invalides.".into());
    }
    let stride = n.div_ceil(32) * 4;
    let length = 40 + n * n * 4 + stride * n;
    let mut out = vec![0, 0, 2, 0, 1, 0, s.size as u8, s.size as u8, 0, 0];
    for v in hotspot(s) {
        out.extend_from_slice(&(v as u16).to_le_bytes());
    }
    out.extend_from_slice(&(length as u32).to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&s.size.to_le_bytes());
    out.extend_from_slice(&(s.size * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..n).rev() {
        for x in 0..n {
            let p = &rgba[(y * n + x) * 4..][..4];
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    for y in (0..n).rev() {
        let mut row = vec![0u8; stride];
        for x in 0..n {
            if rgba[(y * n + x) * 4 + 3] == 0 {
                row[x / 8] |= 0x80 >> (x % 8);
            }
        }
        out.extend(row);
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_are_kept_separately_for_each_zcursor_theme() {
        let mut s = Settings {
            zcursor: Some("a.zcursor".into()),
            speed: 2.0,
            clicks: false,
            ..Settings::default()
        };
        s.remember_theme();
        s.theme_preferences("b.zcursor");
        assert_eq!(s.speed, 1.0);
        assert!(s.clicks);
        s.theme_preferences("a.zcursor");
        assert_eq!(s.speed, 2.0);
        assert!(!s.clicks);
        let json = serde_json::to_vec(&s).unwrap();
        let mut restored: Settings = serde_json::from_slice(&json).unwrap();
        restored.theme_preferences("a.zcursor");
        assert_eq!(restored.speed, 2.0);
    }
    #[test]
    fn every_theme_has_alpha_and_transparency() {
        for i in 0..THEMES.len() {
            let mut s = Settings::default();
            s.preset(i);
            let b = raster(&s).unwrap();
            assert_eq!(b.len(), 40 * 40 * 4);
            assert!(b.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
            assert!(b.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
        }
    }
    #[test]
    fn exported_cursor_has_valid_hotspot_and_dib_dimensions() {
        let s = Settings::default();
        let b = cur_bytes(&s, &raster(&s).unwrap()).unwrap();
        assert_eq!(&b[..6], &[0, 0, 2, 0, 1, 0]);
        assert_eq!(u32::from_le_bytes(b[18..22].try_into().unwrap()), 22);
        assert_eq!(u32::from_le_bytes(b[22..26].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(b[26..30].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(b[30..34].try_into().unwrap()), 80);
    }
    #[test]
    fn corrupt_settings_are_bounded() {
        let mut s = Settings {
            size: u32::MAX,
            theme: usize::MAX,
            strength: f32::NAN,
            ..Settings::default()
        };
        s.sanitize();
        assert_eq!(s.size, 96);
        assert!(s.strength.is_finite());
    }
}

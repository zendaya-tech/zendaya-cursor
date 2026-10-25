#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod model;
mod native;
mod overlay;
mod zcursor;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Vec2};
use model::Settings;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
const BG: Color32 = Color32::from_rgb(13, 16, 25);
const PANEL: Color32 = Color32::from_rgb(20, 24, 36);
const CARD: Color32 = Color32::from_rgb(26, 31, 45);
const INK: Color32 = Color32::from_rgb(235, 238, 247);
const MUTED: Color32 = Color32::from_rgb(139, 149, 171);
const ACCENT: Color32 = Color32::from_rgb(167, 139, 250);
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Themes,
    Effects,
    Settings,
}
struct Thumbnail {
    name: String,
    texture: Option<egui::TextureHandle>,
    description: String,
}
#[derive(Clone)]
struct Dot {
    pos: Pos2,
    at: Instant,
}
struct App {
    s: Settings,
    page: Page,
    session: native::CursorSession,
    recovery: native::Recovery,
    effects: overlay::DesktopEffects,
    desktop_effects: bool,
    status: String,
    error: bool,
    package: Option<Arc<zcursor::Package>>,
    package_index: usize,
    package_clock: Instant,
    applied_key: String,
    applied_package: Option<Arc<zcursor::Package>>,
    applied_settings: Option<Settings>,
    library: Vec<PathBuf>,
    thumbs: HashMap<PathBuf, Thumbnail>,
    search: String,
    textures: HashMap<(usize, u32, u16), egui::TextureHandle>,
    trail: VecDeque<Dot>,
    clicks: Vec<Dot>,
    snapshot: Option<PathBuf>,
    frames: usize,
    snapshot_requested: bool,
}
fn config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ZendayaCursor/settings.json")
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, snapshot: Option<PathBuf>) -> Self {
        let mut s = std::fs::read(config_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default();
        s.sanitize();
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.override_text_color = Some(INK);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        style.visuals.widgets.inactive.bg_fill = CARD;
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 49, 69);
        style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.45);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.slider_width = 130.0;
        style.spacing.button_padding = Vec2::new(15.0, 10.0);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let recovery = native::Recovery::new();
        let effects = overlay::DesktopEffects::new(recovery.signal());
        let startup = s.zcursor.clone();
        let mut app = Self {
            s,
            page: Page::Themes,
            session: Default::default(),
            recovery,
            effects,
            desktop_effects: false,
            status: "Choisissez un thème ZCursor.".into(),
            error: false,
            package: None,
            package_index: 0,
            package_clock: Instant::now(),
            applied_key: String::new(),
            applied_package: None,
            applied_settings: None,
            library: Vec::new(),
            thumbs: HashMap::new(),
            search: String::new(),
            textures: HashMap::new(),
            trail: VecDeque::new(),
            clicks: Vec::new(),
            snapshot,
            frames: 0,
            snapshot_requested: false,
        };
        app.refresh_library();
        if let Some(path) = startup {
            app.load_theme(PathBuf::from(path));
        }
        if app.package.is_none()
            && let Some(path) = app.library.first().cloned()
        {
            app.load_theme(path);
        }
        app
    }
    fn refresh_library(&mut self) {
        let mut dirs = vec![
            config_path().with_file_name("themes"),
            PathBuf::from("dist/zcursor"),
        ];
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            dirs.push(parent.join("themes"));
            dirs.push(parent.join("../../dist/zcursor"));
        }
        self.library.clear();
        self.thumbs.clear();
        for dir in dirs {
            if let Ok(files) = std::fs::read_dir(dir) {
                for file in files.flatten().take(256) {
                    let path = file.path();
                    if path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("zcursor"))
                        && let Ok(path) = path.canonicalize()
                    {
                        self.library.push(path);
                    }
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        self.library.retain(|p| {
            seen.insert(
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase(),
            )
        });
        self.library.sort_by_key(|p| {
            p.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        });
    }
    fn load_theme(&mut self, path: PathBuf) {
        match zcursor::Package::load(&path) {
            Ok(package) => {
                self.s.remember_theme();
                let path = path
                    .canonicalize()
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                self.s.theme_preferences(&path);
                self.s.zcursor = Some(path);
                self.s.custom_png = None;
                self.status = format!(
                    "{} · {} curseurs · {} effets",
                    package.name,
                    package.cursors.len(),
                    package.effects.len()
                );
                self.error = false;
                self.package = Some(Arc::new(package));
                self.package_index = 0;
                self.choose_scale();
                self.package_clock = Instant::now();
                self.textures.clear();
                self.trail.clear();
                self.clicks.clear();
            }
            Err(e) => {
                if self.package.is_none() {
                    self.s.zcursor = None;
                }
                self.status = e;
                self.error = true;
            }
        }
    }
    fn selected_sprite(&self) -> Option<&zcursor::Sprite> {
        self.package
            .as_ref()?
            .cursors
            .iter()
            .chain(&self.package.as_ref()?.effects)
            .nth(self.package_index)
    }
    fn choose_scale(&mut self) {
        if let Some(package) = &self.package {
            let role = self.selected_sprite().map(|s| s.entry.role.clone());
            if let Some((i, _)) = package
                .cursors
                .iter()
                .chain(&package.effects)
                .enumerate()
                .filter(|(_, s)| role.as_ref().is_none_or(|r| r == &s.entry.role))
                .min_by(|(_, a), (_, b)| {
                    (a.entry.scale - self.s.resolution)
                        .abs()
                        .total_cmp(&(b.entry.scale - self.s.resolution).abs())
                })
            {
                self.package_index = i;
            }
        }
    }
    fn import_theme(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Thèmes ZCursor", &["zcursor"])
            .pick_file()
        {
            let result = (|| -> Result<PathBuf, String> {
                zcursor::Package::load(&path)?;
                let dir = config_path().with_file_name("themes");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let target = dir.join(path.file_name().ok_or("Nom de fichier invalide.")?);
                if path.canonicalize().ok() != target.canonicalize().ok() {
                    std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
                }
                Ok(target)
            })();
            match result {
                Ok(path) => {
                    self.load_theme(path);
                    self.refresh_library();
                }
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            }
        }
    }
    fn result(&mut self, r: Result<(), String>, success: &str) {
        match r {
            Ok(()) => {
                self.status = success.into();
                self.error = false;
            }
            Err(e) => {
                self.status = e;
                self.error = true;
            }
        }
    }
    fn restore(&mut self) {
        self.desktop_effects = false;
        self.effects.set(None);
        self.effects.set_package(None);
        self.applied_package = None;
        self.applied_settings = None;
        let r = self.session.reset();
        self.result(r, "Curseurs Windows restaurés.");
    }
    fn save(&mut self) {
        self.s.remember_theme();
        let path = config_path();
        let r = (|| -> Result<(), String> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&self.s).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })();
        self.result(r, "Réglages du thème enregistrés.");
    }
    fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.s.zcursor.as_deref().unwrap_or(""),
            self.s.resolution,
            self.s.speed,
            self.s.trail,
            self.s.clicks,
            self.s.trail_length,
            self.s.strength
        )
    }
    fn apply(&mut self) {
        let Some(package) = self.package.clone() else {
            return;
        };
        let r = self
            .session
            .apply_package(&package, self.s.resolution, self.s.speed);
        if r.is_ok() {
            self.s.remember_theme();
            self.applied_key = self.key();
            self.desktop_effects = !package.effects.is_empty() && (self.s.trail || self.s.clicks);
            self.applied_package = Some(package);
            self.applied_settings = Some(self.s.clone());
        } else {
            self.desktop_effects = false;
            self.applied_package = None;
            self.applied_settings = None;
        }
        self.result(r, "Thème ZCursor appliqué · Ctrl + Alt + R pour restaurer.");
    }
    fn header(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("VOTRE SOURIS. VOTRE STYLE.")
                .size(11.0)
                .color(ACCENT),
        );
        ui.add_space(8.0);
        ui.heading(
            RichText::new(match self.page {
                Page::Themes => "Vos thèmes ZCursor.",
                Page::Effects => "Les effets de votre thème.",
                Page::Settings => "Votre curseur, vos réglages.",
            })
            .size(30.0),
        );
        ui.label(
            RichText::new("Choisissez, prévisualisez et appliquez votre thème au bureau.")
                .color(MUTED),
        );
        ui.add_space(22.0);
    }
    fn thumbnail(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let entry = match zcursor::Package::load(path) {
            Ok(package) => {
                let sprite = package
                    .cursors
                    .iter()
                    .find(|s| s.entry.role == "normal")
                    .or_else(|| package.cursors.first())
                    .or_else(|| package.effects.first());
                let texture = sprite.map(|s| {
                    let im = s.frame(s.preview_frame());
                    let im = image::imageops::thumbnail(&im, 100, 100);
                    ctx.load_texture(
                        path.to_string_lossy(),
                        egui::ColorImage::from_rgba_unmultiplied(
                            [im.width() as usize, im.height() as usize],
                            im.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    )
                });
                Thumbnail {
                    name: package.name,
                    texture,
                    description: if package.cursors.is_empty() {
                        format!(
                            "{} effet{}",
                            package.effects.len(),
                            if package.effects.len() > 1 { "s" } else { "" }
                        )
                    } else {
                        format!(
                            "{} curseurs · {} effets",
                            package.cursors.len(),
                            package.effects.len()
                        )
                    },
                }
            }
            Err(_) => Thumbnail {
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                texture: None,
                description: "Archive à vérifier".into(),
            },
        };
        self.thumbs.insert(path.to_owned(), entry);
    }
    fn theme_cards(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Importer un .zcursor").clicked() {
                self.import_theme();
            }
            if ui.button("Actualiser").clicked() {
                self.refresh_library();
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Rechercher un thème")
                    .desired_width(210.0),
            );
        });
        ui.add_space(14.0);
        ui.label(RichText::new(format!("{} thèmes ZCursor", self.library.len())).color(MUTED));
        ui.add_space(8.0);
        if self.library.is_empty() {
            ui.label("Importez votre premier fichier .zcursor pour commencer.");
            return;
        }
        let query = self.search.to_lowercase();
        let paths: Vec<_> = self
            .library
            .iter()
            .filter(|p| {
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&query)
                    || self
                        .thumbs
                        .get(*p)
                        .is_some_and(|t| t.name.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        let columns = if ui.available_width() > 600.0 { 3 } else { 2 };
        let width = (ui.available_width() - 12.0 * (columns - 1) as f32) / columns as f32;
        let mut loaded = false;
        for row in paths.chunks(columns) {
            ui.horizontal(|ui| {
                for path in row {
                    let (r, response) =
                        ui.allocate_exact_size(Vec2::new(width, 160.0), egui::Sense::click());
                    if ui.is_rect_visible(r) && !self.thumbs.contains_key(path) && !loaded {
                        self.thumbnail(path, ui.ctx());
                        loaded = true;
                    }
                    let selected =
                        self.s.zcursor.as_deref() == Some(path.to_string_lossy().as_ref());
                    let p = ui.painter_at(r);
                    p.rect_filled(
                        r,
                        12.0,
                        if selected {
                            Color32::from_rgb(39, 33, 60)
                        } else {
                            CARD
                        },
                    );
                    p.rect_stroke(
                        r,
                        12.0,
                        Stroke::new(
                            1.2,
                            if selected {
                                ACCENT
                            } else {
                                Color32::from_rgb(43, 49, 65)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let fallback = path.file_stem().unwrap_or_default().to_string_lossy();
                    let name = self
                        .thumbs
                        .get(path)
                        .map_or(fallback.as_ref(), |t| t.name.as_str());
                    if let Some(thumb) = self.thumbs.get(path) {
                        if let Some(tex) = &thumb.texture {
                            let size = tex.size_vec2();
                            let target = size * (78.0 / size.x.max(size.y)).min(1.5);
                            p.image(
                                tex.id(),
                                Rect::from_center_size(
                                    r.center_top() + Vec2::new(0.0, 55.0),
                                    target,
                                ),
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        p.text(
                            r.left_bottom() + Vec2::new(14.0, -18.0),
                            egui::Align2::LEFT_BOTTOM,
                            &thumb.description,
                            egui::FontId::proportional(10.0),
                            MUTED,
                        );
                    } else {
                        p.text(
                            r.center_top() + Vec2::new(0.0, 55.0),
                            egui::Align2::CENTER_CENTER,
                            "…",
                            egui::FontId::proportional(25.0),
                            MUTED,
                        );
                    }
                    let short: String = name.chars().take(25).collect();
                    p.text(
                        r.left_bottom() + Vec2::new(14.0, -41.0),
                        egui::Align2::LEFT_BOTTOM,
                        short,
                        egui::FontId::proportional(15.0),
                        INK,
                    );
                    if response.on_hover_text(name).clicked() {
                        self.load_theme(path.clone());
                    }
                }
            });
        }
    }
    fn options(&mut self, ui: &mut egui::Ui) {
        let Some(package) = self.package.clone() else {
            ui.label("Sélectionnez d'abord un thème ZCursor.");
            return;
        };
        ui.heading(&package.name);
        ui.label("Ces réglages concernent uniquement le thème sélectionné.");
        ui.add_space(16.0);
        let trail = package
            .effects
            .iter()
            .any(|s| s.entry.role == "cursorfx-trail");
        let clicks = package
            .effects
            .iter()
            .any(|s| s.entry.role != "cursorfx-trail");
        if package.effects.is_empty() {
            egui::Frame::new().fill(CARD).corner_radius(12).inner_margin(20).show(ui,|ui|{
            ui.label("Ce thème ne contient pas d'effets de clic ni de traînée.");
            ui.label("Ses curseurs animés restent disponibles dans l'aperçu. Réglez leur vitesse dans Préférences.");});
            return;
        }
        let old_strength = self.s.strength;
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(12)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.add_enabled_ui(trail, |ui| {
                    ui.checkbox(&mut self.s.trail, "Traînée du thème");
                    ui.add(
                        egui::Slider::new(&mut self.s.trail_length, 0.1..=0.8)
                            .text("Persistance · secondes"),
                    );
                });
                ui.add_enabled_ui(clicks, |ui| {
                    ui.checkbox(&mut self.s.clicks, "Animations du thème au clic");
                });
                ui.add(egui::Slider::new(&mut self.s.strength, 0.5..=2.0).text("Intensité"));
                ui.label("Déplacez la souris et cliquez dans l'aperçu pour essayer.");
            });
        ui.add_space(20.0);
        if old_strength != self.s.strength {
            self.textures.clear();
        }
        for sprite in &package.effects {
            ui.label(format!(
                "{} · {} images",
                zcursor::role_label(&sprite.entry.role),
                sprite.entry.frame_count
            ));
        }
        ui.add_space(12.0);
        ui.label("Appliquez le thème pour activer ces réglages sur le bureau.");
    }
    fn preferences(&mut self, ui: &mut egui::Ui) {
        let Some(package) = self.package.clone() else {
            ui.label("Sélectionnez d'abord un thème ZCursor.");
            return;
        };
        ui.heading(&package.name);
        if let Some(author) = &package.author {
            ui.label(RichText::new(author).color(MUTED));
        }
        ui.add_space(16.0);
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(12)
            .inner_margin(20)
            .show(ui, |ui| {
                let mut scales: Vec<_> = package
                    .cursors
                    .iter()
                    .chain(&package.effects)
                    .map(|s| s.entry.scale)
                    .collect();
                scales.sort_by(f64::total_cmp);
                scales.dedup();
                let old = self.s.resolution;
                egui::ComboBox::from_label("Résolution du thème")
                    .selected_text(format!("{} %", (self.s.resolution * 100.0).round()))
                    .show_ui(ui, |ui| {
                        for scale in scales {
                            ui.selectable_value(
                                &mut self.s.resolution,
                                scale,
                                format!("{} %", (scale * 100.0).round()),
                            );
                        }
                    });
                if old != self.s.resolution {
                    self.choose_scale();
                }
                ui.add(
                    egui::Slider::new(&mut self.s.speed, 0.25..=3.0)
                        .text("Vitesse des animations · ×"),
                );
                ui.label("Les proportions et les points de clic du thème sont conservés.");
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Enregistrer pour ce thème").clicked() {
                        self.save();
                    }
                    if ui.button("Réinitialiser ce thème").clicked()
                        && let Some(path) = self.s.zcursor.clone()
                    {
                        self.s.profiles.remove(&path);
                        self.s.theme_preferences(&path);
                        self.choose_scale();
                    }
                });
            });
        ui.add_space(20.0);
        ui.label(RichText::new("Ctrl + Alt + R").strong().color(ACCENT));
        ui.label("Restaure les curseurs Windows et coupe les effets.");
        ui.label(
            "Le thème reste sélectionné au démarrage. Son application au bureau reste manuelle.",
        );
        ui.add_space(16.0);
        ui.label(concat!("Zendaya Cursor · ", env!("CARGO_PKG_VERSION")));
    }
    fn sprite_texture(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        sprite: &zcursor::Sprite,
        frame: u32,
        opacity: f32,
    ) -> egui::TextureHandle {
        self.textures
            .entry((index, frame, (opacity * 1000.0).round() as u16))
            .or_insert_with(|| {
                let mut image = sprite.frame(frame);
                for pixel in image.pixels_mut() {
                    pixel[3] = (f32::from(pixel[3]) * opacity).round().clamp(0.0, 255.0) as u8;
                }
                ctx.load_texture(
                    format!("sprite-{index}-{frame}"),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                )
            })
            .clone()
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(PANEL)
            .corner_radius(16)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("APERÇU DU THÈME SÉLECTIONNÉ")
                        .size(10.0)
                        .color(MUTED),
                );
                let Some(package) = self.package.clone() else {
                    ui.label("Aucun thème sélectionné.");
                    return;
                };
                ui.heading(&package.name);
                let old = self.package_index;
                egui::ComboBox::from_id_salt("role")
                    .width(ui.available_width())
                    .selected_text(
                        self.selected_sprite()
                            .map(|s| {
                                format!(
                                    "{} · ×{}",
                                    zcursor::role_label(&s.entry.role),
                                    s.entry.scale
                                )
                            })
                            .unwrap_or_default(),
                    )
                    .show_ui(ui, |ui| {
                        for (i, sprite) in
                            package.cursors.iter().chain(&package.effects).enumerate()
                        {
                            ui.selectable_value(
                                &mut self.package_index,
                                i,
                                format!(
                                    "{} · ×{} · {} images",
                                    zcursor::role_label(&sprite.entry.role),
                                    sprite.entry.scale,
                                    sprite.entry.frame_count
                                ),
                            );
                        }
                    });
                if old != self.package_index
                    && let Some(sprite) = self.selected_sprite()
                {
                    self.s.resolution = sprite.entry.scale;
                }
                let (r, response) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 230.0),
                    egui::Sense::click(),
                );
                let p = ui.painter_at(r);
                p.rect_filled(r, 10.0, BG);
                let pos = response.hover_pos().unwrap_or(r.center());
                let now = Instant::now();
                if response.hovered() && self.trail.back().is_none_or(|d| d.pos.distance(pos) > 2.0)
                {
                    self.trail.push_back(Dot { pos, at: now });
                }
                while self.trail.len() > 80
                    || self
                        .trail
                        .front()
                        .is_some_and(|d| d.at.elapsed().as_secs_f32() > self.s.trail_length)
                {
                    self.trail.pop_front();
                }
                if response.clicked() || response.secondary_clicked() {
                    if self.clicks.len() >= 16 {
                        self.clicks.remove(0);
                    }
                    self.clicks.push(Dot { pos, at: now });
                }
                self.clicks.retain(|d| d.at.elapsed().as_secs_f32() < 30.0);
                for (i, sprite) in package.effects.iter().enumerate().take(8) {
                    let dots: Vec<_> = if sprite.entry.role == "cursorfx-trail" {
                        if self.s.trail {
                            self.trail
                                .iter()
                                .rev()
                                .step_by(4)
                                .take(20)
                                .cloned()
                                .collect()
                        } else {
                            Vec::new()
                        }
                    } else if self.s.clicks {
                        self.clicks.clone()
                    } else {
                        Vec::new()
                    };
                    for dot in dots {
                        let age =
                            (dot.at.elapsed().as_millis() as f64 * f64::from(self.s.speed)) as u64;
                        if age >= sprite.duration_ms() {
                            continue;
                        }
                        let frame = sprite.frame_at(age);
                        let tex = self.sprite_texture(
                            ui.ctx(),
                            package.cursors.len() + i,
                            sprite,
                            frame,
                            self.s.strength,
                        );
                        let hot = sprite.entry.hotspot;
                        let size = sprite.entry.frame_size;
                        p.image(
                            tex.id(),
                            Rect::from_min_size(
                                dot.pos - Vec2::new(hot.x as f32, hot.y as f32),
                                Vec2::new(size.width as f32, size.height as f32),
                            ),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                }
                if let Some(sprite) = package
                    .cursors
                    .iter()
                    .chain(&package.effects)
                    .nth(self.package_index)
                {
                    let frame = sprite.frame_at(
                        (self.package_clock.elapsed().as_millis() as f64 * f64::from(self.s.speed))
                            as u64,
                    );
                    let tex = self.sprite_texture(ui.ctx(), self.package_index, sprite, frame, 1.0);
                    let hot = sprite.entry.hotspot;
                    let size = sprite.entry.frame_size;
                    p.image(
                        tex.id(),
                        Rect::from_min_size(
                            pos - Vec2::new(hot.x as f32, hot.y as f32),
                            Vec2::new(size.width as f32, size.height as f32),
                        ),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    ui.label(format!(
                        "{} × {} px · {} image{}",
                        size.width,
                        size.height,
                        sprite.entry.frame_count,
                        if sprite.entry.frame_count > 1 {
                            "s"
                        } else {
                            ""
                        }
                    ));
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                }
                ui.label(
                    RichText::new("Bougez et cliquez ici pour essayer.")
                        .size(12.0)
                        .color(MUTED),
                );
            });
    }
    fn overlay(&mut self) {
        self.recovery
            .set_active(self.session.applied || self.desktop_effects);
        if self.recovery.take_emergency() {
            self.desktop_effects = false;
            self.session.applied = false;
            self.applied_package = None;
            self.applied_settings = None;
            self.status = "Curseurs restaurés (Ctrl + Alt + R).".into();
            self.error = false;
        }
        if let Some(error) = self.effects.take_error() {
            self.desktop_effects = false;
            self.status = error;
            self.error = true;
        }
        self.effects.set_package(if self.desktop_effects {
            self.applied_package.clone()
        } else {
            None
        });
        self.effects.set(if self.desktop_effects {
            self.applied_settings.clone()
        } else {
            None
        });
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        self.s.sanitize();
        self.overlay();
        egui::Panel::left("sidebar")
            .exact_size(190.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(22))
            .show(ui, |ui| {
                ui.label(RichText::new("↖").size(38.0).color(ACCENT));
                ui.label(RichText::new("ZENDAYA").size(17.0).strong());
                ui.label("C U R S O R");
                ui.add_space(40.0);
                for (page, label) in [
                    (Page::Themes, "Mes curseurs"),
                    (Page::Effects, "Effets"),
                    (Page::Settings, "Préférences"),
                ] {
                    if ui
                        .add_sized(
                            [146.0, 43.0],
                            egui::Button::new(label).selected(self.page == page),
                        )
                        .clicked()
                    {
                        self.page = page;
                    }
                }
            });
        egui::Panel::bottom("actions")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(24, 15)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 420.0).max(140.0));
                        ui.label(
                            RichText::new(if self.applied_package.is_some() {
                                if self.applied_key != self.key() {
                                    "Modifications à appliquer"
                                } else {
                                    "Votre thème est actif"
                                }
                            } else {
                                "Prêt à personnaliser"
                            })
                            .strong(),
                        );
                        ui.label(RichText::new(&self.status).size(11.0).color(if self.error {
                            Color32::LIGHT_RED
                        } else {
                            MUTED
                        }));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                self.package.is_some(),
                                egui::Button::new(
                                    RichText::new("Appliquer au bureau ↗").color(BG).strong(),
                                )
                                .fill(ACCENT),
                            )
                            .clicked()
                        {
                            self.apply();
                        }
                        if ui.button("Restaurer Windows").clicked() {
                            self.restore();
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(28))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.header(ui);
                    let width = ui.available_width();
                    if width > 740.0 {
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(width - 324.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| match self.page {
                                    Page::Themes => self.theme_cards(ui),
                                    Page::Effects => self.options(ui),
                                    Page::Settings => self.preferences(ui),
                                },
                            );
                            ui.allocate_ui_with_layout(
                                Vec2::new(300.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| self.inspector(ui),
                            );
                        });
                    } else {
                        match self.page {
                            Page::Themes => self.theme_cards(ui),
                            Page::Effects => self.options(ui),
                            Page::Settings => self.preferences(ui),
                        }
                        ui.add_space(20.0);
                        self.inspector(ui);
                    }
                });
            });
        if self.snapshot.is_some() {
            if self.frames > 15 && !self.snapshot_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.snapshot_requested = true;
            }
            let screenshot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot
                && let Some(path) = self.snapshot.take()
            {
                let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("Capture : {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn on_exit(&mut self) {
        self.desktop_effects = false;
        self.effects.set(None);
        if self.session.applied {
            let _ = self.session.reset();
        }
        self.save();
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|s| s == "--restore") {
        if let Err(e) = native::restore() {
            eprintln!("{e}");
            std::process::exit(1)
        }
        return Ok(());
    }
    if args.iter().any(|s| s == "--self-test") {
        if let Err(e) = native::self_test() {
            eprintln!("{e}");
            std::process::exit(1)
        }
        println!("Win32 : 6 thèmes et 6 fichiers .cur validés. Aucun curseur système modifié.");
        return Ok(());
    }
    if let Some(i) = args.iter().position(|s| s == "--check-zcursor") {
        let result = (|| -> Result<(), String> {
            let path = args
                .get(i + 1)
                .map(PathBuf::from)
                .ok_or("Fichier ou dossier requis.")?;
            let paths = if path.is_dir() {
                std::fs::read_dir(path)
                    .map_err(|e| e.to_string())?
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("zcursor"))
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![path]
            };
            if paths.is_empty() {
                return Err("Aucun fichier .zcursor.".into());
            }
            for path in paths {
                let package = zcursor::Package::load(&path)?;
                native::check_package(&package)?;
                println!(
                    "OK {} : {} curseurs, {} effets",
                    package.name,
                    package.cursors.len(),
                    package.effects.len()
                );
            }
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let initial_theme = args
        .iter()
        .position(|s| s == "--theme")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| {
            args.get(1)
                .filter(|s| s.to_ascii_lowercase().ends_with(".zcursor"))
                .map(PathBuf::from)
        });
    let snapshot = args
        .iter()
        .position(|s| s == "--snapshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Zendaya Cursor")
        .with_inner_size([1280.0, 860.0])
        .with_min_inner_size([900.0, 700.0])
        .with_transparent(true);
    if let Ok(rgba) = model::raster(&Settings::default()) {
        viewport = viewport.with_icon(egui::IconData {
            rgba,
            width: 40,
            height: 40,
        });
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Zendaya Cursor",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc, snapshot);
            if let Some(i) = args.iter().position(|s| s == "--page") {
                app.page = match args.get(i + 1).map(String::as_str) {
                    Some("effects") => Page::Effects,
                    Some("settings") => Page::Settings,
                    _ => Page::Themes,
                };
            }
            if let Some(path) = initial_theme {
                app.load_theme(path);
            }
            Ok(Box::new(app))
        }),
    )
}

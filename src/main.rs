#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod model;
mod native;
mod overlay;
mod zcursor;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Vec2};
use model::{Settings, THEMES};
use std::{
    collections::VecDeque,
    path::PathBuf,
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
    Studio,
    Settings,
}
struct Pulse {
    p: Pos2,
    at: Instant,
    right: bool,
}
#[derive(Default)]
struct Field {
    trail: VecDeque<(Pos2, Instant)>,
    pulses: Vec<Pulse>,
}
impl Field {
    fn push(&mut self, p: Pos2, click: Option<bool>, s: &Settings) {
        let now = Instant::now();
        if self
            .trail
            .back()
            .is_none_or(|(old, _)| old.distance(p) > 1.0)
        {
            self.trail.push_back((p, now));
        }
        while self.trail.len() > 160
            || self
                .trail
                .front()
                .is_some_and(|(_, at)| now.duration_since(*at).as_secs_f32() > s.trail_length)
        {
            self.trail.pop_front();
        }
        self.pulses
            .retain(|v| now.duration_since(v.at).as_secs_f32() < 0.8);
        if s.clicks
            && let Some(right) = click
        {
            if self.pulses.len() >= 24 {
                self.pulses.remove(0);
            }
            self.pulses.push(Pulse { p, at: now, right });
        }
    }
    fn paint(&self, p: &egui::Painter, s: &Settings, origin: Pos2, scale: f32) {
        let now = Instant::now();
        let tint = s.tint();
        if s.trail {
            for (pos, at) in &self.trail {
                let age = now.duration_since(*at).as_secs_f32() / s.trail_length;
                let a = (1.0 - age).clamp(0.0, 1.0);
                let pos = origin + pos.to_vec2() * scale;
                p.circle_filled(
                    pos,
                    (2.0 + 5.0 * a) * scale * s.strength,
                    tint.gamma_multiply(a * 0.45),
                );
            }
        }
        for v in &self.pulses {
            let t = now.duration_since(v.at).as_secs_f32() / 0.8;
            let fade = (1.0 - t).max(0.0);
            let c = if v.right {
                Color32::from_rgb(103, 232, 249)
            } else {
                tint
            };
            let pos = origin + v.p.to_vec2() * scale;
            let r = (8.0 + 50.0 * t) * scale * s.strength;
            match s.effect {
                1 => {
                    for i in 0..10 {
                        let a = i as f32 * std::f32::consts::TAU / 10.0;
                        let dir = Vec2::angled(a);
                        p.line_segment(
                            [pos + dir * r * 0.65, pos + dir * r],
                            Stroke::new(2.0 * scale, c.gamma_multiply(fade)),
                        );
                    }
                }
                2 => {
                    p.circle_filled(pos, r, c.gamma_multiply(fade * 0.13));
                    p.circle_stroke(pos, r, Stroke::new(2.0 * scale, c.gamma_multiply(fade)));
                }
                _ => {
                    p.circle_stroke(pos, r, Stroke::new(2.5 * scale, c.gamma_multiply(fade)));
                    p.circle_stroke(
                        pos,
                        r * 0.65,
                        Stroke::new(scale, c.gamma_multiply(fade * 0.5)),
                    );
                }
            }
        }
    }
}
struct App {
    s: Settings,
    page: Page,
    session: native::CursorSession,
    recovery: native::Recovery,
    status: String,
    error: bool,
    desktop_effects: bool,
    effects: overlay::DesktopEffects,
    preview: Field,

    texture: Option<egui::TextureHandle>,
    texture_key: String,
    applied_key: String,
    snapshot: Option<PathBuf>,
    frames: usize,
    snapshot_requested: bool,
    package: Option<std::sync::Arc<zcursor::Package>>,
    package_index: usize,
    package_clock: Instant,
    library: Vec<PathBuf>,
}
fn config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ZendayaCursor")
        .join("settings.json")
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
            status: "Choisissez un thème pour commencer.".into(),
            error: false,
            desktop_effects: false,
            effects,
            preview: Default::default(),

            texture: None,
            texture_key: String::new(),
            applied_key: String::new(),
            snapshot,
            frames: 0,
            snapshot_requested: false,
            package: None,
            package_index: 0,
            package_clock: Instant::now(),
            library: Vec::new(),
        };
        app.refresh_library();
        if let Some(path) = startup {
            app.load_theme(PathBuf::from(path));
        }
        app
    }
    fn refresh_library(&mut self) {
        self.library.clear();
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
        self.library.sort();
        self.library.dedup();
    }
    fn load_theme(&mut self, path: PathBuf) {
        match zcursor::Package::load(&path) {
            Ok(package) => {
                self.status = format!(
                    "{} : {} curseurs, {} effets chargés.",
                    package.name,
                    package.cursors.len(),
                    package.effects.len()
                );
                self.error = false;
                self.s.zcursor = Some(path.to_string_lossy().into_owned());
                self.s.custom_png = None;
                self.package = Some(std::sync::Arc::new(package));
                self.package_index = 0;
                self.package_clock = Instant::now();
                self.texture_key.clear();
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
        let package = self.package.as_ref().filter(|_| self.s.zcursor.is_some())?;
        package
            .cursors
            .iter()
            .chain(&package.effects)
            .nth(self.package_index)
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
                let name = path.file_name().ok_or("Nom de fichier invalide.")?;
                let target = dir.join(name);
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
                self.error = false
            }
            Err(e) => {
                self.status = e;
                self.error = true
            }
        }
    }
    fn restore(&mut self) {
        self.desktop_effects = false;
        self.effects.set(None);
        let r = self.session.reset();
        self.result(r, "Curseurs Windows restaurés.");
    }
    fn save(&mut self) {
        let path = config_path();
        let r = (|| -> Result<(), String> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&self.s).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        self.result(r, "Préférences enregistrées.");
    }
    fn key(&self) -> String {
        format!(
            "{}:{}",
            serde_json::to_string(&self.s).unwrap_or_default(),
            self.package_index
        )
    }
    fn texture(&mut self, ctx: &egui::Context) {
        let frame = self
            .selected_sprite()
            .map(|s| s.frame_at(self.package_clock.elapsed().as_millis() as u64));
        let key = format!("{}:{}:{frame:?}", self.key(), self.package_index);
        if key == self.texture_key {
            return;
        }
        self.texture_key = key;
        let result = if let Some(sprite) = self.selected_sprite() {
            let image = sprite.frame(frame.unwrap_or(0));
            Ok((
                [image.width() as usize, image.height() as usize],
                image.into_raw(),
            ))
        } else {
            model::raster(&self.s).map(|bytes| ([self.s.size as usize; 2], bytes))
        };
        match result {
            Ok((size, bytes)) => {
                self.texture = Some(ctx.load_texture(
                    "cursor-preview",
                    egui::ColorImage::from_rgba_unmultiplied(size, &bytes),
                    egui::TextureOptions::LINEAR,
                ))
            }
            Err(e) => {
                self.texture = None;
                self.status = e;
                self.error = true;
            }
        }
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("VOTRE SOURIS. VOTRE STYLE.")
                        .size(11.0)
                        .color(ACCENT)
                        .strong(),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(match self.page {
                        Page::Themes => "Le détail qui change tout.",
                        Page::Effects => "Donnez du mouvement.",
                        Page::Studio => "Créez votre signature.",
                        Page::Settings => "À votre rythme.",
                    })
                    .size(30.0)
                    .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Des curseurs qui vous ressemblent, jusque sur le bureau.")
                        .color(MUTED),
                );
            });
        });
        ui.add_space(26.0);
    }
    fn theme_cards(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Thèmes ZCursor");
            if ui.button("+ Importer un .zcursor").clicked() {
                self.import_theme();
            }
            if ui.button("Actualiser").clicked() {
                self.refresh_library();
            }
        });
        if self.library.is_empty() {
            ui.label("Importez une archive .zcursor pour utiliser ses curseurs et animations.");
        }
        let paths = self.library.clone();
        egui::ScrollArea::vertical()
            .id_salt("zcursor-library")
            .max_height(160.0)
            .min_scrolled_height(140.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for path in paths {
                    let name = path.file_stem().unwrap_or_default().to_string_lossy();
                    let selected =
                        self.s.zcursor.as_deref() == Some(path.to_string_lossy().as_ref());
                    if ui.selectable_label(selected, name).clicked() {
                        self.load_theme(path);
                    }
                }
            });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("La collection").size(18.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("06 THÈMES ORIGINAUX").size(10.0).color(MUTED));
            });
        });
        ui.add_space(8.0);
        let columns = if ui.available_width() > 510.0 { 3 } else { 2 };
        let width = (ui.available_width() - (columns - 1) as f32 * 12.0) / columns as f32;
        for row in 0..6usize.div_ceil(columns) {
            ui.horizontal(|ui| {
                for col in 0..columns {
                    let i = row * columns + col;
                    if i >= 6 {
                        break;
                    }
                    let theme = &THEMES[i];
                    let (r, response) =
                        ui.allocate_exact_size(Vec2::new(width, 162.0), egui::Sense::click());
                    let selected = self.s.theme == i
                        && self.s.custom_png.is_none()
                        && self.s.zcursor.is_none();
                    let fill = if selected {
                        Color32::from_rgb(39, 33, 60)
                    } else if response.hovered() {
                        Color32::from_rgb(32, 37, 53)
                    } else {
                        CARD
                    };
                    let p = ui.painter();
                    p.rect_filled(r, 12.0, fill);
                    p.rect_stroke(
                        r,
                        12.0,
                        Stroke::new(
                            if selected { 1.5 } else { 1.0 },
                            if selected {
                                ACCENT
                            } else {
                                Color32::from_rgb(43, 49, 65)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let mut sample = self.s.clone();
                    sample.preset(i);
                    model::draw_cursor(p, r.center_top() + Vec2::new(-30.0, 20.0), 70.0, &sample);
                    p.text(
                        r.left_bottom() + Vec2::new(16.0, -42.0),
                        egui::Align2::LEFT_BOTTOM,
                        theme.name,
                        egui::FontId::proportional(16.0),
                        INK,
                    );
                    p.text(
                        r.left_bottom() + Vec2::new(16.0, -20.0),
                        egui::Align2::LEFT_BOTTOM,
                        theme.label,
                        egui::FontId::proportional(9.0),
                        MUTED,
                    );
                    if selected {
                        p.circle_filled(r.right_top() + Vec2::new(-18.0, 18.0), 4.0, ACCENT);
                    }
                    if response.on_hover_text("Sélectionner ce thème").clicked() {
                        self.s.preset(i);
                    }
                }
            });
            ui.add_space(2.0);
        }
        ui.add_space(16.0);
        egui::Frame::new()
            .fill(PANEL)
            .corner_radius(12)
            .inner_margin(18)
            .show(ui, |ui| {
                ui.label(RichText::new("Une idée bien à vous ?").strong());
                ui.label(
                    RichText::new("Importez une image PNG et créez votre propre curseur.")
                        .color(MUTED),
                );
                if ui.button("Ouvrir l'atelier").clicked() {
                    self.page = Page::Studio;
                }
            });
    }
    fn options(&mut self, ui: &mut egui::Ui) {
        ui.heading("Le mouvement, à votre façon");
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(12)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.checkbox(&mut self.s.trail, "Traînée lumineuse");
                ui.label(RichText::new("Une trace douce qui suit les déplacements.").color(MUTED));
                ui.add(
                    egui::Slider::new(&mut self.s.trail_length, 0.1..=0.8).text("Durée · secondes"),
                );
                ui.add_space(18.0);
                ui.checkbox(&mut self.s.clicks, "Effets au clic");
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in ["Ondes", "Étincelles", "Halo"].iter().enumerate() {
                        ui.selectable_value(&mut self.s.effect, i, *name);
                    }
                });
                ui.add(egui::Slider::new(&mut self.s.strength, 0.5..=2.0).text("Intensité"));
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Clic gauche : couleur du thème · clic droit : cyan")
                        .color(MUTED)
                        .size(12.0),
                );
            });
        ui.add_space(22.0);
        ui.label(RichText::new("Essayez ici, puis appliquez au bureau.").size(18.0));
        ui.label(
            RichText::new("Déplacez la souris et cliquez dans la zone d'aperçu à droite.")
                .color(MUTED),
        );
    }
    fn import(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png"])
            .pick_file()
        {
            let mut next = self.s.clone();
            next.zcursor = None;
            next.custom_png = Some(path.to_string_lossy().into_owned());
            next.hotspot = [0, 0];
            match model::raster(&next) {
                Ok(_) => {
                    self.s = next;
                    self.status = "Image importée. Réglez son point actif dans l'atelier.".into();
                    self.error = false
                }
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            }
        }
    }
    fn studio(&mut self, ui: &mut egui::Ui) {
        ui.heading("L'atelier de curseurs");
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(12)
            .inner_margin(22)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Votre image, votre pointeur.")
                        .size(21.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "PNG transparent conseillé. Le point actif indique où le clic se produit.",
                    )
                    .color(MUTED),
                );
                ui.add_space(18.0);
                if ui.button("+  Importer un PNG").clicked() {
                    self.import();
                }
                if let Some(path) = &self.s.custom_png {
                    ui.label(
                        PathBuf::from(path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    );
                    ui.add_space(14.0);
                    let max = self.s.size - 1;
                    ui.add(
                        egui::Slider::new(&mut self.s.hotspot[0], 0..=max).text("Point actif X"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.s.hotspot[1], 0..=max).text("Point actif Y"),
                    );
                }
                ui.add_space(18.0);
                if ui.button("Exporter le curseur .cur  ↗").clicked() {
                    self.export();
                }
                ui.label(
                    RichText::new("Compatible avec les propriétés de la souris Windows.")
                        .color(MUTED)
                        .size(12.0),
                );
            });
        ui.add_space(22.0);
        ui.label(RichText::new("Bon à savoir").strong());
        ui.label("Le point actif des formes anneau et réticule est au centre. Celui des flèches se trouve à la pointe.");
    }
    fn export(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Curseur Windows", &["cur"])
            .set_file_name("zendaya-cursor.cur")
            .save_file()
        {
            let r = if let Some(sprite) = self.selected_sprite() {
                Ok(sprite.cur(sprite.frame_at(self.package_clock.elapsed().as_millis() as u64)))
            } else {
                model::raster(&self.s).and_then(|p| model::cur_bytes(&self.s, &p))
            }
            .and_then(|b| std::fs::write(path, b).map_err(|e| e.to_string()));
            self.result(r, "Curseur .cur exporté.");
        }
    }
    fn preferences(&mut self, ui: &mut egui::Ui) {
        ui.heading("Simple, dès le départ.");
        ui.add_space(14.0);
        egui::Frame::new().fill(CARD).corner_radius(12).inner_margin(22).show(ui,|ui|{
      ui.label(RichText::new("Ctrl + Alt + R").size(23.0).strong().color(ACCENT));ui.label("Restaure les curseurs Windows et coupe les effets du bureau.");ui.add_space(18.0);
      ui.label("Les préférences sont conservées à la fermeture. Le thème système reste un choix explicite à chaque ouverture.");ui.add_space(14.0);
      if ui.button("Enregistrer les préférences").clicked(){self.save();}
      ui.add_space(14.0);ui.label(RichText::new("Dossier des préférences").color(MUTED));ui.label(config_path().display().to_string());
    });
        ui.add_space(24.0);
        ui.label(
            RichText::new(concat!("Zendaya Cursor  /  ", env!("CARGO_PKG_VERSION")))
                .size(16.0)
                .strong(),
        );
        ui.label(RichText::new("Application native en Rust · Windows 10 / 11").color(MUTED));
        ui.label("Thèmes et graphismes originaux. Projet indépendant inspiré des utilitaires de personnalisation du curseur.");
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(PANEL).corner_radius(16).inner_margin(20).show(ui,|ui| {
            ui.label(RichText::new("APERÇU EN DIRECT").size(10.0).color(MUTED));
            if let Some(package)=self.package.as_ref().filter(|_|self.s.zcursor.is_some()) {
                ui.heading(&package.name);
                if let Some(author)=&package.author {ui.label(author);}
                egui::ComboBox::from_id_salt("zcursor-role").selected_text(self.selected_sprite().map(|s|format!("{} · ×{}",zcursor::role_label(&s.entry.role),s.entry.scale)).unwrap_or_else(||"Curseur".into())).show_ui(ui,|ui| {
                    for (i,s) in package.cursors.iter().chain(&package.effects).enumerate() {
                        ui.selectable_value(&mut self.package_index,i,format!("{} · ×{} · {} images",zcursor::role_label(&s.entry.role),s.entry.scale,s.entry.frame_count));
                    }
                });
            }
            let (r,response)=ui.allocate_exact_size(Vec2::new(ui.available_width(),240.0),egui::Sense::click());
            let p=ui.painter_at(r); p.rect_filled(r,10.0,BG);
            let pos=response.hover_pos().unwrap_or(r.center());
            let sprite=self.selected_sprite();
            let (size,hot)=if let Some(sprite)=sprite {
                (Vec2::new(sprite.entry.frame_size.width as f32,sprite.entry.frame_size.height as f32),[sprite.entry.hotspot.x,sprite.entry.hotspot.y])
            }else{(Vec2::splat(self.s.size as f32),model::hotspot(&self.s))};
            if self.s.zcursor.is_none() {
                self.preview.push((pos-r.min).to_pos2(),if response.clicked(){Some(false)}else if response.secondary_clicked(){Some(true)}else{None},&self.s);
                self.preview.paint(&p,&self.s,r.min,1.0);
            }
            if let Some(tex)=&self.texture { p.image(tex.id(),Rect::from_min_size(pos-Vec2::new(hot[0] as f32,hot[1] as f32),size),Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0)),Color32::WHITE); }
            if response.hovered(){ui.ctx().set_cursor_icon(egui::CursorIcon::None);}
            if self.s.zcursor.is_some() {
                ui.label("Dimensions et point actif du thème conservés.");
                ui.label("Choisissez une variante ci-dessus pour l'aperçu et son échelle d'application.");
                ui.label("Les effets convertis utilisent les animations au clic et en traînée ; certains réglages CursorFX ne sont pas transposés.");
            } else {
                ui.heading(if self.s.custom_png.is_some(){"Votre création"}else{THEMES[self.s.theme].name});
                ui.add_enabled_ui(self.s.custom_png.is_none(),|ui|{ui.color_edit_button_srgb(&mut self.s.color);ui.checkbox(&mut self.s.shadow,"Ombre douce");});
                ui.add(egui::Slider::new(&mut self.s.size,24..=96).text("Taille · px"));
            }
        });
    }
    fn overlay(&mut self, _ctx: &egui::Context) {
        self.recovery
            .set_active(self.session.applied || self.desktop_effects);
        if self.recovery.take_emergency() {
            self.desktop_effects = false;
            self.session.applied = false;
            self.status = "Curseurs Windows restaurés (Ctrl + Alt + R).".into();
            self.error = false;
        }
        if let Some(e) = self.effects.take_error() {
            self.desktop_effects = false;
            self.status = e;
            self.error = true;
        }
        self.effects
            .set_package(if self.desktop_effects && self.s.zcursor.is_some() {
                self.package.clone()
            } else {
                None
            });
        self.effects.set(if self.desktop_effects {
            Some(self.s.clone())
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
        self.texture(&ctx);
        self.overlay(&ctx);
        egui::Panel::left("sidebar")
            .exact_size(190.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(22))
            .show(ui, |ui| {
                ui.add_space(13.0);
                ui.label(RichText::new("↖").size(38.0).color(ACCENT));
                ui.label(RichText::new("ZENDAYA").strong().size(17.0));
                ui.label(RichText::new("C U R S O R").size(11.0).color(MUTED));
                ui.add_space(42.0);
                for (page, label) in [
                    (Page::Themes, "       Mes curseurs"),
                    (Page::Effects, "       Effets"),
                    (Page::Studio, "       L'atelier"),
                    (Page::Settings, "       Préférences"),
                ] {
                    let selected = self.page == page;
                    let response = ui.add_sized(
                        [146.0, 43.0],
                        egui::Button::new(RichText::new(label).color(if selected {
                            ACCENT
                        } else {
                            MUTED
                        }))
                        .fill(if selected {
                            Color32::from_rgb(42, 34, 64)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(8),
                    );
                    let c = response.rect.left_center() + Vec2::new(19.0, 0.0);
                    let color = if selected { ACCENT } else { MUTED };
                    let stroke = Stroke::new(1.5, color);
                    let p = ui.painter();
                    match page {
                        Page::Themes => {
                            p.add(egui::Shape::closed_line(
                                vec![
                                    c + Vec2::new(0.0, -6.0),
                                    c + Vec2::new(6.0, 0.0),
                                    c + Vec2::new(0.0, 6.0),
                                    c + Vec2::new(-6.0, 0.0),
                                ],
                                stroke,
                            ));
                        }
                        Page::Effects => {
                            for i in 0..4 {
                                let d = Vec2::angled(i as f32 * std::f32::consts::FRAC_PI_2);
                                p.line_segment([c + d * 2.0, c + d * 7.0], stroke);
                            }
                        }
                        Page::Studio => {
                            p.line_segment(
                                [c - Vec2::new(6.0, 0.0), c + Vec2::new(6.0, 0.0)],
                                stroke,
                            );
                            p.line_segment(
                                [c - Vec2::new(0.0, 6.0), c + Vec2::new(0.0, 6.0)],
                                stroke,
                            );
                        }
                        Page::Settings => {
                            p.circle_stroke(c, 5.0, stroke);
                            p.circle_filled(c, 1.5, color);
                        }
                    }
                    if response.clicked() {
                        self.page = page;
                    }
                }
                ui.add_space((ui.available_height() - 95.0).max(0.0));
                ui.separator();
                ui.label(
                    RichText::new("PETIT DÉTAIL.\nGRANDE DIFFÉRENCE.")
                        .size(10.0)
                        .color(MUTED),
                );
                ui.label(
                    RichText::new("Natif. Fluide. À vous.")
                        .size(11.0)
                        .color(ACCENT),
                );
            });
        egui::Panel::bottom("actions")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(26, 15)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let changed = (self.session.applied || self.desktop_effects)
                        && self.applied_key != self.key();
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 450.0).max(130.0));
                        ui.label(
                            RichText::new(if changed {
                                "Modifications à appliquer"
                            } else if self.session.applied || self.desktop_effects {
                                "Votre thème est actif"
                            } else {
                                "Prêt à personnaliser"
                            })
                            .strong(),
                        );
                        ui.label(RichText::new(&self.status).size(11.0).color(if self.error {
                            Color32::from_rgb(251, 146, 146)
                        } else {
                            MUTED
                        }));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Appliquer au bureau  ↗").strong().color(BG),
                                )
                                .fill(ACCENT)
                                .corner_radius(8),
                            )
                            .clicked()
                        {
                            let r = if let Some(package) =
                                self.package.as_ref().filter(|_| self.s.zcursor.is_some())
                            {
                                self.session.apply_package(
                                    package,
                                    self.selected_sprite().map_or(1.0, |s| s.entry.scale),
                                )
                            } else {
                                self.session.apply(&self.s)
                            };
                            if r.is_ok() {
                                self.applied_key = self.key();
                                self.desktop_effects = self.s.trail || self.s.clicks;
                            }
                            self.result(r, "Thème appliqué · Ctrl + Alt + R pour restaurer.");
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
                        let right = 300.0;
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(width - right - 24.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| match self.page {
                                    Page::Themes => self.theme_cards(ui),
                                    Page::Effects => self.options(ui),
                                    Page::Studio => self.studio(ui),
                                    Page::Settings => self.preferences(ui),
                                },
                            );
                            ui.allocate_ui_with_layout(
                                Vec2::new(right, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    self.inspector(ui);
                                },
                            );
                        });
                    } else {
                        match self.page {
                            Page::Themes => self.theme_cards(ui),
                            Page::Effects => self.options(ui),
                            Page::Studio => self.studio(ui),
                            Page::Settings => self.preferences(ui),
                        }
                        ui.add_space(20.0);
                        self.inspector(ui);
                    }
                });
            });
        if self.snapshot.is_some() {
            if self.frames > 8 && !self.snapshot_requested {
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
        ctx.request_repaint_after(Duration::from_millis(if self.desktop_effects {
            16
        } else {
            33
        }));
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn on_exit(&mut self) {
        self.desktop_effects = false;
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
            if let Some(path) = initial_theme {
                app.load_theme(path);
            }
            Ok(Box::new(app))
        }),
    )
}

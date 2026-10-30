//! Books: the original help files (`help/*.HLP`), shown as a shelf of manuals.

use crate::panel::Plate;
use eframe::egui::{self, Color32, RichText, Vec2};
use gmdss::help::{self, HelpFile, Topic};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Books {
    shelf: crate::svg_art::Art,
    files: Vec<(String, PathBuf)>,
    open: Option<(usize, HelpFile)>,
    topic: usize,
    /// Topics to go back to.
    back: Vec<usize>,
    /// The open book's pictures, as they are drawn.
    textures: HashMap<usize, egui::TextureHandle>,
}

impl Books {
    /// `help_dir` is the original `help` folder (next to `data`).
    pub fn new(help_dir: Option<PathBuf>) -> Books {
        let mut files: Vec<(String, PathBuf)> = help_dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("hlp")))
            .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p))
            .collect();
        files.sort();
        Books { shelf: crate::svg_art::Art::new("shelf", include_str!("../assets/panels/shelf.svg")), files, open: None, topic: 0, back: vec![], textures: HashMap::new() }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self.files.is_empty() {
            ui.label("No help files found (expected the original 'help' folder next to 'data').");
            return;
        }
        // The manuals stand on shelves, twelve to a shelf; click one to open it.
        const PER_SHELF: usize = 12;
        const COLOURS: [Color32; 6] = [
            Color32::from_rgb(28, 70, 40),
            Color32::from_rgb(30, 60, 140),
            Color32::from_rgb(214, 110, 40),
            Color32::from_rgb(40, 120, 80),
            Color32::from_rgb(25, 40, 100),
            Color32::from_rgb(120, 30, 30),
        ];
        let mut clicked = None;
        for (row, chunk) in self.files.chunks(PER_SHELF).enumerate() {
            let p = Plate::with_art(ui, "Books", &mut self.shelf, 760.0);
            let space = p.place(self.shelf.rect("space"));
            let w = space.width() / PER_SHELF as f32;
            for (k, (name, _)) in chunk.iter().enumerate() {
                let i = row * PER_SHELF + k;
                let open = self.open.as_ref().is_some_and(|o| o.0 == i);
                let h = space.height() * (0.82 + 0.06 * ((i * 7) % 4) as f32 / 3.0);
                let spine = egui::Rect::from_min_size(egui::pos2(space.left() + w * k as f32 + 1.0, space.bottom() - h - if open { 6.0 } else { 0.0 }), egui::vec2(w - 2.0, h));
                let c = COLOURS[i % COLOURS.len()];
                let painter = ui.painter();
                painter.rect_filled(spine, 2.0, c);
                painter.rect_filled(spine.shrink2(egui::vec2(0.0, 6.0)).with_max_x(spine.left() + 3.0), 0.0, Color32::from_white_alpha(25));
                for y in [spine.top() + 8.0, spine.bottom() - 10.0] {
                    painter.hline(spine.x_range(), y, egui::Stroke::new(1.5, Color32::from_rgb(210, 180, 60)));
                }
                let galley = painter.layout_no_wrap(name.clone(), egui::FontId::proportional((w * 0.42).min(15.0)), Color32::from_rgb(235, 220, 160));
                let pos = egui::pos2(spine.center().x - galley.size().y / 2.0, spine.center().y + galley.size().x / 2.0);
                painter.add(egui::epaint::TextShape::new(pos, galley, Color32::WHITE).with_angle(-std::f32::consts::FRAC_PI_2));
                if p.spot(ui, name, spine.translate(-p.rect.min.to_vec2()) / (p.rect.width() / 613.0)).0 {
                    clicked = Some(i);
                }
            }
        }
        if let Some(i) = clicked {
            self.load(i);
        }
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            let Some((_, h)) = &self.open else {
                ui.label("Choose a book.");
                return;
            };
            let mut go = None;
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.back.is_empty(), egui::Button::new("< Back")).clicked() {
                        self.topic = self.back.pop().unwrap();
                    }
                    if ui.button("Contents").clicked() {
                        go = Some(0);
                    }
                });
                ui.label(RichText::new("TOPICS").small().color(Color32::GRAY));
                egui::ScrollArea::vertical().id_salt("topics").max_height(540.0).show(ui, |ui| {
                    for (i, t) in h.topics.iter().enumerate() {
                        let title: String = t.title.chars().take(34).collect();
                        if ui.selectable_label(self.topic == i, title).clicked() {
                            go = Some(i);
                        }
                    }
                });
            });
            if let Some(t) = h.topics.get(self.topic) {
                egui::Frame::new().fill(PAPER).inner_margin(12).show(ui, |ui| {
                    egui::ScrollArea::both().id_salt(("text", self.topic)).max_height(560.0).auto_shrink([false, false]).show(ui, |ui| {
                        if let Some(l) = page(ui, h, t, &mut self.textures) {
                            go = Some(l);
                        }
                    });
                });
            }
            if let Some(i) = go.filter(|&i| i != self.topic) {
                self.back.push(self.topic);
                self.topic = i;
            }
        });
    }

    fn load(&mut self, i: usize) {
        let h = std::fs::read(&self.files[i].1).ok().and_then(|b| help::parse(&b)).unwrap_or(HelpFile { topics: vec![], pictures: vec![] });
        self.open = Some((i, h));
        self.topic = 0;
        self.back.clear();
        self.textures.clear();
    }

    /// Opens `file` (e.g. `vhf_dsc.hlp`) at the topic named `topic`, for a lesson's `help` line.
    pub fn open_topic(&mut self, file: &str, topic: &str) -> bool {
        let stem = file.split('.').next().unwrap_or(file);
        let Some(i) = self.files.iter().position(|(n, _)| n.eq_ignore_ascii_case(stem)) else { return false };
        if self.open.as_ref().is_none_or(|o| o.0 != i) {
            self.load(i);
        }
        let Some(t) = self.open.as_ref().and_then(|(_, h)| h.named(topic)) else { return false };
        self.topic = t;
        true
    }
}

const PAPER: Color32 = Color32::from_rgb(245, 243, 230);

/// One help topic as the original lays it out: a character grid (tabs every 8 columns) with the
/// links in it and the pictures drawn over it, one picture pixel to an eighth of a character.
/// Returns the topic of the link clicked.
fn page(ui: &mut egui::Ui, h: &HelpFile, t: &Topic, textures: &mut HashMap<usize, egui::TextureHandle>) -> Option<usize> {
    let font = egui::FontId::monospace(13.0);
    let (cw, lh) = ui.fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
    let link_of = |i: usize| t.links.iter().position(|l| (l.start..l.start + l.len).contains(&i));
    // Runs of text on one line in one link (or none): (line, column, text, link).
    let mut runs: Vec<(usize, usize, String, Option<usize>)> = vec![];
    let mut placed = vec![];
    let (mut line, mut col, mut width) = (0, 0, 0);
    let mut chars = 0;
    for (i, c) in t.text.chars().enumerate() {
        placed.extend(t.pictures.iter().filter(|p| p.0 == i).map(|p| (line, col, p.1)));
        match c {
            '\n' => (line, col) = (line + 1, 0),
            '\t' => col = (col / 8 + 1) * 8,
            _ => {
                let link = link_of(i);
                match runs.last_mut() {
                    Some(r) if r.0 == line && r.3 == link && r.1 + r.2.chars().count() == col => r.2.push(c),
                    _ => runs.push((line, col, c.to_string(), link)),
                }
                col += 1;
            }
        }
        width = width.max(col);
        chars = i + 1;
    }
    placed.extend(t.pictures.iter().filter(|p| p.0 >= chars).map(|p| (line, col, p.1)));
    let s = cw / 8.0;
    let pic_size = |p: usize| h.pictures.get(p).map_or(Vec2::ZERO, |p| Vec2::new(p.width as f32, p.height as f32) * s);
    let mut size = Vec2::new(width as f32 * cw, (line + 1) as f32 * lh);
    for &(l, c, p) in &placed {
        size = size.max(Vec2::new(c as f32 * cw, l as f32 * lh) + pic_size(p));
    }
    let (rect, _) = ui.allocate_exact_size(size.max(Vec2::new(ui.available_width(), 0.0)), egui::Sense::hover());
    let at = |l: usize, c: usize| rect.min + Vec2::new(c as f32 * cw, l as f32 * lh);
    let mut go = None;
    for (k, (l, c, text, link)) in runs.iter().enumerate() {
        let Some(link) = link else {
            ui.painter().text(at(*l, *c), egui::Align2::LEFT_TOP, text, font.clone(), Color32::from_rgb(20, 20, 40));
            continue;
        };
        let r = egui::Rect::from_min_size(at(*l, *c), Vec2::new(text.chars().count() as f32 * cw, lh));
        let resp = ui.interact(r, ui.id().with(("link", k)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let colour = if resp.hovered() { Color32::from_rgb(170, 30, 30) } else { Color32::from_rgb(20, 60, 160) };
        ui.painter().text(r.min, egui::Align2::LEFT_TOP, text, font.clone(), colour);
        ui.painter().hline(r.x_range(), r.bottom() - 1.0, egui::Stroke::new(1.0, colour));
        if resp.clicked() {
            go = Some(t.links[*link].target);
        }
    }
    for &(l, c, p) in &placed {
        let Some(pic) = h.pictures.get(p).filter(|p| p.width > 0) else { continue };
        let tex = textures.entry(p).or_insert_with(|| {
            let rgba: Vec<u8> = pic
                .pixels
                .iter()
                .flat_map(|&i| if i == help::CLEAR { [0; 4] } else { let [r, g, b] = help::PALETTE[usize::from(i & 15)]; [r, g, b, 255] })
                .collect();
            ui.ctx().load_texture(format!("help-picture-{p}"), egui::ColorImage::from_rgba_unmultiplied([pic.width, pic.height], &rgba), egui::TextureOptions::NEAREST)
        });
        let r = egui::Rect::from_min_size(at(l, c), pic_size(p));
        ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
    go
}

#[cfg(test)]
mod tests {
    /// The shelves with the original help files, when they are present.
    #[test]
    fn shelf_snapshot() {
        let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../_gmdss/help"));
        if !dir.is_dir() {
            return;
        }
        let mut books = super::Books::new(Some(dir.clone()));
        crate::snapshot::render("books", eframe::egui::vec2(780.0, 560.0), |ui| books.show(ui));
        // Topics with links and pictures: the HF keys, a SART manual page, a lesson's topic.
        for (file, topic, name) in [("hf.hlp", "-", "books_hf"), ("sart.hlp", "1b", "books_sart"), ("vhf_dsc.hlp", "Cdc", "books_cdc")] {
            let mut books = super::Books::new(Some(dir.clone())); // each snapshot has its own egui context
            if !books.open_topic(file, topic) {
                books.topic = 9;
            }
            crate::snapshot::render(name, eframe::egui::vec2(1000.0, 1300.0), |ui| books.show(ui));
        }
    }
}

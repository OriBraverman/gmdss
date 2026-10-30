//! Books: the original help files (`help/*.HLP`), shown as a shelf of manuals.

use crate::panel::Plate;
use eframe::egui::{self, Color32, RichText, Vec2};
use gmdss::help::{self, HelpFile, Topic};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Books {
    shelf_ref: crate::svg_art::Art,
    /// The map of coast stations was taken from the shelf: show the chart.
    pub chart_request: bool,
    files: Vec<(String, PathBuf)>,
    open: Option<(usize, HelpFile)>,
    topic: usize,
    /// Topics to go back to.
    back: Vec<usize>,
    /// The open book is shown (else the shelves).
    reading: bool,
    /// The open book's pictures, as they are drawn.
    textures: HashMap<usize, egui::TextureHandle>,
    /// Russian pages in English, where a translation is present (the default).
    pub english: bool,
    /// The open book has a translation (`help_en/<BOOK>.txt` beside `help`).
    translated: bool,
    /// The open book's table of contents page.
    contents: Option<usize>,
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
        Books {
            chart_request: false,
            shelf_ref: crate::svg_art::Art::new("shelf_reference", include_str!("../assets/panels/shelf_reference.svg")),
            files, open: None, reading: false, topic: 0, back: vec![], textures: HashMap::new(), english: true, translated: false, contents: None }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self.files.is_empty() {
            ui.label("No help files found (expected the original 'help' folder next to 'data').");
            return;
        }
        if self.reading && self.open.is_some() {
            self.reader(ui);
        } else {
            self.shelves(ui);
        }
    }

    /// The shelf, as on the original (SHELF.CPP): the reference books, the map of coast stations
    /// (which opens the chart), the self-education book; the equipment manuals open from their
    /// devices. Click a book to read it.
    fn shelves(&mut self, ui: &mut egui::Ui) {
        let find = |name: &str| self.files.iter().position(|(n, _)| n.eq_ignore_ascii_case(name));
        let opened = self.open.as_ref().map(|o| o.0);
        let width = ui.available_width().min((ui.available_height() - 30.0).max(200.0) * 613.0 / 390.0);
        let mut clicked = None;
        ui.vertical_centered(|ui| {
            ui.set_width(width);
            let p = Plate::with_art(ui, "Books", &mut self.shelf_ref, width);
            for (k, file) in REFERENCE.iter().enumerate() {
                let (clicked_now, r) = p.spot(ui, &format!("book-{k}"), self.shelf_ref.rect(&format!("book-{k}")));
                let r = r.on_hover_text(if *file == "Chart" { "Map of coast stations: the chart" } else { title_of(file) });
                let i = find(file);
                if i.is_some() && opened == i {
                    ui.painter().rect_stroke(r.rect, 2.0, egui::Stroke::new(2.5, crate::panel::YELLOW), egui::StrokeKind::Outside);
                }
                if clicked_now {
                    if *file == "Chart" {
                        self.chart_request = true;
                    } else {
                        clicked = i;
                    }
                }
            }
            ui.label(RichText::new("The equipment manuals open with the Manual buttons of each device.").small().color(Color32::GRAY));
        });
        if let Some(i) = clicked {
            if opened != Some(i) {
                self.load(i);
            }
            self.reading = true;
        }
    }

    /// Opens help file `file` (e.g. `vhf.hlp`) at its contents, for a device's Manual button.
    pub fn open_file(&mut self, file: &str) -> bool {
        let stem = file.split('.').next().unwrap_or(file);
        let Some(i) = self.files.iter().position(|(n, _)| n.eq_ignore_ascii_case(stem)) else { return false };
        if self.open.as_ref().is_none_or(|o| o.0 != i) {
            self.load(i);
        }
        self.topic = self.open.as_ref().and_then(|o| o.1.root).or(self.contents).unwrap_or(0);
        self.reading = true;
        true
    }

    /// An open book: its topics on the left, the page on the right, the whole height of the page.
    /// An open book, as the original's help window: the page, with Next, Prev, Back, Root and
    /// Contents (the table of contents page) above it.
    fn reader(&mut self, ui: &mut egui::Ui) {
        let Some((i, h)) = &self.open else { return };
        let mut go = None;
        let mut close = false;
        let mut relanguage = None;
        let here = h.topics.get(self.topic);
        ui.horizontal(|ui| {
            if ui.button("\u{25c0} Shelf").clicked() {
                close = true;
            }
            ui.label(RichText::new(title_of(&self.files[*i].0)).size(17.0).strong().color(Color32::from_rgb(235, 220, 160)));
            ui.add_space(16.0);
            for (label, to) in [("Next", here.and_then(|t| t.next)), ("Prev", here.and_then(|t| t.prev))] {
                if ui.add_enabled(to.is_some(), egui::Button::new(label)).clicked() {
                    go = to;
                }
            }
            if ui.add_enabled(!self.back.is_empty(), egui::Button::new("Back")).clicked() {
                self.topic = self.back.pop().unwrap();
            }
            for (label, to) in [("Root", h.root), ("Contents", self.contents)] {
                if ui.add_enabled(to.is_some(), egui::Button::new(label)).clicked() {
                    go = to;
                }
            }
            if self.translated {
                ui.add_space(16.0);
                for (english, label) in [(true, "English"), (false, "\u{420}\u{443}\u{441}\u{441}\u{43a}\u{438}\u{439}")] {
                    if ui.selectable_label(self.english == english, label).clicked() && self.english != english {
                        relanguage = Some(english);
                    }
                }
            }
        });
        ui.add_space(6.0);
        let height = (ui.available_height() - 8.0).max(200.0);
        if let Some(t) = h.topics.get(self.topic) {
            egui::Frame::new().fill(PAPER).inner_margin(12).corner_radius(3).show(ui, |ui| {
                egui::ScrollArea::both().id_salt(("text", self.topic)).max_height(height - 24.0).auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(l) = page(ui, h, t, &mut self.textures) {
                        go = Some(l);
                    }
                });
            });
        }
        if close {
            self.reading = false;
        }
        if let Some(english) = relanguage {
            let (i, topic, back) = (self.open.as_ref().unwrap().0, self.topic, std::mem::take(&mut self.back));
            self.english = english;
            self.load(i);
            (self.topic, self.back) = (topic, back);
        }
        if let Some(k) = go.filter(|&k| k != self.topic) {
            self.back.push(self.topic);
            self.topic = k;
        }
    }

    fn load(&mut self, i: usize) {
        let mut h = std::fs::read(&self.files[i].1).ok().and_then(|b| help::parse(&b)).unwrap_or(HelpFile { topics: vec![], pictures: vec![], root: None });
        let translation = self.files[i].1.parent().and_then(|d| d.parent()).map(|d| d.join("help_en").join(format!("{}.txt", self.files[i].0.to_ascii_uppercase())));
        let translation = translation.and_then(|p| std::fs::read_to_string(p).ok());
        self.translated = translation.is_some();
        if let Some(t) = translation.filter(|_| self.english) {
            h.translate(&t);
        }
        // The table of contents, when the book lists topics, is the last page.
        if h.topics.iter().any(|t| t.listed) {
            let contents = h.contents();
            h.topics.push(contents);
            self.contents = Some(h.topics.len() - 1);
        } else {
            self.contents = None;
        }
        self.open = Some((i, h));
        self.topic = self.open.as_ref().and_then(|o| o.1.root).or(self.contents).unwrap_or(0);
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
        self.reading = true;
        true
    }
}

const PAPER: Color32 = Color32::from_rgb(245, 243, 230);
/// What each help file is, for the reader and the Manual buttons.
pub fn title_of(stem: &str) -> &str {
    const TITLES: [(&str, &str); 24] = [
        ("GMDSS", "GMDSS Handbook"),
        ("COAST_RS", "Map of coast stations"),
        ("CALLSIGN", "List of call signs and numerical identities (VIIA)"),
        ("SERVICE", "List of radiodetermination and special service stations (VI)"),
        ("SHIPS", "List of ship stations (V)"),
        ("COAST", "List of coast stations (IV)"),
        ("TUTOR", "Self education"),
        ("CAPSAT", "Inmarsat-C ship earth station"),
        ("CAPSAT_S", "Inmarsat-C message terminal"),
        ("COSPAS", "EPIRBs (COSPAS-SARSAT)"),
        ("ELEC", "Batteries and power supply"),
        ("HF", "HF SSB RE2100"),
        ("HF_DSC", "MF/HF DSC RM2150"),
        ("HF_TLX", "DSC / telex RM2151"),
        ("INMA", "Inmarsat-A JUE-45"),
        ("INMARSAT", "Inmarsat earth stations"),
        ("NAVTEX", "NAVTEX receiver NT-900"),
        ("NVTX_RS", "NAVTEX stations"),
        ("R501", "Watch receiver R501"),
        ("SART", "SART"),
        ("SP3110", "Portable VHF SP3110"),
        ("TELEX", "Radiotelex"),
        ("VHF", "VHF RT2048"),
        ("VHF_DSC", "VHF DSC RM2042"),
    ];
    TITLES.iter().find(|(f, _)| f.eq_ignore_ascii_case(stem)).map_or(stem, |(_, t)| t)
}

/// The help file of each book on the shelf (`book-0` .. `book-7` in its artwork); the map of
/// coast stations is the chart, as on the original.
const REFERENCE: [&str; 8] = ["GMDSS", "Chart", "CALLSIGN", "SERVICE", "SHIPS", "SHIPS", "COAST", "TUTOR"];

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
        crate::snapshot::render("books", eframe::egui::vec2(940.0, 620.0), |ui| books.show(ui));
        // Topics with links and pictures, in the reader.
        for (file, topic, name) in [("hf.hlp", "-", "books_hf"), ("sart.hlp", "1b", "books_sart"), ("vhf_dsc.hlp", "Cdc", "books_cdc")] {
            let mut books = super::Books::new(Some(dir.clone())); // each snapshot has its own egui context
            if !books.open_topic(file, topic) {
                books.open_file(file);
                books.topic = 9;
            }
            crate::snapshot::render(name, eframe::egui::vec2(940.0, 620.0), |ui| books.show(ui));
        }
        // A Russian book, in English where the translation (help_en) is present.
        let mut books = super::Books::new(Some(dir.clone()));
        books.open_file("tutor.hlp");
        crate::snapshot::render("books_tutor", eframe::egui::vec2(940.0, 620.0), |ui| books.show(ui));
        // The table of contents of a book.
        let mut books = super::Books::new(Some(dir.clone()));
        books.open_file("hf_dsc.hlp");
        crate::snapshot::render("books_contents", eframe::egui::vec2(940.0, 620.0), |ui| books.show(ui));
    }
}

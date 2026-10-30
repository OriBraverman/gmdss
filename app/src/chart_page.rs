//! Chart: land (Natural Earth 1:50m, public domain), own ship, coast radio / NAVTEX / earth
//! stations, ranges, other vessels (added and edited here, as the original's vessel list).
//! Mercator; drag to pan, scroll to zoom, click a station or vessel for distance and bearing,
//! drag with the right button to measure.

use crate::Ship;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};
use gmdss::coast_rs::{Kind, Station};
use std::f64::consts::PI;

pub struct Chart {
    stations: Vec<Station>,
    /// View centre (lon, lat degrees) and pixels per degree of longitude.
    centre: (f64, f64),
    scale: f64,
    selected: Option<usize>,
    land: Vec<Ring>,
    /// The land drawn for the view it was drawn for (centre, scale, size).
    land_image: Option<((f64, f64, f64, [usize; 2]), egui::TextureHandle)>,
    pub vessels: Vec<Vessel>,
    /// A station's page was asked for: (help file, topic), for the Books page.
    pub help_request: Option<(String, String)>,
    selected_vessel: Option<usize>,
    /// The last measurement: from, to (lat, lon degrees).
    measure: Option<((f64, f64), (f64, f64))>,
}

/// Another vessel on the chart (degrees north, east).
#[derive(Clone, Debug, PartialEq)]
pub struct Vessel {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// A land outline or a hole in one (lon, lat degrees) and its bounding box.
struct Ring {
    pts: Vec<(f64, f64)>,
    lon: (f64, f64),
    lat: (f64, f64),
}

fn land() -> Vec<Ring> {
    include_str!("../assets/chart/land.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let v: Vec<f64> = l.split(' ').map(|n| f64::from(n.parse::<i32>().unwrap()) / 100.0).collect();
            let pts: Vec<(f64, f64)> = v.chunks(2).map(|c| (c[0], c[1])).collect();
            let range = |f: fn(&(f64, f64)) -> f64| pts.iter().map(f).fold((f64::MAX, f64::MIN), |r, x| (r.0.min(x), r.1.max(x)));
            Ring { lon: range(|p| p.0), lat: range(|p| p.1), pts }
        })
        .collect()
}

/// Land pixels of a `w` x `h` view centred on `centre` (lon, lat) at `scale` pixels per degree:
/// even-odd scanline fill of the rings, repeated every 360 degrees of longitude.
fn land_mask(rings: &[Ring], centre: (f64, f64), scale: f64, w: usize, h: usize) -> Vec<bool> {
    let (half_w, half_h) = (w as f64 / 2.0, h as f64 / 2.0);
    let cy = merc_y(centre.1);
    let view = (centre.0 - half_w / scale, centre.0 + half_w / scale);
    // Edges as (y top, y bottom, x at top, dx/dy) in pixels.
    let mut edges = vec![];
    for r in rings {
        if (cy - merc_y(r.lat.0)) * scale + half_h < 0.0 || (cy - merc_y(r.lat.1)) * scale + half_h > h as f64 {
            continue;
        }
        let first = ((view.0 - r.lon.1) / 360.0).ceil() as i32;
        let last = ((view.1 - r.lon.0) / 360.0).floor() as i32;
        for k in first..=last {
            let xy = |&(lon, lat): &(f64, f64)| ((lon + f64::from(k) * 360.0 - centre.0) * scale + half_w, (cy - merc_y(lat)) * scale + half_h);
            // Points less than a pixel apart add nothing (the world view would have 180,000 edges).
            let mut pts: Vec<(f64, f64)> = Vec::with_capacity(r.pts.len());
            for q in r.pts.iter().map(xy) {
                if pts.last().is_none_or(|l: &(f64, f64)| (l.0 - q.0).abs() + (l.1 - q.1).abs() >= 1.0) {
                    pts.push(q);
                }
            }
            if pts.len() < 3 {
                continue;
            }
            for (i, &a) in pts.iter().enumerate() {
                let b = pts[(i + 1) % pts.len()];
                let (top, bottom) = if a.1 < b.1 { (a, b) } else { (b, a) };
                if top.1 == bottom.1 || bottom.1 < 0.0 || top.1 > h as f64 {
                    continue;
                }
                let slope = (bottom.0 - top.0) / (bottom.1 - top.1);
                edges.push((top.1, bottom.1, top.0, slope));
            }
        }
    }
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut mask = vec![false; w * h];
    let (mut next, mut active, mut xs): (usize, Vec<(f64, f64, f64, f64)>, Vec<f64>) = (0, vec![], vec![]);
    for row in 0..h {
        let y = row as f64 + 0.5;
        while next < edges.len() && edges[next].0 <= y {
            active.push(edges[next]);
            next += 1;
        }
        active.retain(|e| e.1 > y);
        xs.clear();
        xs.extend(active.iter().filter(|e| e.0 <= y).map(|e| e.2 + (y - e.0) * e.3));
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            let (a, b) = ((pair[0] - 0.5).ceil().max(0.0) as usize, ((pair[1] - 0.5).ceil().max(0.0) as usize).min(w));
            mask[row * w + a.min(w)..row * w + b.max(a.min(w))].fill(true);
        }
    }
    mask
}

fn merc_y(lat: f64) -> f64 {
    (PI / 4.0 + lat.to_radians().clamp(-1.4, 1.4) / 2.0).tan().ln().to_degrees()
}

fn inv_merc_y(y: f64) -> f64 {
    (2.0 * y.to_radians().exp().atan() - PI / 2.0).to_degrees()
}

/// Great-circle distance (nm) and initial bearing (deg) from a to b, degrees.
pub fn distance_bearing(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (lat1, lon1, lat2, lon2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let d = (lat1.sin() * lat2.sin() + lat1.cos() * lat2.cos() * (lon2 - lon1).cos()).clamp(-1.0, 1.0).acos();
    let brg = ((lon2 - lon1).sin() * lat2.cos()).atan2(lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * (lon2 - lon1).cos());
    (d.to_degrees() * 60.0, brg.to_degrees().rem_euclid(360.0))
}

impl Chart {
    pub fn new(stations: Vec<Station>, ship: &Ship) -> Chart {
        Chart { stations, centre: (f64::from(ship.lon_min) / 60.0, f64::from(ship.lat_min) / 60.0), scale: 12.0, selected: None, land: land(), land_image: None, vessels: vec![], selected_vessel: None, measure: None, help_request: None }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, ship: &Ship) {
        ui.horizontal_top(|ui| {
            let w = (ui.available_width() - 240.0).max(300.0);
            ui.allocate_ui_with_layout(Vec2::new(w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| self.chart(ui, ship));
            ui.vertical(|ui| self.vessel_list(ui));
        });
    }

    /// The vessel list beside the chart: add, edit, delete.
    fn vessel_list(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("VESSELS").small().color(Color32::GRAY));
        let mut delete = None;
        for (i, v) in self.vessels.iter_mut().enumerate() {
            egui::Frame::new().fill(if self.selected_vessel == Some(i) { Color32::from_rgb(40, 60, 80) } else { Color32::TRANSPARENT }).inner_margin(4).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(130.0));
                    if ui.small_button("Delete").clicked() {
                        delete = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut v.lat).range(-85.0..=85.0).speed(0.01).fixed_decimals(2).suffix("\u{b0}N"));
                    ui.add(egui::DragValue::new(&mut v.lon).range(-180.0..=180.0).speed(0.01).fixed_decimals(2).suffix("\u{b0}E"));
                });
            });
        }
        if let Some(i) = delete {
            self.vessels.remove(i);
            self.selected_vessel = None;
        }
        if ui.button("Add a vessel at the chart centre").clicked() {
            self.vessels.push(Vessel { name: format!("VESSEL {}", self.vessels.len() + 1), lat: self.centre.1, lon: self.centre.0 });
            self.selected_vessel = Some(self.vessels.len() - 1);
            self.selected = None;
        }
    }

    fn chart(&mut self, ui: &mut egui::Ui, ship: &Ship) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size() - Vec2::new(0.0, 40.0), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, Color32::from_rgb(170, 200, 225));
        if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.centre.0 -= f64::from(d.x) / self.scale;
            let y = merc_y(self.centre.1) + f64::from(d.y) / self.scale;
            self.centre.1 = inv_merc_y(y);
        }
        if resp.hovered() {
            let z = ui.input(|i| i.smooth_scroll_delta.y);
            self.scale = (self.scale * (1.0 + f64::from(z) / 400.0)).clamp(0.5, 400.0);
        }
        // Land, drawn again when the view changes; the coast is the land's edge.
        let size = [rect.width().max(1.0) as usize, rect.height().max(1.0) as usize];
        let key = (self.centre.0, self.centre.1, self.scale, size);
        if self.land_image.as_ref().is_none_or(|(k, _)| *k != key) {
            let mask = land_mask(&self.land, self.centre, self.scale, size[0], size[1]);
            let (w, h) = (size[0], size[1]);
            let at = |x: usize, y: usize| mask[y * w + x];
            let pixels = (0..w * h)
                .map(|i| {
                    let (x, y) = (i % w, i / w);
                    if !mask[i] {
                        Color32::TRANSPARENT
                    } else if (x > 0 && !at(x - 1, y)) || (x + 1 < w && !at(x + 1, y)) || (y > 0 && !at(x, y - 1)) || (y + 1 < h && !at(x, y + 1)) {
                        COAST
                    } else {
                        LAND
                    }
                })
                .collect();
            let image = egui::ColorImage { size, source_size: Vec2::new(w as f32, h as f32), pixels };
            match &mut self.land_image {
                Some((k, tex)) => {
                    tex.set(image, egui::TextureOptions::NEAREST);
                    *k = key;
                }
                None => self.land_image = Some((key, ui.ctx().load_texture("chart-land", image, egui::TextureOptions::NEAREST))),
            }
        }
        if let Some((_, tex)) = &self.land_image {
            p.image(tex.id(), egui::Rect::from_min_size(rect.min, Vec2::new(size[0] as f32, size[1] as f32)), egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let c = rect.center();
        let cy = merc_y(self.centre.1);
        let to_screen = |lat: f64, lon: f64| -> Pos2 {
            let mut dl = lon - self.centre.0;
            dl = (dl + 540.0).rem_euclid(360.0) - 180.0;
            c + Vec2::new((dl * self.scale) as f32, ((cy - merc_y(lat)) * self.scale) as f32)
        };
        // Grid every 10 deg (5 or 1 when zoomed in).
        let step = if self.scale > 60.0 { 1.0 } else if self.scale > 15.0 { 5.0 } else { 10.0 };
        let grid = Stroke::new(0.5, Color32::from_rgb(120, 150, 180));
        let mut lat = -80.0;
        while lat <= 80.0 {
            let y = to_screen(lat, self.centre.0).y;
            p.hline(rect.x_range(), y, grid);
            p.text(Pos2::new(rect.left() + 4.0, y), Align2::LEFT_BOTTOM, format!("{lat:.0}\u{b0}"), FontId::proportional(10.0), Color32::from_rgb(60, 80, 110));
            lat += step;
        }
        let mut lon = -180.0;
        while lon < 180.0 {
            let x = to_screen(0.0, lon).x;
            p.vline(x, rect.y_range(), grid);
            p.text(Pos2::new(x + 2.0, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, format!("{lon:.0}\u{b0}"), FontId::proportional(10.0), Color32::from_rgb(60, 80, 110));
            lon += step;
        }
        // Own ship and ranges (nm -> degrees of latitude, drawn as circles: fine at these scales).
        let ship_pos = (f64::from(ship.lat_min) / 60.0, f64::from(ship.lon_min) / 60.0);
        let sp = to_screen(ship_pos.0, ship_pos.1);
        let px_per_nm = (self.scale / 60.0 / ship_pos.0.to_radians().cos()) as f32;
        for (nm, label) in [(30.0, "VHF  A1"), (150.0, "MF  A2"), (400.0, "NAVTEX")] {
            let r = nm * px_per_nm;
            p.circle_stroke(sp, r, Stroke::new(1.0, Color32::from_rgb(40, 90, 40)));
            p.text(sp + Vec2::new(r * 0.7, -r * 0.7), Align2::LEFT_BOTTOM, label, FontId::proportional(10.0), Color32::from_rgb(40, 90, 40));
        }
        p.circle_filled(sp, 6.0, Color32::from_rgb(220, 30, 30));
        p.text(sp + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, &ship.name, FontId::proportional(12.0), Color32::BLACK);
        // Stations.
        let mut nearest: Option<(usize, f32)> = None;
        let pointer = resp.interact_pointer_pos();
        for (i, s) in self.stations.iter().enumerate() {
            let pos = to_screen(f64::from(s.lat_min) / 60.0, f64::from(s.lon_min) / 60.0);
            if !rect.contains(pos) {
                continue;
            }
            let colour = match s.kind {
                Kind::CoastRadio => Color32::from_rgb(20, 60, 160),
                Kind::Navtex => Color32::from_rgb(150, 60, 10),
                Kind::EarthStation => Color32::from_rgb(100, 20, 120),
            };
            let shape = egui::Rect::from_center_size(pos, Vec2::splat(if self.selected == Some(i) { 10.0 } else { 6.0 }));
            if s.kind == Kind::EarthStation {
                p.circle_filled(pos, 4.0, colour);
            } else {
                p.rect_filled(shape, 0.0, colour);
            }
            if self.scale > 20.0 {
                p.text(pos + Vec2::new(5.0, -3.0), Align2::LEFT_BOTTOM, &s.name, FontId::proportional(10.0), colour);
            }
            if let Some(ptr) = pointer {
                let d = ptr.distance(pos);
                if d < 12.0 && nearest.is_none_or(|n| d < n.1) {
                    nearest = Some((i, d));
                }
            }
        }
        // Other vessels.
        let mut nearest_vessel: Option<(usize, f32)> = None;
        for (i, v) in self.vessels.iter().enumerate() {
            let pos = to_screen(v.lat, v.lon);
            let s = if self.selected_vessel == Some(i) { 9.0 } else { 7.0 };
            let colour = Color32::from_rgb(20, 90, 40);
            p.add(egui::Shape::convex_polygon(vec![pos + Vec2::new(0.0, -s), pos + Vec2::new(s * 0.7, s * 0.7), pos + Vec2::new(-s * 0.7, s * 0.7)], colour, Stroke::new(1.0, Color32::BLACK)));
            p.text(pos + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, &v.name, FontId::proportional(11.0), colour);
            if let Some(ptr) = pointer {
                let d = ptr.distance(pos);
                if d < 12.0 && nearest_vessel.is_none_or(|n| d < n.1) {
                    nearest_vessel = Some((i, d));
                }
            }
        }
        // Measuring with the right button: a line with its distance and bearing.
        let from_screen = |q: Pos2| -> (f64, f64) {
            let lon = self.centre.0 + f64::from(q.x - c.x) / self.scale;
            (inv_merc_y(cy - f64::from(q.y - c.y) / self.scale), (lon + 540.0).rem_euclid(360.0) - 180.0)
        };
        if let Some(q) = resp.interact_pointer_pos() {
            if resp.drag_started_by(egui::PointerButton::Secondary) {
                self.measure = Some((from_screen(q), from_screen(q)));
            } else if resp.dragged_by(egui::PointerButton::Secondary) {
                if let Some(m) = &mut self.measure {
                    m.1 = from_screen(q);
                }
            }
        }
        if let Some((a, b)) = self.measure {
            let (pa, pb) = (to_screen(a.0, a.1), to_screen(b.0, b.1));
            p.line_segment([pa, pb], Stroke::new(1.5, Color32::from_rgb(160, 20, 20)));
            let (d, brg) = distance_bearing(a, b);
            p.text(pb + Vec2::new(6.0, -6.0), Align2::LEFT_BOTTOM, format!("{d:.1} nm  {brg:03.0}\u{b0}"), FontId::proportional(12.0), Color32::from_rgb(160, 20, 20));
        }
        if resp.clicked() {
            match (nearest, nearest_vessel) {
                (_, Some(v)) if nearest.is_none_or(|s| v.1 <= s.1) => (self.selected, self.selected_vessel) = (None, Some(v.0)),
                (s, _) => (self.selected, self.selected_vessel) = (s.map(|n| n.0), None),
            }
        }
        // Info line.
        if let Some(v) = self.selected_vessel.and_then(|i| self.vessels.get(i)) {
            let (d, b) = distance_bearing(ship_pos, (v.lat, v.lon));
            ui.label(format!("{} (vessel): {} {}  -  {d:.0} nm, bearing {b:03.0}\u{b0} from {}", v.name, fmt_lat((v.lat * 60.0) as f32), fmt_lon((v.lon * 60.0) as f32), ship.name));
            return;
        }
        // Each station has its page in the station books (by id), as on the original.
        if let Some(s) = self.selected.and_then(|i| self.stations.get(i)) {
            let file = match s.kind {
                Kind::CoastRadio => "coast_rs.hlp",
                Kind::Navtex => "nvtx_rs.hlp",
                Kind::EarthStation => "inmarsat.hlp",
            };
            let mut open = false;
            ui.horizontal(|ui| open = ui.button("Station details").clicked());
            if open {
                self.help_request = Some((file.into(), s.id.clone()));
            }
        }
        let info = match self.selected.and_then(|i| self.stations.get(i)) {
            Some(s) => {
                let (d, b) = distance_bearing(ship_pos, (f64::from(s.lat_min) / 60.0, f64::from(s.lon_min) / 60.0));
                let kind = match s.kind {
                    Kind::CoastRadio => "coast radio station",
                    Kind::Navtex => "NAVTEX station",
                    Kind::EarthStation => "Inmarsat earth station",
                };
                format!("{} ({kind}, id {}): {} {}  -  {d:.0} nm, bearing {b:03.0}\u{b0} from {}", s.name, s.id, fmt_lat(s.lat_min), fmt_lon(s.lon_min), ship.name)
            }
            None => format!("Own ship {} {} {}  -  drag to pan, scroll to zoom, click a station or vessel, right-drag to measure", ship.name, fmt_lat(ship.lat_min as f32), fmt_lon(ship.lon_min as f32)),
        };
        ui.label(info);
    }
}

const LAND: Color32 = Color32::from_rgb(232, 222, 182);
const COAST: Color32 = Color32::from_rgb(110, 100, 70);

fn fmt_lat(m: f32) -> String {
    let a = m.abs().round() as i32;
    format!("{:02}\u{b0}{:02}'{}", a / 60, a % 60, if m >= 0.0 { 'N' } else { 'S' })
}

fn fmt_lon(m: f32) -> String {
    let a = m.abs().round() as i32;
    format!("{:03}\u{b0}{:02}'{}", a / 60, a % 60, if m >= 0.0 { 'E' } else { 'W' })
}

#[cfg(test)]
mod tests {
    #[test]
    fn distance_and_bearing() {
        let (d, b) = super::distance_bearing((0.0, 0.0), (1.0, 0.0));
        assert!((d - 60.0).abs() < 0.01 && b.abs() < 0.01);
        let (_, b) = super::distance_bearing((0.0, 0.0), (0.0, 1.0));
        assert!((b - 90.0).abs() < 0.01);
    }

    /// A vessel and a measurement on the chart, with the vessel list beside it.
    #[test]
    fn vessels_and_measuring() {
        let ship = crate::default_ship();
        let mut c = super::Chart::new(vec![], &ship);
        c.vessels.push(super::Vessel { name: "RESCUER".into(), lat: 33.2, lon: 34.2 });
        c.measure = Some(((32.92, 34.67), (33.2, 34.2)));
        c.scale = 60.0;
        crate::snapshot::render("chart_vessels", eframe::egui::vec2(940.0, 600.0), |ui| c.show(ui, &ship));
        let (d, b) = super::distance_bearing((32.92, 34.67), (33.2, 34.2));
        assert!((25.0..35.0).contains(&d) && (290.0..330.0).contains(&b), "{d} {b}");
    }

    /// The whole world at 2 pixels per degree, centred on 0/0: land where land is, also across
    /// the date line (the view is 400 degrees wide).
    #[test]
    fn land_and_sea() {
        let (w, h) = (800, 400);
        let mask = super::land_mask(&super::land(), (0.0, 0.0), 2.0, w, h);
        let at = |lat: f64, lon: f64| {
            let x = (lon * 2.0 + w as f64 / 2.0) as usize;
            let y = (h as f64 / 2.0 - super::merc_y(lat) * 2.0) as usize;
            mask[y * w + x]
        };
        for (lat, lon, land, place) in [
            (0.0, 20.0, true, "Congo"),
            (46.0, 2.0, true, "France"),
            (40.0, -100.0, true, "USA"),
            (-25.0, 135.0, true, "Australia"),
            (0.0, -30.0, false, "Atlantic"),
            (35.0, 18.0, false, "Mediterranean"),
            (-30.0, 80.0, false, "Indian Ocean"),
            (65.0, -199.0, true, "Alaska, west of the date line"),
            (0.0, 190.0, false, "Pacific, east of the date line"),
        ] {
            assert_eq!(at(lat, lon), land, "{place}");
        }
    }
}

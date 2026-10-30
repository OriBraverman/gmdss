//! Options (own ship identity and position) and Intercom (the classroom network and the exercise
//! events an instructor sends: other ships' distress alerts, alarms, messages).

use crate::Ship;
use eframe::egui::{self, Color32, RichText};
use gmdss::dsc::Position;

pub fn show_options(ui: &mut egui::Ui, ship: &mut Ship) {
    ui.label(RichText::new("OWN SHIP").small().color(Color32::GRAY));
    egui::Grid::new("ship").num_columns(2).show(ui, |ui| {
        for (label, value) in [("Name", &mut ship.name), ("Call sign", &mut ship.call_sign), ("MMSI", &mut ship.mmsi), ("Telex number", &mut ship.telex), ("Answerback", &mut ship.answerback)] {
            ui.label(label);
            ui.text_edit_singleline(value);
            ui.end_row();
        }
        ui.label("Latitude (minutes, N +)");
        ui.add(egui::DragValue::new(&mut ship.lat_min).range(-80 * 60..=80 * 60));
        ui.end_row();
        ui.label("Longitude (minutes, E +)");
        ui.add(egui::DragValue::new(&mut ship.lon_min).range(-180 * 60..=180 * 60));
        ui.end_row();
    });
    ui.label(Position { lat_min: ship.lat_min, lon_min: ship.lon_min }.text());
    ui.add_space(8.0);
    ui.label(RichText::new("POSITION PRESETS").small().color(Color32::GRAY));
    ui.horizontal_wrapped(|ui| {
        for (name, lat, lon) in [
            ("Off Haifa (A1)", 32 * 60 + 55, 34 * 60 + 40),
            ("Off Cyprus (A2)", 34 * 60 + 30, 33 * 60),
            ("Central Mediterranean (A2/A3)", 35 * 60, 18 * 60),
            ("Mid-Atlantic (A3)", 40 * 60, -35 * 60),
            ("North Sea off Lyngby", 56 * 60, 9 * 60),
        ] {
            if ui.button(name).clicked() {
                ship.lat_min = lat;
                ship.lon_min = lon;
            }
        }
    });
    if ship.mmsi.len() != 9 || !ship.mmsi.chars().all(|c| c.is_ascii_digit()) {
        ui.colored_label(Color32::LIGHT_RED, "The MMSI must be 9 digits.");
    }
}

/// Exercise events an instructor sends: id (also the network name) and button text.
pub const EXERCISES: [(&str, &str); 6] = [
    ("vhf_distress", "VHF DSC distress alert from a nearby ship (FIRE)"),
    ("mf_distress", "MF DSC distress alert on 2187.5 kHz (SINKING)"),
    ("two_tone", "Radiotelephone alarm signal on 2182 kHz"),
    ("sart_radar", "Rescue vessel radar sweeps our SART"),
    ("navtex_sar", "NAVTEX search and rescue message (type D)"),
    ("shore_telex", "Shore office sends a telex to Inmarsat-C"),
];

/// The Intercom page: the classroom network and the exercise buttons; returns the exercise clicked.
pub fn show_intercom(ui: &mut egui::Ui, link: Option<&crate::net_link::Link>) -> Option<&'static str> {
    ui.label(RichText::new("CLASSROOM NETWORK").small().color(Color32::GRAY));
    match link {
        None => {
            ui.label("Stand-alone. Start with --instructor PORT to teach a class, or --student HOST:PORT to join one.");
        }
        Some(l) if l.instructor => {
            ui.label(format!("Instructor console, listening on {}. Students: {}", l.address, l.students.len()));
            egui::Grid::new("students").striped(true).show(ui, |ui| {
                for s in &l.students {
                    ui.label(&s.name);
                    ui.label(&s.mmsi);
                    ui.label(Position { lat_min: s.pos.0, lon_min: s.pos.1 }.text());
                    ui.end_row();
                }
            });
        }
        Some(l) => {
            ui.label(format!("Student, instructor at {} - {}", l.address, if l.connected() > 0 { "connected" } else { "connection lost" }));
        }
    }
    ui.add_space(10.0);
    let to = if link.is_some_and(|l| l.instructor) { "sent to every student" } else { "on this console" };
    ui.label(RichText::new(format!("EXERCISE EVENTS ({to})")).small().color(Color32::GRAY));
    let mut chosen = None;
    for (id, text) in EXERCISES {
        if ui.button(text).clicked() {
            chosen = Some(id);
        }
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Received calls appear on the units and in the radio log; DSC alarms sound until STOP.").small().color(Color32::GRAY));
    chosen
}

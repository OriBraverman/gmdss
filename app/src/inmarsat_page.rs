//! Inmarsat-A (telex SES with VDU) and Inmarsat-C (transceiver + message terminal).

use crate::clock::SimClock;
use crate::panel::{GREEN, RED};
use crate::{Shared, Ship};
use eframe::egui::{self, Color32, RichText, Vec2};
use gmdss::ces_list::{CesList, OceanRegion};
use gmdss::dsc::Nature;
use gmdss::egc::Broadcast;
use gmdss::inmarsat_a::{look_angles, satellite_longitude, Env, Priority, Ses};
use gmdss::inmarsat_c::{Link, Now, Terminal};
use gmdss::subscriber::Subscriber;
use std::path::PathBuf;

pub struct Inmarsat {
    pub ses: Ses,
    setup_open: bool,
    keys_a: crate::svg_art::Art,
    /// Inmarsat-A PRINT ON: screen output also goes to the printer.
    print_on: bool,
    /// How much of the Inmarsat-A screen and how many Inmarsat-C files went to the printer.
    printed_a: usize,
    printed_c: usize,
    pub term: Terminal,
    view: View,
    editor_name: String,
    editor: String,
    destination: String,
    transceiver: crate::svg_art::Art,
    /// Sim times the Set and Alarm keys were pressed (distress needs both).
    set_at: Option<f64>,
    alarm_at: Option<f64>,
    /// Received messages already seen in the receive log (the Mail lamp shows new ones).
    mail_seen: usize,
    data_dir: PathBuf,
}

#[derive(PartialEq, Clone, Copy)]
enum View {
    Editor,
    Directory,
    TransmitLog,
    ReceiveLog,
    EgcLog,
    Setup,
}

fn now_c(clock: &SimClock) -> Now {
    let t = clock.now();
    Now { secs: clock.secs(), hour: t.hour, minute: t.minute, day: t.day, month: t.month, year: t.year }
}

impl Inmarsat {
    pub fn new(ship: &Ship, data_dir: PathBuf) -> Inmarsat {
        Inmarsat {
            print_on: false,
            printed_a: 0,
            printed_c: 0,
            keys_a: crate::svg_art::Art::new("inmarsat_a_keys", include_str!("../assets/panels/inmarsat_a_keys.svg")),
            ses: Ses::new(&ship.answerback),
            setup_open: false,
            term: Terminal::new(),
            view: View::Editor,
            editor_name: "MSG001".into(),
            editor: String::new(),
            destination: String::new(),
            transceiver: crate::svg_art::Art::new("inmarsat_c", include_str!("../assets/panels/inmarsat_c.svg")),
            set_at: None,
            alarm_at: None,
            mail_seen: 0,
            data_dir,
        }
    }

    pub fn tick(&mut self, clock: &SimClock, ship: &Ship, ces: &CesList, subs: &mut Vec<Subscriber>, schedule: &[Broadcast], shared: &mut Shared) {
        let t = clock.now();
        let mut env = Env { now: clock.secs(), date: (t.day, t.month, t.year, t.hour, t.minute), lat: f64::from(ship.lat_min) / 60.0, lon: f64::from(ship.lon_min) / 60.0, ces, subscribers: subs };
        self.ses.tick(&mut env);
        let before = (self.term.receive_log.len(), self.term.egc_log.len(), self.term.status.clone());
        let dir = self.data_dir.clone();
        let egc_text = move |file: &str| std::fs::read(dir.join(file)).ok().map(|b| gmdss::dos::latin1(&b).replace('\r', ""));
        self.term.tick(&now_c(clock), schedule, &egc_text);
        if self.term.egc_log.len() > before.1 {
            let e = self.term.egc_log.last().unwrap();
            shared.note(clock, format!("Inmarsat-C EGC received: {} from LES {:03}", e.kind, e.les));
        }
        if self.term.receive_log.len() > before.0 {
            shared.note(clock, format!("Inmarsat-C message received: {}", self.term.receive_log.last().unwrap().file));
        }
        if self.term.status != before.2 && self.term.status.starts_with("Log") {
            shared.note(clock, format!("Inmarsat-C: {}", self.term.status));
        }
    }

    pub fn load_a(&self) -> f64 {
        (if self.ses.on { 3.0 } else { 0.0 }) + if self.term.transceiver_on { 1.0 } else { 0.0 }
    }

    pub fn show_a(&mut self, ui: &mut egui::Ui, clock: &SimClock, ship: &Ship, ces: &CesList, subs: &mut Vec<Subscriber>) {
        let t = clock.now();
        let mut env = Env { now: clock.secs(), date: (t.day, t.month, t.year, t.hour, t.minute), lat: f64::from(ship.lat_min) / 60.0, lon: f64::from(ship.lon_min) / 60.0, ces, subscribers: subs };
        // VDU in its beige cabinet (as the key strip below; 720 wide in all).
        let cabinet = egui::Frame::new().fill(Color32::from_rgb(216, 210, 172)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 88, 70))).inner_margin(12).corner_radius(3);
        cabinet.show(ui, |ui| egui::Frame::new().fill(Color32::from_rgb(10, 14, 10)).stroke(egui::Stroke::new(2.0, Color32::from_rgb(70, 74, 62))).inner_margin(10).corner_radius(4).show(ui, |ui| {
            ui.set_width(720.0 - 2.0 * 12.0 - 2.0 * 10.0 - 4.0);
            ui.set_height(300.0);
            let status = if !self.ses.on { "POWER OFF".into() } else if self.ses.online() { "ON LINE".into() } else { format!("IDLE   {}  CES {}  {}", self.ses.setup.region.header(), self.ses.setup.ces_id, if self.ses.setup.priority == Priority::Distress { "DISTRESS" } else { "ROUTINE" }) };
            ui.label(RichText::new(format!("JUE-45 INMARSAT-A SES   {status}")).monospace().color(Color32::BLACK).background_color(Color32::from_rgb(150, 200, 150)));
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                let mut s = self.ses.screen.clone();
                if self.ses.on {
                    s.push('_');
                }
                ui.label(RichText::new(s).monospace().size(13.0).color(Color32::from_rgb(140, 230, 140)));
            });
        }));
        // Keyboard to the line.
        if self.ses.on && !self.setup_open {
            for e in ui.input(|i| i.events.clone()) {
                match e {
                    egui::Event::Text(s) => s.chars().for_each(|c| self.ses.type_char(c, &mut env)),
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => self.ses.type_char('\n', &mut env),
                    _ => {}
                }
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(720.0);
                let p = crate::panel::Plate::with_art(ui, "Inmarsat_A", &mut self.keys_a, 720.0);
                let art = &self.keys_a;
                let painter = ui.painter().clone();
                for (lamp, on) in [("LINE", self.ses.online()), ("SES_CONT", self.setup_open), ("PRINT_ON", self.print_on)] {
                    if on {
                        let r = p.place(art.rect(&format!("lamp-{lamp}")));
                        painter.rect_filled(r.expand(1.5), 1.0, Color32::from_rgba_unmultiplied(255, 0, 12, 70));
                        painter.rect_filled(r, 1.0, Color32::from_rgb(0xff, 0x00, 0x0c));
                    }
                }
                for name in ["LINE", "RLS", "SES_CONT", "EDIT", "FLIST", "TRSFR", "WRU", "HERE_IS", "PRINT_ON"] {
                    if p.spot(ui, name, art.rect(&format!("key-{name}"))).0 {
                        match name {
                            "LINE" => self.ses.line(&env),
                            "RLS" => self.ses.release(&mut env),
                            "SES_CONT" => self.setup_open = !self.setup_open,
                            "WRU" => self.ses.wru(&env),
                            "HERE_IS" => self.ses.here_is(),
                            "PRINT_ON" => self.print_on = !self.print_on,
                            _ => {} // EDIT, FLIST, TRSFR: not simulated
                        }
                    }
                }
            });
            ui.add_space(12.0);
            if crate::panel::hook(&ui.add(egui::Button::new(RichText::new("POWER").color(if self.ses.on { GREEN } else { Color32::GRAY })).min_size(Vec2::new(70.0, 34.0))), "Inmarsat_A", "POWER") {
                self.ses.on = !self.ses.on;
                if self.ses.on {
                    self.ses.screen.push_str("JUE-45 READY\n");
                } else {
                    self.ses.release(&mut env);
                }
            }
        });
        if self.setup_open {
            ui.add_space(6.0);
            egui::Frame::new().fill(Color32::from_rgb(30, 40, 42)).inner_margin(8).show(ui, |ui| {
                ui.label(RichText::new("SES CONTROL").strong());
                egui::Grid::new("sescont").show(ui, |ui| {
                    ui.label("SATELLITE (ocean region)");
                    egui::ComboBox::from_id_salt("region").selected_text(self.ses.setup.region.display_name()).show_ui(ui, |ui| {
                        for r in OceanRegion::ALL {
                            ui.selectable_value(&mut self.ses.setup.region, r, r.display_name());
                        }
                    });
                    ui.end_row();
                    ui.label("CES");
                    let list = ces.region(self.ses.setup.region);
                    let current = list.iter().find(|c| c.id == self.ses.setup.ces_id).map_or("?".to_string(), |c| format!("{} {}", c.id, c.name.trim()));
                    egui::ComboBox::from_id_salt("ces").selected_text(current).show_ui(ui, |ui| {
                        for c in list {
                            ui.selectable_value(&mut self.ses.setup.ces_id, c.id.clone(), format!("{} {} ({})", c.id, c.name.trim(), c.country.trim()));
                        }
                    });
                    ui.end_row();
                    ui.label("AZIMUTH");
                    ui.add(egui::DragValue::new(&mut self.ses.setup.azimuth).range(0.0..=359.9).speed(0.5).suffix(" deg"));
                    ui.end_row();
                    ui.label("ELEVATION");
                    ui.add(egui::DragValue::new(&mut self.ses.setup.elevation).range(-5.0..=90.0).speed(0.5).suffix(" deg"));
                    ui.end_row();
                    ui.label("PRIORITY");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.ses.setup.priority, Priority::Routine, "ROUTINE");
                        ui.selectable_value(&mut self.ses.setup.priority, Priority::Distress, "DISTRESS");
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Antenna positioning table for the ship's position:").small().color(Color32::GRAY));
                for r in OceanRegion::ALL {
                    let (az, el) = look_angles(env.lat, env.lon, satellite_longitude(r));
                    ui.label(RichText::new(format!("{:22} azimuth {:5.1}  elevation {:5.1}", r.display_name(), az, el)).monospace().small().color(if el > 0.0 { Color32::LIGHT_GRAY } else { Color32::DARK_GRAY }));
                }
            });
        }
        ui.label(RichText::new("Dial e.g. 00 + country + number + '+'; five full stops end the call.").small().color(Color32::GRAY));
    }

    /// Text for the printer since the last call: Inmarsat-A screen output while PRINT ON, and every
    /// message the Inmarsat-C terminal receives (the Capsat prints them automatically).
    pub fn printout(&mut self) -> String {
        let mut out = String::new();
        let screen = &self.ses.screen;
        if self.printed_a > screen.len() {
            self.printed_a = 0; // screen was cleared
        }
        if self.print_on {
            out.push_str(&screen[self.printed_a..]);
        }
        self.printed_a = screen.len();
        for (name, text) in self.term.files.iter().skip(self.printed_c) {
            out.push_str(&format!("\n----- {name} -----\n{text}\n"));
        }
        self.printed_c = self.term.files.len();
        out
    }

    /// Set and Alarm were pressed: the distress alert goes out in a few seconds.
    pub fn arming(&self) -> bool {
        self.set_at.zip(self.alarm_at).is_some_and(|(a, b)| (a - b).abs() <= 3.0)
    }

    pub fn show_c(&mut self, ui: &mut egui::Ui, clock: &SimClock) {
        let now = now_c(clock);
        // Transceiver front: keys, lamps; Set + Alarm within 3 s arms the distress alert, sent 5 s later.
        ui.horizontal(|ui| {
            ui.set_width(420.0);
            let p = crate::panel::Plate::with_art(ui, "Inm", &mut self.transceiver, 420.0);
            let art = &self.transceiver;
            let t = clock.secs();
            if p.spot(ui, "ON_OFF", art.rect("key-ON_OFF")).0 {
                self.term.transceiver_on = !self.term.transceiver_on;
                if !self.term.transceiver_on {
                    self.term.link = Link::LoggedOut;
                }
            }
            if p.spot(ui, "SET", art.rect("key-SET")).0 {
                self.set_at = Some(t);
            }
            if p.spot(ui, "ALARM", art.rect("key-ALARM")).0 {
                self.alarm_at = Some(t);
            }
            if let (Some(a), Some(b)) = (self.set_at, self.alarm_at) {
                if (a - b).abs() <= 3.0 && self.term.transceiver_on && !self.term.distress_active {
                    let armed = a.max(b);
                    if t - armed >= 5.0 {
                        self.term.distress_button(&now);
                        (self.set_at, self.alarm_at) = (None, None);
                    }
                } else if t - a.max(b) > 3.0 {
                    (self.set_at, self.alarm_at) = (None, None);
                }
            }
            let arming = self.set_at.zip(self.alarm_at).is_some_and(|(a, b)| (a - b).abs() <= 3.0);
            let flash = ui.input(|i| i.time) % 0.5 < 0.25;
            let on = self.term.transceiver_on;
            let mail = self.term.receive_log.len() > self.mail_seen;
            p.lamp_glow(ui, art.rect("lamp-POWER"), on, GREEN, "Power");
            p.lamp_glow(ui, art.rect("lamp-LOGIN"), on && self.term.link == Link::LoggedIn, GREEN, "Logged in");
            p.lamp_glow(ui, art.rect("lamp-SEND"), on && self.term.status.contains("SENDING"), GREEN, "Sending");
            p.lamp_glow(ui, art.rect("lamp-MAIL"), on && mail, Color32::from_rgb(0xd8, 0xe0, 0x70), "New message received");
            p.lamp_glow(ui, art.rect("lamp-ALARM"), on && (self.term.distress_active || arming && flash), RED, "Distress alert");
            if arming {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        });
        ui.add_space(6.0);
        // Message terminal.
        egui::Frame::new().fill(Color32::from_rgb(20, 30, 60)).inner_margin(8).corner_radius(4).show(ui, |ui| {
            ui.set_width(760.0);
            ui.horizontal_wrapped(|ui| {
                for (label, v) in [("Editor  F1", View::Editor), ("Directory  F4", View::Directory), ("Transmit log", View::TransmitLog), ("Receive log", View::ReceiveLog), ("EGC log", View::EgcLog), ("Setup", View::Setup)] {
                    let r = ui.selectable_label(self.view == v, label);
                    if crate::panel::hook(&r, "Inm", label.split_whitespace().next().unwrap_or(label)) {
                        self.view = v;
                    }
                }
                ui.separator();
                let regions = OceanRegion::ALL;
                if crate::panel::hook(&ui.add_enabled(self.term.link == Link::LoggedOut && self.term.transceiver_on, egui::Button::new("Login...")), "Inm", "Login") {
                    let r = self.term.region;
                    self.term.login(r, &now);
                }
                egui::ComboBox::from_id_salt("cregion").selected_text(self.term.region.display_name()).show_ui(ui, |ui| {
                    for r in regions {
                        ui.selectable_value(&mut self.term.region, r, r.display_name());
                    }
                });
                if crate::panel::hook(&ui.add_enabled(self.term.link == Link::LoggedIn, egui::Button::new("Logout...")), "Inm", "Logout") {
                    self.term.logout(&now);
                }
            });
            ui.separator();
            ui.set_min_height(320.0);
            match self.view {
                View::Editor => {
                    ui.horizontal(|ui| {
                        ui.label("File:");
                        ui.text_edit_singleline(&mut self.editor_name);
                        ui.label("To:");
                        ui.add(egui::TextEdit::singleline(&mut self.destination).hint_text("telex no. / name").desired_width(160.0));
                        if crate::panel::hook(&ui.button("Save"), "Inm", "Save") {
                            self.save();
                        }
                        if crate::panel::hook(&ui.button("< SEND >"), "Inm", "SEND") {
                            self.save();
                            let (name, dest) = (self.editor_name.clone(), self.destination.clone());
                            self.term.transmit(&name, &dest, &now);
                        }
                    });
                    ui.add(egui::TextEdit::multiline(&mut self.editor).font(egui::TextStyle::Monospace).desired_rows(14).desired_width(f32::INFINITY));
                }
                View::Directory => {
                    ui.label(RichText::new("Filename       Size").monospace());
                    let mut open = None;
                    for (i, (name, text)) in self.term.files.iter().enumerate() {
                        if ui.selectable_label(false, RichText::new(format!("{name:<14}{:>5}", text.len())).monospace()).clicked() {
                            open = Some(i);
                        }
                    }
                    if let Some(i) = open {
                        self.editor_name = self.term.files[i].0.clone();
                        self.editor = self.term.files[i].1.clone();
                        self.view = View::Editor;
                    }
                }
                View::TransmitLog | View::ReceiveLog | View::EgcLog => {
                    let (title, log) = match self.view {
                        View::TransmitLog => ("Transmit log", &self.term.transmit_log),
                        View::ReceiveLog => {
                            self.mail_seen = self.term.receive_log.len();
                            ("Receive log", &self.term.receive_log)
                        }
                        _ => ("Egc  Log", &self.term.egc_log),
                    };
                    ui.label(RichText::new(title).strong());
                    ui.label(RichText::new("File      Ref.No  Time   Type                 Pri       LES  Status").monospace());
                    for e in log {
                        ui.label(RichText::new(format!("{:<9} {:>6}  {:02}:{:02}  {:<20} {:<9} {:03}  {}", e.file, e.ref_no, e.time.0, e.time.1, e.kind, e.priority, e.les, e.status)).monospace());
                    }
                }
                View::Setup => {
                    ui.label("EGC: NAVAREA / METAREA numbers to receive");
                    ui.horizontal_wrapped(|ui| {
                        for a in 1..=16u8 {
                            let mut on = self.term.areas.contains(&a);
                            if ui.checkbox(&mut on, a.to_string()).changed() {
                                if on { self.term.areas.push(a) } else { self.term.areas.retain(|x| *x != a) }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("LES for sending:");
                        ui.add(egui::DragValue::new(&mut self.term.les).range(0..=399));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Distress message nature:");
                        egui::ComboBox::from_id_salt("cnature").selected_text(self.term.distress_nature.name()).show_ui(ui, |ui| {
                            for n in Nature::ALL {
                                ui.selectable_value(&mut self.term.distress_nature, n, n.name());
                            }
                        });
                    });
                    ui.label(RichText::new("Only the details of the alert are set here; nothing is sent.\nThe alert itself goes out with SET + ALARM on the transceiver panel.").small().color(Color32::GRAY));
                }
            }
            ui.separator();
            // The terminal computer is always on here; the original's lessons switch it on (`Power`).
            crate::panel::hook(&ui.label(RichText::new(format!("Status:   {}", self.term.status)).monospace().color(Color32::from_rgb(200, 220, 255))), "Inm_Mon", "Power");
        });
    }

    fn save(&mut self) {
        let name = self.editor_name.trim().to_uppercase();
        match self.term.files.iter_mut().find(|f| f.0 == name) {
            Some(f) => f.1 = self.editor.clone(),
            None => self.term.files.push((name, self.editor.clone())),
        }
    }
}

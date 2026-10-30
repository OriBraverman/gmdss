//! The smaller units: NAVTEX, R501 watch receiver, SART, EPIRB, portable VHF, battery charger.

use crate::clock::SimClock;
use crate::panel::{Plate, GREEN, RED, YELLOW};
use crate::Shared;
use eframe::egui::{self, Align2, Color32, RichText, Stroke, Vec2};
use gmdss::charger::{Charger, Switch};
use gmdss::epirb::Epirb;
use gmdss::navtex::{self, Message, Navtex};
use gmdss::portable::{self, Portable, Target};
use gmdss::r501::R501;
use gmdss::sart::{Sart, State};

pub struct Units {
    pub navtex: Navtex,
    navtex_art: crate::svg_art::Art,
    broadcasts: Vec<Message>,
    next_broadcast: f64,
    pub r501: R501,
    r501_test_down: bool,
    r501_art: crate::svg_art::Art,
    pub sart: Sart,
    sart_art: crate::svg_art::Art,
    radar_in_range: bool,
    last_sweep: f64,
    pub epirb: Epirb,
    epirb_art: crate::svg_art::Art,
    epirb_reported: bool,
    pub portable: Portable,
    portable_art: crate::svg_art::Art,
    press_start: Option<(portable::Key, f64)>,
    pub charger: Charger,
    charger_art: crate::svg_art::Art,
    last_secs: f64,
}

impl Units {
    pub fn new(ntx: Vec<Message>) -> Units {
        Units {
            navtex: Navtex::default(),
            navtex_art: crate::svg_art::Art::new("navtex", include_str!("../assets/panels/navtex.svg")),
            broadcasts: if ntx.is_empty() { demo_navtex() } else { ntx },
            next_broadcast: 60.0,
            r501: R501::default(),
            r501_test_down: false,
            r501_art: crate::svg_art::Art::new("r501", include_str!("../assets/panels/r501.svg")),
            sart: Sart::default(),
            sart_art: crate::svg_art::Art::new("sart", include_str!("../assets/panels/sart.svg")),
            radar_in_range: false,
            last_sweep: 0.0,
            epirb: Epirb::new(),
            epirb_art: crate::svg_art::Art::new("epirb", include_str!("../assets/panels/epirb.svg")),
            epirb_reported: false,
            portable: Portable::default(),
            portable_art: crate::svg_art::Art::new("portable", include_str!("../assets/panels/portable.svg")),
            press_start: None,
            charger: Charger::default(),
            charger_art: crate::svg_art::Art::new("charger", include_str!("../assets/panels/charger.svg")),
            last_secs: 0.0,
        }
    }

    /// Runs every unit; `load_a` is what the other radio equipment draws now.
    pub fn tick(&mut self, clock: &SimClock, shared: &mut Shared, load_a: f64) {
        let now = clock.secs();
        // NAVTEX broadcasts on 518 kHz every few minutes.
        if now >= self.next_broadcast && !self.broadcasts.is_empty() {
            let m = self.broadcasts.remove(0);
            // Haifa NAVTEX (518 kHz) must be in range for the demo broadcasts.
            let in_range = gmdss::propagation::heard(HAIFA_NAVTEX, shared.ship_pos, 518.0, shared.hour);
            if in_range && self.navtex.receive(&m) {
                shared.note(clock, format!("NAVTEX printed {}{}{:02} ({})", m.b1, m.b2, m.number, navtex::subject(m.b2)));
            }
            self.broadcasts.push(m);
            self.next_broadcast = now + 240.0;
        }
        self.r501.tick(now);
        if self.radar_in_range && now - self.last_sweep >= 2.5 {
            // A search vessel's 9 GHz radar sweeps the SART every 2.5 s.
            self.last_sweep = now;
            self.sart.radar_pulse();
        }
        self.sart.tick(now);
        self.epirb.tick(now);
        match self.epirb.active_since {
            Some(t) if now - t > 120.0 && !self.epirb_reported => {
                self.epirb_reported = true;
                shared.note(clock, "COSPAS-SARSAT: 406 MHz EPIRB alert of SEA STAR received and passed to the MRCC".into());
            }
            None => self.epirb_reported = false,
            _ => {}
        }
        self.portable.tick(now);
        let extra = if self.navtex.on { 0.3 } else { 0.0 } + if self.r501.on() { 0.2 } else { 0.0 };
        self.charger.load_a = load_a + extra;
        self.charger.tick(now - self.last_secs);
        self.last_secs = now;
    }

    pub fn show_navtex(&mut self, ui: &mut egui::Ui) {
        let p = Plate::with_art(ui, "Navtex", &mut self.navtex_art, 640.0);
        let art = &self.navtex_art;
        let painter = ui.painter().clone();
        let n = &self.navtex;
        let lit = n.on && n.dim > 0;
        // Square lamps: lit in their colour, dimmed with the DIMMER setting.
        let lamp = |id: &str, on: bool, c: Color32| {
            if on {
                let r = p.place(art.rect(id));
                painter.rect_filled(r.expand(r.width() * 0.6), 1.0, c.gamma_multiply(0.25));
                painter.rect_filled(r, 0.5, c.gamma_multiply(0.4 + 0.2 * f32::from(n.dim.max(1))));
            }
        };
        lamp("lamp-ALARM", n.alarm, RED);
        lamp("keylamp-ALARM_STOP", n.alarm, RED);
        lamp("keylamp-POWER", lit, Color32::from_rgb(0xe7, 0xf7, 0x51));
        // The paper window shows the last printed lines.
        if n.on {
            let r = p.place(art.rect("paper"));
            let tail: Vec<&str> = n.paper.lines().rev().take(2).collect();
            for (i, line) in tail.iter().enumerate() {
                painter.text(r.left_bottom() + Vec2::new(4.0, -r.height() * (0.28 + 0.45 * i as f32)), Align2::LEFT_CENTER, *line, p.mono(7.0), Color32::from_rgb(30, 30, 60));
            }
        }
        use navtex::Key as K;
        for (name, key) in [("MENU", K::Menu), ("ALARM_STOP", K::AlarmStop), ("SELECT", K::Select), ("Up", K::Up), ("EXCLUDE", K::Exclude), ("Down", K::Down), ("LINE_FEED", K::LineFeed), ("POWER", K::Power)] {
            if p.spot(ui, name, art.rect(&format!("key-{name}"))).0 {
                self.navtex.press(key);
            }
        }
        // The paper that came out of the printer.
        ui.add_space(6.0);
        egui::Frame::new().fill(Color32::from_rgb(245, 243, 230)).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width().min(620.0));
            egui::ScrollArea::vertical().max_height(260.0).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
                ui.label(RichText::new(&self.navtex.paper).monospace().size(12.0).color(Color32::from_rgb(30, 30, 60)));
            });
        });
    }

    pub fn show_r501(&mut self, ui: &mut egui::Ui, clock: &SimClock) {
        let p = Plate::with_art(ui, "R_501", &mut self.r501_art, 620.0);
        let art = &self.r501_art;
        let knob = art.rect("knob-AF_GAIN");
        if p.knob_spot(ui, "AF_GAIN", knob, &mut self.r501.af_gain, 10) && !self.r501.on() {
            self.r501.mute_off();
        }
        p.knob_pointer(ui, knob, self.r501.af_gain, 10);
        let minute = clock.now().minute;
        let speaker = self.r501.loudspeaker(minute);
        p.lamp_glow(ui, art.rect("lamp-1"), self.r501.on(), GREEN, "Power");
        p.lamp_glow(ui, art.rect("lamp-2"), self.r501.on() && self.r501.muted, YELLOW, "Loudspeaker muted");
        p.lamp_glow(ui, art.rect("lamp-3"), speaker, RED, "Loudspeaker on (alarm signal, silence period or test)");
        if self.r501.on() && self.r501.miss_silence {
            let r = p.place(art.rect("btn-MISS_SILENCE_PERIOD"));
            ui.painter().circle_stroke(r.center(), r.width() * 0.5, egui::Stroke::new(2.0, YELLOW));
        }
        for name in ["MISS_SILENCE_PERIOD", "CLOCK_RESET", "TEST", "MUTE_OFF", "MUTE_ON"] {
            let (clicked, r) = p.spot(ui, name, art.rect(&format!("btn-{name}")));
            match name {
                "MUTE_ON" if clicked => self.r501.mute_on(),
                "MUTE_OFF" if clicked => self.r501.mute_off(),
                "TEST" => {
                    let down = crate::panel::down(&r, "R_501", "TEST");
                    if down != self.r501_test_down {
                        self.r501_test_down = down;
                        self.r501.test(down, clock.secs());
                    }
                }
                "CLOCK_RESET" if clicked => self.r501.clock_reset(),
                "MISS_SILENCE_PERIOD" if clicked => self.r501.miss_silence = !self.r501.miss_silence,
                _ => {}
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Exercise:").color(Color32::GRAY));
            if ui.button("Two-tone alarm heard on 2182 kHz").clicked() {
                self.r501.alarm_signal();
            }
        });
        ui.label(RichText::new(if gmdss::r501::silence_period(minute) { "Silence period now (XX.00-03 / XX.30-33)" } else { "" }).color(YELLOW));
    }

    pub fn show_sart(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(140.0);
                let p = Plate::with_art(ui, "SART", &mut self.sart_art, 140.0);
                let art = &self.sart_art;
                let painter = ui.painter().clone();
                if self.sart.flash() {
                    let r = p.place(art.rect("lamp"));
                    painter.rect_filled(r.expand(4.0), 4.0, Color32::from_rgba_unmultiplied(255, 240, 160, 80));
                    painter.rect_filled(r, 2.0, Color32::from_rgb(255, 250, 210));
                }
                let switch = art.rect("switch");
                if self.sart.state != State::Off {
                    // Pin pulled out: the slot is empty.
                    painter.rect_filled(p.place(switch).shrink(3.0), 2.0, Color32::from_rgb(20, 20, 20));
                }
                if p.spot(ui, "switch", switch).0 {
                    if self.sart.state == State::Off { self.sart.switch_on() } else { self.sart.switch_off() }
                }
            });
            ui.vertical(|ui| {
                let state = match (self.sart.state, self.sart.in_bracket) {
                    (State::Off, true) => "In mounting bracket, off",
                    (State::Off, false) => "Off",
                    (State::Standby, _) => "STANDBY - lamp and beep every 12 s",
                    (State::Transponding, _) => "TRANSPONDING - radar interrogation, every 2 s",
                };
                ui.label(RichText::new(state).size(16.0).color(Color32::WHITE));
                ui.label(format!("Battery: {:.0} h standby left", self.sart.battery / 3600.0));
                ui.add_space(8.0);
                if crate::panel::hook(&ui.button(if self.sart.in_bracket { "Remove from bracket" } else { "Put back in bracket" }), "SART", "BRACKET") {
                    self.sart.in_bracket = !self.sart.in_bracket;
                }
                ui.label(RichText::new("Click the red pin to switch on (seal broken) or off.").small().color(Color32::GRAY));
                if crate::panel::hook(&ui.add_enabled(self.sart.state == State::Off, egui::Button::new("Self test (probe in 3 mm hole)")), "SART", "TEST") && self.sart.state == State::Off {
                    self.sart.self_test(4.0);
                }
                ui.add_space(12.0);
                ui.checkbox(&mut self.radar_in_range, "Exercise: rescue vessel radar within 5 nm");
            });
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }

    pub fn show_epirb(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(330.0);
                let p = Plate::with_art(ui, "EPIRB", &mut self.epirb_art, 330.0);
                let art = &self.epirb_art;
                let painter = ui.painter().clone();
                let (red, strobe) = self.epirb.lamps();
                if strobe {
                    let r = p.place(art.rect("lamp-strobe"));
                    painter.circle_filled(r.center(), r.width() * 0.75, Color32::from_rgba_unmultiplied(255, 255, 255, 90));
                    painter.circle_filled(r.center(), r.width() * 0.5, Color32::WHITE);
                }
                p.lamp_glow(ui, art.rect("lamp-red"), red, RED, "Indicator lamp");
                let sw = art.rect("switch");
                if self.epirb.switch_on {
                    // Pin turned to ON.
                    let r = p.place(sw);
                    painter.circle_filled(r.center(), r.height() * 0.38, Color32::from_rgb(58, 58, 58));
                    painter.rect_filled(egui::Rect::from_center_size(r.center(), Vec2::new(r.height() * 0.7, r.height() * 0.18)), 2.0, Color32::from_rgb(200, 200, 200));
                }
                let half = |right: bool| egui::Rect::from_min_size(sw.min + Vec2::new(if right { sw.width() / 2.0 } else { 0.0 }, 0.0), Vec2::new(sw.width() / 2.0, sw.height()));
                if p.spot(ui, "AUTO", half(false)).0 {
                    self.epirb.switch_on = false;
                }
                if p.spot(ui, "ON", half(true)).0 {
                    self.epirb.switch_on = true;
                }
                if p.spot(ui, "TEST", art.rect("key-TEST")).0 {
                    self.epirb.test();
                }
                if self.epirb.in_water {
                    painter.rect_filled(egui::Rect::from_min_max(p.rect.left_bottom() - Vec2::new(0.0, p.rect.height() * 0.35), p.rect.right_bottom()), 0.0, Color32::from_rgba_unmultiplied(40, 80, 160, 120));
                }
                if self.epirb.in_bracket {
                    painter.rect_stroke(p.rect.shrink(3.0), 8.0, Stroke::new(4.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
                }
            });
            ui.vertical(|ui| {
                let status = if self.epirb.transmitting() { "TRANSMITTING on 406 MHz (and 121.5 MHz homing)" } else { "Not transmitting" };
                ui.label(RichText::new(status).size(16.0).color(if self.epirb.transmitting() { RED } else { Color32::WHITE }));
                ui.add_space(6.0);
                ui.label(RichText::new("Click AUTO or ON beside the switch; TEST is the orange key.").small().color(Color32::GRAY));
                if crate::panel::hook(&ui.button(if self.epirb.in_bracket { "Remove from release mechanism" } else { "Place in release mechanism" }), "EPIRB", "BRACKET") {
                    let b = !self.epirb.in_bracket;
                    self.epirb.set_in_bracket(b);
                }
                if crate::panel::hook(&ui.button(if self.epirb.in_water { "Take out of the water" } else { "Put into the water (or ship sinks)" }), "EPIRB", "WATER") {
                    let w = !self.epirb.in_water;
                    self.epirb.set_in_water(w);
                }
            });
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }

    pub fn show_portable(&mut self, ui: &mut egui::Ui) {
        use portable::Key as K;
        let now = ui.input(|i| i.time);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(250.0);
                let p = Plate::with_art(ui, "VHF_Portable", &mut self.portable_art, 250.0);
                let art = &self.portable_art;
                let r = p.place(art.rect("lcd"));
                let painter = ui.painter().clone();
                let v = &self.portable;
                if v.on {
                    if v.light() {
                        painter.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(170, 230, 120, 110));
                    }
                    let blink = now % 0.6 < 0.3;
                    let sign = |t: Target, s: &str| if v.target() == t && blink { " ".repeat(s.len()) } else { s.to_string() };
                    let lines = [
                        format!("{} {} {}", sign(Target::Power, if v.high_power { "HI" } else { "LO" }), if v.transmitting() { "TX" } else { "  " }, if v.locked { "KEY" } else { "   " }),
                        format!("CH {:02}  {}", v.channel, if v.loud { "SPK" } else { "   " }),
                        format!("{} {} {:>3.0}%", sign(Target::Squelch, &format!("SQ{}", v.squelch)), sign(Target::Volume, &format!("V{}", v.volume)), v.battery * 100.0),
                    ];
                    let h = r.height() / 3.0;
                    let size = (h * 0.75).min(r.width() / (13.0 * 0.6));
                    for (i, l) in lines.iter().enumerate() {
                        painter.text(r.left_top() + Vec2::new(r.width() * 0.05, h * (i as f32 + 0.5)), Align2::LEFT_CENTER, l, egui::FontId::monospace(size), Color32::from_rgb(10, 14, 6));
                    }
                }
                for (name, key) in [
                    ("SPK", K::Speaker), ("LOCK", K::Lock), ("BEEP", K::Beep), ("ON_OFF", K::OnOff), ("PWR", K::Pwr), ("Up", K::Up), ("Down", K::Down),
                    ("VOL", K::Vol), ("CH", K::Ch), ("SQ", K::Sq), ("A", K::A), ("B", K::B), ("16", K::Ch16),
                ] {
                    let (_, r) = p.spot(ui, name, art.rect(&format!("key-{name}")));
                    // Acts on release, so a hold of 1 s (simulation time) gives the long function
                    // (ON/OFF, LOCK, BEEP, A/B programming).
                    let (down, t) = (crate::panel::down(&r, "VHF_Portable", name), self.portable.time);
                    match self.press_start {
                        None if down => self.press_start = Some((key, t)),
                        Some((k, t0)) if k == key && !down => {
                            self.press_start = None;
                            self.portable.press(key, t - t0 >= 1.0);
                        }
                        _ => {}
                    }
                }
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("SIDE CONTROLS").small().color(Color32::GRAY));
                let ptt = ui.add(egui::Button::new("PTT (hold)").min_size(Vec2::new(110.0, 40.0)));
                self.portable.ptt = ptt.is_pointer_button_down_on();
                if crate::panel::hook(&ui.button("LIGHT"), "VHF_Portable", "LIGHT") {
                    self.portable.press(K::Light, false);
                }
                ui.checkbox(&mut self.portable.monitor, "MONITOR");
                ui.add_space(8.0);
                ui.label(RichText::new("Hold ON/OFF, LOCK and BEEP for 1 s.\nProgram A/B: select channel, LOCK (short), then hold A or B.").small().color(Color32::GRAY));
            });
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    }

    pub fn show_charger(&mut self, ui: &mut egui::Ui) {
        let p = Plate::with_art(ui, "Charger", &mut self.charger_art, 700.0);
        let art = &self.charger_art;
        let painter = ui.painter().clone();
        // Needles: pivot 147.6 units below the face's bottom centre, -23.2..+23.2 degrees (as in the art).
        for (id, value, lo, hi) in [("face-A", self.charger.battery_a() as f32, -80.0, 80.0), ("face-V", self.charger.voltage() as f32, 0.0, 30.0)] {
            let face = p.place(art.rect(id));
            let s = face.width() / 164.0;
            let pivot = face.center_bottom() + Vec2::new(0.0, 147.6 * s);
            let a = (-23.2 + 46.4 * (value.clamp(lo, hi) - lo) / (hi - lo)).to_radians();
            let tip = pivot + Vec2::new(a.sin(), -a.cos()) * 200.0 * s;
            painter.with_clip_rect(face).line_segment([pivot, tip], Stroke::new(1.6 * s, RED));
        }
        p.lamp_glow(ui, art.rect("lamp-ON"), self.charger.mains && self.charger.switch != Switch::Off, RED, "Charging");
        // Rocker switch: MAN / OFF / AUTO from the top; the pressed end sinks in.
        let sw = art.rect("switch");
        let r = p.place(sw);
        painter.rect_filled(r, 2.0, Color32::from_rgb(28, 28, 28));
        let half = |top: bool| if top { egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.center().y)) } else { egui::Rect::from_min_max(egui::pos2(r.min.x, r.center().y), r.max) };
        let raised = match self.charger.switch {
            Switch::Manual => Some(false),
            Switch::Auto => Some(true),
            Switch::Off => None,
        };
        match raised {
            Some(top) => painter.rect_filled(half(top).shrink(1.5), 2.0, Color32::from_rgb(95, 95, 95)),
            None => painter.rect_filled(r.shrink(1.5), 2.0, Color32::from_rgb(70, 70, 70)),
        };
        for (i, (name, pos)) in [("MAN", Switch::Manual), ("OFF", Switch::Off), ("AUTO", Switch::Auto)].into_iter().enumerate() {
            let third = egui::Rect::from_min_size(sw.min + Vec2::new(0.0, sw.height() * i as f32 / 3.0), Vec2::new(sw.width(), sw.height() / 3.0));
            if p.spot(ui, name, third).0 {
                self.charger.switch = pos;
            }
        }
        ui.add_space(8.0);
        ui.checkbox(&mut self.charger.mains, "Ship's mains available (exercise: untick for a blackout)");
        ui.label(format!(
            "Battery {:.0}% of {:.0} Ah, {:.1} V, load {:.1} A, charger {:.1} A",
            self.charger.charge * 100.0,
            self.charger.capacity_ah,
            self.charger.voltage(),
            self.charger.load_a,
            self.charger.charging_a()
        ));
    }
}

const HAIFA_NAVTEX: (f64, f64) = (32.82, 35.0);

/// Our own sample broadcasts, used when no `.ntx` files are present.
fn demo_navtex() -> Vec<Message> {
    let m = |b1, b2, number, text: &str| Message { b1, b2, number, text: text.into() };
    vec![
        m('H', 'A', 12, "HAIFA NAV WARNING 12\nEASTERN MEDITERRANEAN.\nUNLIT BUOY ADRIFT VICINITY 32-58N 034-41E.\nVESSELS ARE REQUESTED TO KEEP SHARP LOOKOUT."),
        m('H', 'E', 31, "WEATHER FORECAST FOR EASTERN MEDITERRANEAN\nVALID 24 HOURS.\nWIND NW 4 TO 5, LOCALLY 6 LATER.\nSEA MODERATE. VISIBILITY GOOD."),
        m('H', 'D', 0, "MAYDAY RELAY\nFISHING VESSEL REPORTED SINKING POSITION 33-10N 034-20E.\nVESSELS IN THE VICINITY ARE REQUESTED TO KEEP A SHARP LOOKOUT\nAND REPORT TO MRCC HAIFA."),
        m('H', 'B', 7, "GALE WARNING\nNORTH WESTERLY GALE FORCE 8 EXPECTED WITHIN 12 HOURS\nIN AREA EASTERN MEDITERRANEAN."),
    ]
}

/// Loads NAVTEX messages from `<B1><B2><nn>.ntx` files (the original's naming).
pub fn load_ntx(dir: &std::path::Path) -> Vec<Message> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_uppercase();
        let stem: Vec<char> = name.trim_end_matches(".NTX").chars().collect();
        if !name.ends_with(".NTX") || stem.len() < 4 {
            continue;
        }
        let n = stem.len();
        if let (Ok(number), Ok(text)) = (stem[n - 2..].iter().collect::<String>().parse(), std::fs::read(e.path())) {
            out.push(Message { b1: stem[n - 4], b2: stem[n - 3], number, text: gmdss::dos::latin1(&text).replace('\r', "") });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The small units drawn at their originals' sizes, for comparison.
    #[test]
    fn panel_snapshots() {
        let mut u = Units::new(vec![]);
        let clock = SimClock::new();
        u.r501.af_gain = 6;
        crate::snapshot::render("r501", egui::vec2(466.0, 304.0), |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            u.show_r501(ui, &clock);
        });
        u.navtex.press(navtex::Key::Power);
        crate::snapshot::render("navtex", egui::vec2(640.0, 520.0), |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            u.show_navtex(ui);
        });
        u.portable.press(portable::Key::OnOff, true);
        crate::snapshot::render("portable", egui::vec2(420.0, 820.0), |ui| u.show_portable(ui));
        u.sart.switch_on();
        crate::snapshot::render("sart", egui::vec2(560.0, 460.0), |ui| u.show_sart(ui));
        u.epirb.switch_on = true;
        crate::snapshot::render("epirb", egui::vec2(700.0, 400.0), |ui| u.show_epirb(ui));
        u.charger.switch = Switch::Auto;
        crate::snapshot::render("charger", egui::vec2(564.0, 200.0), |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            u.show_charger(ui);
        });
    }
}

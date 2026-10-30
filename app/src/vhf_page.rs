//! VHF installation: the RT2048 radiotelephone above the DSC controller, as on the original console.

use crate::clock::SimClock;
use crate::panel::Plate;
use crate::{dsc_panel, Shared, Ship};
use eframe::egui::{self, Color32};
use gmdss::dsc::{Band, Controller, Eos, Position};
use gmdss::vhf::{Channel, Key, VhfRadio};

pub struct VhfStation {
    art: crate::svg_art::Art,
    vhf: VhfRadio,
    vol: u8,
    dsc: Controller,
    dsc_vol: u8,
    dsc_art: crate::svg_art::Art,
    distress_sent: u32,
    distress_acked: u32,
}

impl VhfStation {
    pub fn new(ship: &Ship) -> VhfStation {
        let mut dsc = Controller::new(&ship.mmsi, Band::Vhf);
        dsc.position = Some(Position { lat_min: ship.lat_min, lon_min: ship.lon_min });
        VhfStation { art: crate::svg_art::Art::new("vhf_rt2048", include_str!("../assets/panels/vhf_rt2048.svg")), vhf: VhfRadio::default(), vol: 0, dsc, dsc_vol: 0, dsc_art: dsc_panel::art("vhf_dsc", "VHF DSC RM2042"), distress_sent: 0, distress_acked: 0 }
    }

    /// Applies the ship's identity and position (Options may change them).
    pub fn sync_ship(&mut self, ship: &Ship) {
        if self.dsc.mmsi != ship.mmsi {
            self.dsc.mmsi = ship.mmsi.clone();
        }
        self.dsc.position = Some(Position { lat_min: ship.lat_min, lon_min: ship.lon_min });
    }

    /// Current drawn from the 24 V supply (A).
    pub fn load_a(&self) -> f64 {
        let radio = if !self.vhf.on { 0.0 } else if self.vhf.transmitting() { 5.5 } else { 0.6 };
        radio + if self.dsc_vol > 0 { 0.3 } else { 0.0 }
    }

    /// Runs the units (also while another device is shown).
    pub fn tick(&mut self, clock: &SimClock, shared: &mut Shared) {
        let now = clock.secs();
        let t = clock.now();
        self.vhf.tick(now, &|_| false);
        if self.dsc_vol > 0 {
            let out = self.dsc.tick(now, (t.hour, t.minute, (t.seconds % 60) as u8));
            self.transmit(out, clock, shared);
        }
        for call in shared.world.tick_band(now, false) {
            if self.dsc_vol == 0 {
                continue;
            }
            shared.note(clock, format!("VHF DSC received: {}", dsc_panel::describe(&call)));
            if call.tc1 == gmdss::dsc::Telecommand::DistressAck && call.distress_ship.as_deref() == Some(self.dsc.mmsi.as_str()) {
                self.distress_acked += 1;
            }
            if let (Some(ch), Eos::Ack) = (call.channel, call.eos) {
                self.vhf.set_channel(Channel::Intl(ch)); // the DSC unit tunes the VHF to the working channel
            }
            self.dsc.receive(call);
        }
    }

    fn transmit(&mut self, calls: Vec<gmdss::dsc::Call>, clock: &SimClock, shared: &mut Shared) {
        for call in calls {
            if self.vhf.on {
                shared.note(clock, format!("VHF DSC sent: {}", dsc_panel::describe(&call)));
                if call.format == gmdss::dsc::Format::Distress {
                    self.distress_sent += 1;
                }
                shared.world.transmit(&call, clock.secs(), shared.ship_pos, shared.hour);
            } else {
                self.dsc.tx_failed();
            }
        }
    }

    /// The DSC controller is sounding its alarm.
    pub fn dsc_alarm(&self) -> bool {
        self.dsc_vol > 0 && self.dsc.alarm
    }

    pub fn show(&mut self, ui: &mut egui::Ui, clock: &SimClock, shared: &mut Shared) {
        // Both units in view: the radiotelephone above the DSC controller.
        let w = crate::panel::fit_width(ui, 225.0 / 471.0, 2.0, 12.0, 640.0);
        self.rt2048(ui, w);
        ui.add_space(10.0);
        let out = dsc_panel::show(ui, "VHF_DSC", &mut self.dsc_art, &mut self.dsc, &mut self.dsc_vol, w);
        self.transmit(out, clock, shared);
    }

    fn rt2048(&mut self, ui: &mut egui::Ui, width: f32) {
        let p = Plate::with_art(ui, "VHF", &mut self.art, width);
        let art = &self.art;
        p.knob_spot(ui, "SQ", art.rect("knob-SQ"), &mut self.vhf.squelch, 10);
        if p.knob_spot(ui, "VOL_OFF", art.rect("knob-VOL_OFF"), &mut self.vol, 10) {
            self.vhf.power(self.vol > 0);
            self.vhf.volume = self.vol;
        }
        // Channel display: the LCD lights up with the radio; annunciators along its lower edge.
        let glass = p.place(art.rect("lcd"));
        let painter = ui.painter().clone();
        if self.vhf.on && self.vhf.dim > 0 {
            painter.rect_filled(glass, 0.0, Color32::from_rgba_unmultiplied(230, 240, 150, 40));
        }
        if self.vhf.on {
            let ink = crate::panel::LCD_INK;
            painter.text(glass.center() - egui::vec2(0.0, glass.height() * 0.08), egui::Align2::CENTER_CENTER, self.vhf.display(), p.mono(22.0), ink);
            for (i, (label, on)) in [("1W", self.vhf.low_power), ("TX", self.vhf.transmitting()), ("US", self.vhf.us_mode)].into_iter().enumerate() {
                if on {
                    painter.text(glass.left_bottom() + egui::vec2(glass.width() * (0.1 + 0.3 * i as f32), -glass.height() * 0.06), egui::Align2::LEFT_BOTTOM, label, p.font(6.0), ink);
                }
            }
            if self.vhf.shift {
                painter.rect_filled(egui::Rect::from_min_size(glass.min + glass.size() * egui::vec2(0.06, 0.08), glass.size() * egui::vec2(0.16, 0.05)), 0.0, ink);
            }
        }
        self.vhf.ptt = handset(ui, &p, art.rect("handset"));
        for (name, key) in [
            ("1", Key::Digit(1)), ("2", Key::Digit(2)), ("3", Key::Digit(3)), ("4", Key::Digit(4)), ("5", Key::Digit(5)),
            ("6", Key::Digit(6)), ("7", Key::Digit(7)), ("8", Key::Digit(8)), ("9", Key::Digit(9)), ("0", Key::Digit(0)),
            ("SHIFT", Key::Shift), ("16", Key::Ch16),
        ] {
            if p.spot(ui, name, art.rect(&format!("key-{name}"))).0 {
                self.vhf.press(key);
            }
        }
    }
}

/// The handset rest drawn by the artwork at `r` (SVG units): hold it to talk; true while held.
pub fn handset(ui: &mut egui::Ui, p: &Plate, r: egui::Rect) -> bool {
    let r = p.place(r);
    let resp = ui.interact(r, ui.id().with("handset"), egui::Sense::click_and_drag());
    let down = resp.is_pointer_button_down_on();
    if down {
        ui.painter().circle_filled(r.center(), r.width() / 2.0, Color32::from_rgba_unmultiplied(120, 0, 0, 70));
    }
    resp.on_hover_text("Handset: hold to talk");
    down
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmdss::script::Mode;

    /// Draws the RT2048 at the original's 471 x 225 so it can be laid over it.
    #[test]
    fn rt2048_snapshot() {
        let mut vhf = VhfStation::new(&crate::default_ship());
        vhf.vol = 5;
        vhf.vhf.power(true);
        let path = crate::snapshot::render("vhf_rt2048", egui::vec2(471.0, 225.0), |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            vhf.rt2048(ui, 640.0);
        });
        assert!(path.exists());
        // And at the size the app shows it.
        crate::snapshot::render("vhf_rt2048_large", egui::vec2(640.0, 306.0), |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            vhf.rt2048(ui, 640.0);
        });
    }

    /// Lesson 1 demonstrated on the real panel, frame by frame: the radio ends on channel 16 with
    /// the distress alert sent and acknowledged, and the alarm silenced.
    #[test]
    fn distress_lesson_demonstration() {
        let ship = crate::default_ship();
        let ctx = egui::Context::default();
        let mut vhf = VhfStation::new(&ship);
        let mut clock = SimClock::new();
        let mut shared = Shared {
            world: gmdss::world::World::new(gmdss::world::CoastDsc { name: "HAIFA RADIO".into(), mmsi: "004281000".into(), pos: (32.82, 35.0) }),
            log: vec![],
            ship_pos: (f64::from(ship.lat_min) / 60.0, f64::from(ship.lon_min) / 60.0),
            hour: 12,
            printer: String::new(),
        };
        let mut tutor = crate::tutor_page::Tutor::new();
        clock.tick(0.0);
        tutor.start(&ctx, 0, Mode::Demo, 0.0);
        let mut sent = false;
        for frame in 1..4000 {
            clock.tick(f64::from(frame) * 0.05);
            let now = clock.secs();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                vhf.tick(&clock, &mut shared);
                tutor.before(ui.ctx(), now);
                vhf.show(ui, &clock, &mut shared);
                tutor.after(ui.ctx(), now);
            });
            out.textures_delta.clear();
            sent |= vhf.dsc.distress_active();
        }
        assert!(vhf.vol > 0 && vhf.dsc_vol > 0, "both units switched on");
        assert_eq!(vhf.vhf.display().trim(), "16");
        assert!(sent, "distress alert sent: {:?}", shared.log);
        assert!(shared.log.iter().any(|l| l.contains("acknowledgement")), "{:?}", shared.log);
        assert!(!vhf.dsc.alarm, "alarm silenced");
    }
}

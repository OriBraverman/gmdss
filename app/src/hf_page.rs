//! MF/HF installation: the RE2100 SSB transceiver above the MF/HF DSC unit RM2150; the
//! radiotelex station's DSC / telex unit RM2151 works the same transceiver.

use crate::clock::SimClock;
use crate::panel::Plate;
use crate::{dsc_panel, Shared, Ship};
use eframe::egui::{self, Align2, Color32, RichText};
use gmdss::dsc::{rt_frequency, Band, Controller, Eos, Position, Telecommand};
use gmdss::hf::{HfRadio, Key, Mode};

pub struct HfStation {
    hf: HfRadio,
    vol: u8,
    dsc: Controller,
    dsc_vol: u8,
    art: crate::svg_art::Art,
    dsc_art: crate::svg_art::Art,
    /// The radiotelex station's DSC / telex unit, a DSC controller of its own.
    rm2151: Controller,
    rm2151_vol: u8,
    rm2151_art: crate::svg_art::Art,
}

impl HfStation {
    pub fn new(ship: &Ship) -> HfStation {
        let unit = || {
            let mut c = Controller::new(&ship.mmsi, Band::MfHf);
            c.position = Some(Position { lat_min: ship.lat_min, lon_min: ship.lon_min });
            c
        };
        HfStation {
            art: crate::svg_art::Art::new("hf_re2100", include_str!("../assets/panels/hf_re2100.svg")),
            hf: HfRadio::default(),
            vol: 0,
            dsc: unit(),
            dsc_vol: 0,
            dsc_art: dsc_panel::art("hf_dsc", "MF/HF DSC RM2150"),
            rm2151: unit(),
            rm2151_vol: 0,
            rm2151_art: dsc_panel::art("hf_tlx_dsc", "MF/HF DSC / TELEX RM2151"),
        }
    }

    /// Applies the ship's identity and position (Options may change them).
    pub fn sync_ship(&mut self, ship: &Ship) {
        for c in [&mut self.dsc, &mut self.rm2151] {
            if c.mmsi != ship.mmsi {
                c.mmsi = ship.mmsi.clone();
            }
            c.position = Some(Position { lat_min: ship.lat_min, lon_min: ship.lon_min });
        }
    }

    /// Current drawn from the 24 V supply (A).
    pub fn load_a(&self) -> f64 {
        let radio = if !self.hf.on { 0.0 } else if self.hf.transmitting() { 20.0 } else { 2.0 };
        radio + [self.dsc_vol, self.rm2151_vol].iter().map(|&v| if v > 0 { 0.3 } else { 0.0 }).sum::<f64>()
    }

    pub fn tick(&mut self, clock: &SimClock, shared: &mut Shared) {
        let now = clock.secs();
        let t = clock.now();
        self.hf.tick(now);
        let utc = (t.hour, t.minute, (t.seconds % 60) as u8);
        if self.dsc_vol > 0 {
            let out = self.dsc.tick(now, utc);
            Self::transmit(&self.hf, &mut self.dsc, out, clock, shared);
        }
        if self.rm2151_vol > 0 {
            let out = self.rm2151.tick(now, utc);
            Self::transmit(&self.hf, &mut self.rm2151, out, clock, shared);
        }
        for call in shared.world.tick_band(now, true) {
            if self.rm2151_vol > 0 {
                self.rm2151.receive(call.clone());
            }
            if self.dsc_vol == 0 {
                continue;
            }
            shared.note(clock, format!("MF/HF DSC received: {}", dsc_panel::describe(&call)));
            if call.tc1 == Telecommand::DistressAck && call.distress_ship.as_deref() == Some(self.dsc.mmsi.as_str()) {
                // Distress traffic continues by radiotelephony on the paired frequency.
                let f = rt_frequency(call.freq_khz.unwrap_or(2187.5));
                self.hf.set(f, f, if f == 2182.0 { Mode::H3E } else { Mode::J3E });
            } else if let (Some(f), Eos::Ack) = (call.freq_khz, call.eos) {
                self.hf.set(f, f, Mode::J3E);
            }
            self.dsc.receive(call);
        }
    }

    /// Calls a DSC unit sends go out through the transceiver, if it is on.
    fn transmit(hf: &HfRadio, unit: &mut Controller, calls: Vec<gmdss::dsc::Call>, clock: &SimClock, shared: &mut Shared) {
        for call in calls {
            if hf.on {
                shared.note(clock, format!("MF/HF DSC sent: {}", dsc_panel::describe(&call)));
                shared.world.transmit(&call, clock.secs(), shared.ship_pos, shared.hour);
            } else {
                unit.tx_failed();
            }
        }
    }

    /// The DSC controller is sounding its alarm.
    pub fn dsc_alarm(&self) -> bool {
        (self.dsc_vol > 0 && self.dsc.alarm) || (self.rm2151_vol > 0 && self.rm2151.alarm)
    }

    /// The set sends or tests the two-tone alarm signal.
    pub fn two_tone(&self) -> bool {
        self.hf.alarm_sound()
    }

    pub fn show(&mut self, ui: &mut egui::Ui, clock: &SimClock, shared: &mut Shared) {
        let w = crate::panel::fit_width(ui, 225.0 / 471.0, 2.0, 40.0, 640.0);
        self.re2100(ui, w);
        let ptt = ui.add(egui::Button::new(RichText::new("  Handset PTT (hold)  ").color(Color32::LIGHT_GRAY)));
        self.hf.ptt = ptt.is_pointer_button_down_on() && self.hf.on;
        ui.add_space(6.0);
        let out = dsc_panel::show(ui, "HF_DSC", &mut self.dsc_art, &mut self.dsc, &mut self.dsc_vol, w);
        Self::transmit(&self.hf, &mut self.dsc, out, clock, shared);
    }

    /// The radiotelex station's DSC / telex unit RM2151 (lesson scope `HF_Tlx`).
    pub fn show_rm2151(&mut self, ui: &mut egui::Ui, clock: &SimClock, shared: &mut Shared) {
        let out = dsc_panel::show(ui, "HF_Tlx", &mut self.rm2151_art, &mut self.rm2151, &mut self.rm2151_vol, 420.0);
        Self::transmit(&self.hf, &mut self.rm2151, out, clock, shared);
    }

    fn re2100(&mut self, ui: &mut egui::Ui, width: f32) {
        let p = Plate::with_art(ui, "HF", &mut self.art, width);
        let art = &self.art;
        if p.knob_spot(ui, "VOL", art.rect("knob-VOL"), &mut self.vol, 10) {
            self.hf.on = self.vol > 0;
            self.hf.volume = self.vol;
        }
        p.knob_spot(ui, "RF", art.rect("knob-RF"), &mut self.hf.rf_gain, 10);
        let on = self.hf.on;
        let painter = ui.painter().clone();
        let lit = Color32::from_rgb(0xff, 0x41, 0x20);
        // Signal (receive) or aerial current (transmit): ten meter segments.
        let level = if !on {
            0.0
        } else if self.hf.transmitting() {
            0.3 * f32::from(self.hf.power)
        } else {
            0.08 * f32::from(self.hf.rf_gain) * (0.7 + 0.3 * ((ui.input(|i| i.time) * 7.0).sin() as f32).abs())
        };
        let meter = art.rect("meter");
        for i in 0..(level.min(1.0) * 10.0).round() as usize {
            let seg = egui::Rect::from_min_size(meter.min + egui::vec2(6.0 * i as f32, 0.0), egui::vec2(5.0, meter.height()));
            painter.rect_filled(p.place(seg), 0.0, lit);
        }
        // Transmitter power: the ramp fills up to the power step (1-3).
        if on && self.hf.power > 0 {
            let ramp = p.place(art.rect("ramp"));
            let f = f32::from(self.hf.power) / 3.0;
            let c = if self.hf.transmitting() { lit } else { Color32::from_rgb(110, 30, 20) };
            painter.add(egui::Shape::convex_polygon(vec![ramp.left_bottom(), ramp.left_bottom() + egui::vec2(ramp.width() * f, 0.0), ramp.left_bottom() + egui::vec2(ramp.width() * f, -ramp.height() * f)], c, egui::Stroke::NONE));
        }
        if on {
            let (rx, tx) = self.hf.display();
            for (id, text) in [("freq-rx", rx), ("freq-tx", tx)] {
                let r = p.place(art.rect(id));
                painter.text(r.right_center(), Align2::RIGHT_CENTER, text, p.mono(16.0), lit);
            }
        }
        let modes = p.place(art.rect("modes"));
        for (i, (name, active)) in [
            ("J3E", self.hf.mode == Mode::J3E),
            ("R3E", self.hf.mode == Mode::R3E),
            ("H3E", self.hf.mode == Mode::H3E),
            ("LSB", self.hf.mode == Mode::Lsb),
            ("SQ", self.hf.squelch),
            ("AGC", self.hf.agc),
            ("DUMMY", self.hf.dummy_load),
            ("TUNE", self.hf.tuning()),
        ]
        .into_iter()
        .enumerate()
        {
            let x = modes.left() + modes.width() * i as f32 / 8.0;
            painter.text(egui::pos2(x, modes.center().y), Align2::LEFT_CENTER, name, p.font(5.0), if on && active { lit } else { Color32::from_gray(55) });
        }
        if self.hf.alarm_sound() {
            painter.text(p.place(art.rect("alarm")).left_center(), Align2::LEFT_CENTER, "ALARM SIGNAL", p.font(6.0), lit);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }

        let keys = [
            ("FREQ_Down", Key::FreqDown), ("FREQ_Up", Key::FreqUp), ("TUNE_CLARIF", Key::TuneClarif), ("RX", Key::Rx), ("CH", Key::Ch),
            ("AGC_POWER", Key::AgcPower), ("DIM_DUMMY_LOAD", Key::DimDummy), ("SEND_ALARM", Key::SendAlarm), ("2182", Key::Ch2182),
            ("TEST_ALARM", Key::TestAlarm), ("TX", Key::Tx), ("SC", Key::Scan), ("SQ_ADD", Key::SqAdd), ("MODE_DEL", Key::ModeDel),
            ("1", Key::Digit(1)), ("2", Key::Digit(2)), ("3", Key::Digit(3)), ("4", Key::Digit(4)), ("5", Key::Digit(5)),
            ("6", Key::Digit(6)), ("7", Key::Digit(7)), ("8", Key::Digit(8)), ("9", Key::Digit(9)), ("0", Key::Digit(0)),
            ("Dot", Key::Dot), ("STOP_ENT", Key::StopEnt),
        ];
        for (name, key) in keys {
            if p.spot(ui, name, art.rect(&format!("key-{name}"))).0 {
                self.hf.press(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws the RE2100 and its DSC unit at the originals' 471 x 225 for comparison.
    #[test]
    fn panel_snapshots() {
        let mut hf = HfStation::new(&crate::default_ship());
        hf.vol = 5;
        hf.hf.on = true;
        hf.dsc_vol = 5;
        crate::snapshot::render("hf_re2100", egui::vec2(471.0, 225.0), |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            hf.re2100(ui, 640.0);
        });
        crate::snapshot::render("hf_dsc", egui::vec2(471.0, 225.0), |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            dsc_panel::show(ui, "HF_DSC", &mut hf.dsc_art, &mut hf.dsc, &mut hf.dsc_vol, 640.0);
        });
    }
}

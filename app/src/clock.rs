//! Simulation clock (starts at the computer's UTC, can run faster) and the identity box.

use crate::Ship;
use eframe::egui::{self, Color32, RichText};
use gmdss::telex::SimTime;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SimClock {
    start_unix: f64,
    /// Simulated seconds since start.
    elapsed: f64,
    pub speed: f64,
    last_real: Option<f64>,
}

impl SimClock {
    pub fn new() -> SimClock {
        let start_unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        SimClock { start_unix, elapsed: 0.0, speed: 1.0, last_real: None }
    }

    /// Advances by the real time since the last call, times `speed`.
    pub fn tick(&mut self, real_now: f64) {
        if let Some(last) = self.last_real {
            self.elapsed += (real_now - last).max(0.0) * self.speed;
        }
        self.last_real = Some(real_now);
    }

    /// Simulated seconds since start, for timers.
    pub fn secs(&self) -> f64 {
        self.elapsed
    }

    pub fn now(&self) -> SimTime {
        let secs = (self.start_unix + self.elapsed) as u64;
        let (year, month, day) = civil_from_days((secs / 86_400) as i64);
        let in_day = secs % 86_400;
        SimTime {
            seconds: secs as u32,
            year,
            month,
            day,
            hour: (in_day / 3600) as u8,
            minute: (in_day / 60 % 60) as u8,
        }
    }
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (u16, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    (year, month, day)
}

pub fn show(ui: &mut egui::Ui, clock: &mut SimClock, ship: &Ship) {
    let t = clock.now();
    egui::Frame::new().fill(Color32::from_rgb(196, 190, 120)).inner_margin(8).corner_radius(4).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let ink = Color32::from_rgb(30, 30, 20);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:02} {:02}", t.hour, t.minute)).size(30.0).monospace().color(ink));
            ui.label(RichText::new(format!("UTC {:02}.{:02}", t.day, t.month)).monospace().color(ink));
        });
        for (k, v) in [("Ship", &ship.name), ("Call sign", &ship.call_sign), ("MMSI", &ship.mmsi), ("Telex", &ship.telex)] {
            ui.label(RichText::new(format!("{k:<10}{v}")).monospace().color(ink));
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Speed").small().color(Color32::GRAY));
        for s in [1.0, 2.0, 5.0, 10.0] {
            if ui.selectable_label(clock.speed == s, format!("{s}x")).clicked() {
                clock.speed = s;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(10_271), (1998, 2, 14));
        assert_eq!(civil_from_days(20_737), (2026, 10, 11));
    }

    #[test]
    fn speed_scales_time() {
        let mut c = SimClock::new();
        c.tick(10.0);
        c.speed = 5.0;
        c.tick(12.0);
        assert_eq!(c.secs(), 10.0);
    }
}

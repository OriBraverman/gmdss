//! 406 MHz float-free satellite EPIRB in its release mechanism.
//!
//! The AUTO/ON switch (under a cover) is normally AUTO: the beacon starts 5 s after it is in the
//! water (sea-water switch), e.g. after the hydrostatic release frees it when the ship sinks.
//! ON transmits even in the bracket. Out of the water it stops after 5 s. TEST (press ~1 s):
//! red lamp flashes once, after ~14 s strobe and red lamp flash for ~15 s, red lamp keeps
//! flashing until ~75 s. A transmitting beacon is heard by COSPAS-SARSAT.

#[derive(Debug, Clone, Default)]
pub struct Epirb {
    /// Switch: false = AUTO, true = ON.
    pub switch_on: bool,
    pub in_bracket: bool,
    pub in_water: bool,
    water_since: Option<f64>,
    dry_since: Option<f64>,
    active: bool,
    test_started: Option<f64>,
    time: f64,
    /// When the beacon started transmitting this time.
    pub active_since: Option<f64>,
}

impl Epirb {
    pub fn new() -> Epirb {
        Epirb { in_bracket: true, ..Default::default() }
    }

    pub fn transmitting(&self) -> bool {
        self.active
    }

    pub fn test(&mut self) {
        if !self.active {
            self.test_started = Some(self.time);
        }
    }

    /// Takes it out of (or puts it back into) the release mechanism.
    pub fn set_in_bracket(&mut self, b: bool) {
        self.in_bracket = b;
        if b {
            self.set_in_water(false);
        }
    }

    pub fn set_in_water(&mut self, w: bool) {
        if w != self.in_water {
            self.in_water = w;
            if w {
                self.in_bracket = false;
                self.water_since = Some(self.time);
            } else {
                self.dry_since = Some(self.time);
            }
        }
    }

    pub fn tick(&mut self, now: f64) {
        self.time = now;
        let wet = self.in_water && self.water_since.is_some_and(|t| now - t >= 5.0);
        let dry_long = !self.in_water && self.dry_since.map_or(true, |t| now - t >= 5.0);
        let was = self.active;
        self.active = self.switch_on || wet || (self.active && !dry_long);
        if self.active && !was {
            self.active_since = Some(now);
        }
        if !self.active {
            self.active_since = None;
        }
        if self.test_started.is_some_and(|t| now - t > 75.0) {
            self.test_started = None;
        }
    }

    /// (red indicator lamp, strobe) lit now.
    pub fn lamps(&self) -> (bool, bool) {
        let blink = |period: f64| self.time % period < period / 4.0;
        if self.active {
            return (blink(1.0), blink(1.5));
        }
        match self.test_started.map(|t| self.time - t) {
            Some(dt) if dt < 0.5 => (true, false),
            Some(dt) if (14.0..29.0).contains(&dt) => (blink(1.0), blink(1.5)),
            Some(dt) if dt >= 29.0 => (blink(1.0), false),
            _ => (false, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_free_activation_and_test() {
        let mut e = Epirb::new();
        e.tick(0.0);
        e.set_in_water(true);
        e.tick(3.0);
        assert!(!e.transmitting());
        e.tick(5.5);
        assert!(e.transmitting() && e.active_since == Some(5.5));
        e.set_in_water(false);
        e.tick(8.0);
        assert!(e.transmitting());
        e.tick(11.0);
        assert!(!e.transmitting());
        e.test(); // at 11.0
        e.tick(11.2);
        assert_eq!(e.lamps(), (true, false)); // red flashes once
        e.tick(20.0);
        assert_eq!(e.lamps(), (false, false));
        e.tick(27.0);
        assert_eq!(e.lamps(), (true, true)); // strobe and red after ~14 s
        e.switch_on = true;
        e.tick(100.0);
        assert!(e.transmitting());
    }
}

//! Search and rescue radar transponder ("RT9" type SART).
//!
//! Switched on, it waits in standby (lamp flash and beep every 12 s). A 9 GHz radar pulse makes it
//! transpond (flash/beep every 2 s); after 15 s without pulses it returns to standby. The self
//! test (probe in the test hole) flashes after a delay of 2-12 s. Battery: 96 h standby, then 8 h
//! transponding.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Off,
    Standby,
    Transponding,
}

#[derive(Debug, Clone)]
pub struct Sart {
    pub in_bracket: bool,
    pub state: State,
    last_pulse: f64,
    test_flash_at: Option<f64>,
    /// Remaining battery as standby-seconds (transponding uses 12 x as fast).
    pub battery: f64,
    time: f64,
}

impl Default for Sart {
    fn default() -> Self {
        Sart { in_bracket: true, state: State::Off, last_pulse: f64::NEG_INFINITY, test_flash_at: None, battery: 96.0 * 3600.0, time: 0.0 }
    }
}

impl Sart {
    pub fn switch_on(&mut self) {
        if self.battery > 0.0 {
            self.state = State::Standby;
        }
    }

    pub fn switch_off(&mut self) {
        self.state = State::Off;
    }

    /// Probe in the test hole; `delay` (2-12 s) until the response.
    pub fn self_test(&mut self, delay: f64) {
        self.test_flash_at = Some(self.time + delay.clamp(2.0, 12.0));
    }

    /// A radar sweep reached the SART.
    pub fn radar_pulse(&mut self) {
        if self.state != State::Off {
            self.state = State::Transponding;
            self.last_pulse = self.time;
        }
    }

    pub fn tick(&mut self, now: f64) {
        let dt = (now - self.time).max(0.0);
        self.time = now;
        match self.state {
            State::Transponding if now - self.last_pulse > 15.0 => self.state = State::Standby,
            State::Transponding => self.battery -= dt * 12.0,
            State::Standby => self.battery -= dt,
            State::Off => {}
        }
        if self.battery <= 0.0 {
            self.battery = 0.0;
            self.state = State::Off;
        }
    }

    /// Lamp lit now (with the beep).
    pub fn flash(&self) -> bool {
        let period = match self.state {
            State::Off => return self.test_flash_at.is_some_and(|t| self.time >= t && self.time < t + 3.0 && (self.time - t) % 1.0 < 0.3),
            State::Standby => 12.0,
            State::Transponding => 2.0,
        };
        self.time % period < 0.3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standby_transpond_and_back() {
        let mut s = Sart::default();
        s.switch_on();
        s.tick(1.0);
        s.radar_pulse();
        assert_eq!(s.state, State::Transponding);
        s.tick(10.0);
        assert_eq!(s.state, State::Transponding);
        s.tick(17.0);
        assert_eq!(s.state, State::Standby);
        let mut t = Sart::default();
        t.self_test(5.0);
        t.tick(3.0);
        assert!(!t.flash());
        t.tick(5.1);
        assert!(t.flash());
    }
}

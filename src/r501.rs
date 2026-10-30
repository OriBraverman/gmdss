//! 2182 kHz watchkeeping receiver ("R501"): listens for the radiotelephone alarm signal.
//!
//! AF GAIN turns it on/off and sets the volume. MUTE ON silences the loudspeaker until a two-tone
//! alarm is received (then traffic is heard); MUTE OFF un-mutes. TEST held for 6 s sounds the
//! tone through the mute circuit. MISS SILENCE PERIOD also mutes during the silence periods
//! (minutes 00-03 and 30-33 of each hour); CLOCK RESET aligns the timer at XX.00 / XX.30.

#[derive(Debug, Clone, Default)]
pub struct R501 {
    /// 0 = off.
    pub af_gain: u8,
    pub muted: bool,
    pub miss_silence: bool,
    /// An alarm signal (or the test) opened the muted loudspeaker.
    pub alarm_open: bool,
    test_since: Option<f64>,
    /// Seconds the internal clock is off from the ship's clock (CLOCK RESET sets 0).
    pub clock_error: f64,
}

/// Silence periods: three minutes twice an hour, from XX.00 and XX.30.
pub fn silence_period(minute: u8) -> bool {
    minute % 30 < 3
}

impl R501 {
    pub fn on(&self) -> bool {
        self.af_gain > 0
    }

    pub fn mute_on(&mut self) {
        self.muted = true;
        self.alarm_open = false;
    }

    pub fn mute_off(&mut self) {
        self.muted = false;
        self.alarm_open = false;
    }

    pub fn clock_reset(&mut self) {
        self.clock_error = 0.0;
    }

    /// TEST pressed (true) or released (false) at `now`.
    pub fn test(&mut self, pressed: bool, now: f64) {
        self.test_since = pressed.then_some(now);
    }

    /// The two-tone alarm signal is heard on 2182 kHz.
    pub fn alarm_signal(&mut self) {
        if self.on() && self.muted {
            self.alarm_open = true;
        }
    }

    pub fn tick(&mut self, now: f64) {
        if self.test_since.is_some_and(|t| now - t >= 6.0) {
            self.alarm_signal();
        }
    }

    /// Whether the loudspeaker plays at ship's time `minute`.
    pub fn loudspeaker(&self, minute: u8) -> bool {
        self.on() && (!self.muted || self.alarm_open) && !(self.miss_silence && silence_period(minute))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_alarm_and_test() {
        let mut r = R501 { af_gain: 5, ..Default::default() };
        assert!(r.loudspeaker(10));
        r.mute_on();
        assert!(!r.loudspeaker(10));
        r.alarm_signal();
        assert!(r.loudspeaker(10));
        r.mute_on();
        r.test(true, 100.0);
        r.tick(103.0);
        assert!(!r.loudspeaker(10));
        r.tick(106.5);
        assert!(r.loudspeaker(10));
        r.mute_off();
        r.miss_silence = true;
        assert!(!r.loudspeaker(31) && r.loudspeaker(34));
    }
}

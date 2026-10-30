//! Portable two-way VHF ("SP3110" type, GMDSS survival craft radio).
//!
//! Keys: ON/OFF and keyboard LOCK need a long press (1 s). PWR, CH, SQ, VOL select what the
//! UP/DOWN keys change for 2.5 s (default: volume). A / B / 16 are quick channel keys; A and B
//! are programmed by selecting a channel, a short press on LOCK, then a long press on A or B.
//! Side switches: PTT, MONITOR (open squelch), backlight (off after 30 s).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    OnOff,
    Lock,
    Speaker,
    Beep,
    Pwr,
    Ch,
    Sq,
    Vol,
    Up,
    Down,
    A,
    B,
    Ch16,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Volume,
    Power,
    Channel,
    Squelch,
}

/// Channels fitted (simplex voice channels; 16 is mandatory).
pub const CHANNELS: [u8; 15] = [6, 8, 10, 12, 13, 14, 15, 16, 17, 67, 69, 71, 72, 73, 77];

#[derive(Debug, Clone)]
pub struct Portable {
    pub on: bool,
    pub locked: bool,
    pub loud: bool,
    pub beep: bool,
    pub high_power: bool,
    pub channel: u8,
    pub volume: u8,
    pub squelch: u8,
    pub quick: [u8; 2],
    pub ptt: bool,
    pub monitor: bool,
    /// Battery charge 0-1 (8 h at 1:9 duty cycle).
    pub battery: f64,
    target: Target,
    target_until: f64,
    lock_pressed: f64,
    light_until: f64,
    /// Simulation time of the last tick.
    pub time: f64,
}

impl Default for Portable {
    fn default() -> Self {
        Portable {
            on: false,
            locked: false,
            loud: true,
            beep: true,
            high_power: true,
            channel: 16,
            volume: 4,
            squelch: 3,
            quick: [6, 13],
            ptt: false,
            monitor: false,
            battery: 1.0,
            target: Target::Volume,
            target_until: 0.0,
            lock_pressed: f64::NEG_INFINITY,
            light_until: 0.0,
            time: 0.0,
        }
    }
}

impl Portable {
    /// What UP/DOWN change now (its sign blinks in the display).
    pub fn target(&self) -> Target {
        if self.time < self.target_until { self.target } else { Target::Volume }
    }

    pub fn light(&self) -> bool {
        self.on && self.time < self.light_until
    }

    pub fn transmitting(&self) -> bool {
        self.on && self.ptt && self.battery > 0.0
    }

    pub fn press(&mut self, key: Key, long: bool) {
        if key == Key::OnOff {
            if long {
                self.on = !self.on;
            }
            return;
        }
        if !self.on {
            return;
        }
        match key {
            Key::Lock if long => self.locked = !self.locked,
            Key::Lock => self.lock_pressed = self.time,
            Key::Speaker => self.loud = !self.loud,
            Key::Beep if long => self.beep = !self.beep,
            Key::Light => self.light_until = self.time + 30.0,
            _ if self.locked => {}
            Key::A | Key::B if long && self.time - self.lock_pressed < 5.0 => {
                self.quick[usize::from(key == Key::B)] = self.channel;
            }
            Key::A => self.channel = self.quick[0],
            Key::B => self.channel = self.quick[1],
            Key::Ch16 => self.channel = 16,
            Key::Pwr | Key::Ch | Key::Sq | Key::Vol => {
                self.target = match key {
                    Key::Pwr => Target::Power,
                    Key::Ch => Target::Channel,
                    Key::Sq => Target::Squelch,
                    _ => Target::Volume,
                };
                self.target_until = self.time + 2.5;
            }
            Key::Up | Key::Down => {
                let up = key == Key::Up;
                match self.target() {
                    Target::Volume => self.volume = step(self.volume, up, 8),
                    Target::Squelch => self.squelch = step(self.squelch, up, 8),
                    Target::Power => self.high_power = up,
                    Target::Channel => {
                        let i = CHANNELS.iter().position(|&c| c == self.channel).unwrap_or(7);
                        let n = CHANNELS.len();
                        self.channel = CHANNELS[if up { (i + 1) % n } else { (i + n - 1) % n }];
                    }
                }
                if self.target() != Target::Volume {
                    self.target_until = self.time + 2.5;
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, now: f64) {
        let dt = (now - self.time).max(0.0);
        self.time = now;
        if self.on {
            // 8 h at 1:9 duty: transmit costs about 4.6 x receive.
            let rate = if self.transmitting() { 4.6 } else { 0.6 };
            self.battery = (self.battery - dt * rate / (8.0 * 3600.0)).max(0.0);
            if self.battery == 0.0 {
                self.on = false;
            }
        }
    }
}

fn step(v: u8, up: bool, max: u8) -> u8 {
    if up { (v + 1).min(max) } else { v.saturating_sub(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_quick_channels() {
        let mut p = Portable::default();
        p.press(Key::OnOff, false);
        assert!(!p.on);
        p.press(Key::OnOff, true);
        p.press(Key::Up, false);
        assert_eq!(p.volume, 5);
        p.press(Key::Ch, false);
        p.press(Key::Up, false);
        assert_eq!(p.channel, 17);
        p.press(Key::Lock, false);
        p.press(Key::B, true);
        p.press(Key::Ch16, false);
        p.press(Key::B, false);
        assert_eq!(p.channel, 17);
        p.press(Key::Lock, true);
        p.press(Key::A, false);
        assert_eq!(p.channel, 17); // keyboard locked
        p.tick(3.0);
        assert_eq!(p.target(), Target::Volume);
    }
}

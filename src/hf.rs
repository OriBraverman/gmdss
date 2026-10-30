//! MF/HF SSB transceiver (the "Compact HF SSB RE2100" receiver/exciter).
//!
//! Frequencies are entered in kHz: `RX` digits [`.` digit] `ENT` sets the receive frequency,
//! `TX` ... `ENT` the transmit frequency (TX selection falls back to RX mode after 10 s).
//! `CH` digits `ENT` recalls a stored channel (0-99) or an ITU duplex channel (401, 1203 ...);
//! `CH` digits `ADD` / `DEL` stores / deletes the current RX/TX pair. `2182` selects the
//! distress frequency (H3E). `MODE` cycles J3E, R3E, H3E, LSB. The dual-function keys act on
//! their lower label after `TX` (POWER, DUMMY LOAD) or after `CH` digits (ADD, DEL).
//! `SEND ALARM` together with `TEST ALARM` (here: within 3 s) transmits the two-tone alarm.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    J3E,
    R3E,
    H3E,
    Lsb,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::J3E => "J3E",
            Mode::R3E => "R3E",
            Mode::H3E => "H3E",
            Mode::Lsb => "LSB",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit(u8),
    Dot,
    FreqDown,
    FreqUp,
    TuneClarif,
    Rx,
    Ch,
    Tx,
    AgcPower,
    DimDummy,
    SendAlarm,
    Ch2182,
    TestAlarm,
    Scan,
    SqAdd,
    ModeDel,
    StopEnt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    None,
    Rx(String),
    Tx(String),
    Ch(String),
}

/// ITU duplex radiotelephone channels (Appendix 17): (band prefix, ship TX of channel 1,
/// coast TX of channel 1, number of channels), 3 kHz spacing.
const ITU_BANDS: [(u32, f64, f64, u32); 8] = [
    (4, 4065.0, 4357.0, 28),
    (6, 6200.0, 6501.0, 8),
    (8, 8195.0, 8719.0, 37),
    (12, 12230.0, 13077.0, 41),
    (16, 16360.0, 17242.0, 56),
    (18, 18780.0, 19755.0, 15),
    (22, 22000.0, 22696.0, 53),
    (25, 25070.0, 26145.0, 10),
];

/// (ship receive, ship transmit) kHz of ITU channel `n` (e.g. 401, 1203).
pub fn itu_channel(n: u32) -> Option<(f64, f64)> {
    let (band, ch) = (n / 100, n % 100);
    let &(_, ship, coast, count) = ITU_BANDS.iter().find(|b| b.0 == band)?;
    (ch >= 1 && ch <= count).then(|| (coast + 3.0 * f64::from(ch - 1), ship + 3.0 * f64::from(ch - 1)))
}

pub const DISTRESS_KHZ: f64 = 2182.0;
const DUMMY_LOAD_KHZ: f64 = 2206.4;
const TX_SELECT_SECS: f64 = 10.0;

#[derive(Debug, Clone)]
pub struct HfRadio {
    pub on: bool,
    pub volume: u8,
    pub rf_gain: u8,
    pub rx_khz: f64,
    pub tx_khz: f64,
    pub mode: Mode,
    /// Transmitter power step 1-3.
    pub power: u8,
    pub dummy_load: bool,
    pub agc: bool,
    pub squelch: bool,
    pub clarifier: bool,
    pub dim: u8,
    pub ptt: bool,
    pub scanning: bool,
    pub channels: [Option<(f64, f64)>; 100],
    entry: Entry,
    tx_selected_until: f64,
    tuning_until: f64,
    send_alarm_pressed: f64,
    /// Two-tone alarm being transmitted until this time.
    pub alarm_tx_until: f64,
    /// Local alarm test sounding until this time.
    pub alarm_test_until: f64,
    scan_pos: usize,
    time: f64,
}

impl Default for HfRadio {
    fn default() -> Self {
        HfRadio {
            on: false,
            volume: 5,
            rf_gain: 10,
            rx_khz: DISTRESS_KHZ,
            tx_khz: DISTRESS_KHZ,
            mode: Mode::H3E,
            power: 3,
            dummy_load: false,
            agc: true,
            squelch: false,
            clarifier: false,
            dim: 3,
            ptt: false,
            scanning: false,
            channels: [None; 100],
            entry: Entry::None,
            tx_selected_until: f64::NEG_INFINITY,
            tuning_until: f64::NEG_INFINITY,
            send_alarm_pressed: f64::NEG_INFINITY,
            alarm_tx_until: f64::NEG_INFINITY,
            alarm_test_until: f64::NEG_INFINITY,
            scan_pos: 0,
            time: 0.0,
        }
    }
}

impl HfRadio {
    pub fn tx_selected(&self) -> bool {
        self.time < self.tx_selected_until
    }

    pub fn tuning(&self) -> bool {
        self.on && self.time < self.tuning_until
    }

    pub fn transmitting(&self) -> bool {
        self.on && (self.ptt || self.time < self.alarm_tx_until)
    }

    pub fn alarm_sound(&self) -> bool {
        self.on && (self.time < self.alarm_test_until || self.time < self.alarm_tx_until)
    }

    /// Sets both frequencies and the mode (used by the DSC modem after a distress acknowledgement).
    pub fn set(&mut self, rx: f64, tx: f64, mode: Mode) {
        self.rx_khz = rx;
        self.tx_khz = tx;
        self.mode = mode;
        self.scanning = false;
    }

    pub fn press(&mut self, key: Key) {
        if !self.on {
            return;
        }
        let entry = std::mem::replace(&mut self.entry, Entry::None);
        match (entry, key) {
            (Entry::Rx(mut s) | Entry::Tx(mut s) | Entry::Ch(mut s), Key::Digit(_) | Key::Dot) if s.len() >= 8 => {
                s.clear();
            }
            (Entry::Rx(mut s), Key::Digit(d)) => self.entry = Entry::Rx(push(&mut s, d)),
            (Entry::Tx(mut s), Key::Digit(d)) => self.entry = Entry::Tx(push(&mut s, d)),
            (Entry::Ch(mut s), Key::Digit(d)) => self.entry = Entry::Ch(push(&mut s, d)),
            (Entry::Rx(s), Key::Dot) if !s.contains('.') => self.entry = Entry::Rx(s + "."),
            (Entry::Tx(s), Key::Dot) if !s.contains('.') => self.entry = Entry::Tx(s + "."),
            (Entry::Rx(s), Key::StopEnt) => {
                if let Some(f) = parse_khz(&s) {
                    self.rx_khz = f;
                }
            }
            (Entry::Tx(s), Key::StopEnt) => {
                if let Some(f) = parse_khz(&s) {
                    self.tx_khz = f;
                    self.dummy_load = false;
                }
                self.tx_selected_until = f64::NEG_INFINITY;
            }
            (Entry::Ch(s), Key::StopEnt) => {
                if let Ok(n) = s.parse::<u32>() {
                    let pair = if n < 100 { self.channels[n as usize] } else { itu_channel(n) };
                    if let Some((rx, tx)) = pair {
                        self.set(rx, tx, self.mode);
                    }
                }
            }
            // RX / TX after channel digits convert the channel to that frequency only.
            (Entry::Ch(s), Key::Rx | Key::Tx) => {
                if let Some((rx, tx)) = s.parse().ok().and_then(itu_channel) {
                    if key == Key::Rx { self.rx_khz = rx } else { self.tx_khz = tx }
                }
            }
            (Entry::Ch(s), Key::SqAdd) => {
                if let Ok(n @ 0..=99) = s.parse::<usize>() {
                    self.channels[n] = Some((self.rx_khz, self.tx_khz));
                }
            }
            (Entry::Ch(s), Key::ModeDel) => {
                if let Ok(n @ 0..=99) = s.parse::<usize>() {
                    self.channels[n] = None;
                }
            }
            (Entry::Tx(_), Key::AgcPower) | (_, Key::AgcPower) if self.tx_selected() => {
                self.power = self.power % 3 + 1;
                self.entry = Entry::Tx(String::new());
            }
            (_, Key::DimDummy) if self.tx_selected() => {
                self.dummy_load = !self.dummy_load;
                if self.dummy_load {
                    self.tx_khz = DUMMY_LOAD_KHZ;
                }
            }
            (_, Key::Rx) => self.entry = Entry::Rx(String::new()),
            (_, Key::Tx) => {
                self.entry = Entry::Tx(String::new());
                self.tx_selected_until = self.time + TX_SELECT_SECS;
            }
            (_, Key::Ch) => self.entry = Entry::Ch(String::new()),
            (_, Key::AgcPower) => self.agc = !self.agc,
            (_, Key::DimDummy) => self.dim = (self.dim + 3) % 4,
            (_, Key::SqAdd) => self.squelch = !self.squelch,
            (_, Key::ModeDel) => {
                self.mode = match self.mode {
                    Mode::J3E => Mode::R3E,
                    Mode::R3E => Mode::H3E,
                    Mode::H3E => Mode::Lsb,
                    Mode::Lsb => Mode::J3E,
                }
            }
            (_, Key::Ch2182) => self.set(DISTRESS_KHZ, DISTRESS_KHZ, Mode::H3E),
            (_, Key::FreqUp | Key::FreqDown) => {
                let step = if self.clarifier { 0.01 } else { 0.1 };
                let step = if key == Key::FreqUp { step } else { -step };
                self.rx_khz = ((self.rx_khz + step) * 100.0).round() / 100.0;
            }
            (_, Key::TuneClarif) => {
                if self.tx_selected() {
                    self.tuning_until = self.time + 2.0;
                } else {
                    self.clarifier = !self.clarifier;
                }
            }
            (_, Key::SendAlarm) => self.send_alarm_pressed = self.time,
            (_, Key::TestAlarm) => {
                if self.time - self.send_alarm_pressed <= 3.0 {
                    // Two-tone alarm on 2182 kHz for 45 s (30-60 s required).
                    self.set(DISTRESS_KHZ, DISTRESS_KHZ, Mode::H3E);
                    self.alarm_tx_until = self.time + 45.0;
                } else {
                    self.alarm_test_until = self.time + 3.0;
                }
            }
            (_, Key::Scan) => self.scanning = !self.scanning && self.channels.iter().any(Option::is_some),
            (_, Key::StopEnt) => {
                self.scanning = false;
                self.alarm_tx_until = f64::NEG_INFINITY;
                self.alarm_test_until = f64::NEG_INFINITY;
                self.tuning_until = f64::NEG_INFINITY;
            }
            (_, Key::Digit(_) | Key::Dot) => {}
        }
    }

    pub fn tick(&mut self, now: f64) {
        self.time = now;
        if self.ptt {
            self.scanning = false;
        }
        if self.scanning {
            let stored: Vec<(f64, f64)> = self.channels.iter().flatten().copied().collect();
            if !stored.is_empty() {
                self.scan_pos = (now / 2.0) as usize % stored.len();
                self.rx_khz = stored[self.scan_pos].0;
                self.tx_khz = stored[self.scan_pos].1;
            }
        }
    }

    /// (RX read-out, TX read-out); an entry in progress replaces the frequency it edits.
    pub fn display(&self) -> (String, String) {
        if !self.on || self.dim == 0 {
            return (String::new(), String::new());
        }
        let blink = self.time % 1.0 < 0.5;
        match &self.entry {
            Entry::Rx(s) => (format!("{s}_"), format!("{:.1}", self.tx_khz)),
            Entry::Tx(s) => (format!("{:.1}", self.rx_khz), format!("{s}_")),
            Entry::Ch(s) => (format!("CH {s}_"), format!("{:.1}", self.tx_khz)),
            Entry::None => {
                let tx = if self.tx_selected() && blink { format!("{:.1}", self.tx_khz).replace('.', " ") } else { format!("{:.1}", self.tx_khz) };
                let rx = if self.clarifier { format!("{:.2}", self.rx_khz) } else { format!("{:.1}", self.rx_khz) };
                (rx, tx)
            }
        }
    }
}

fn push(s: &mut String, d: u8) -> String {
    s.push((b'0' + d) as char);
    s.clone()
}

/// Accepts 1.6-27.5 MHz.
fn parse_khz(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|f| (1600.0..=27_500.0).contains(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radio() -> HfRadio {
        let mut r = HfRadio { on: true, ..Default::default() };
        r.tick(0.0);
        r
    }

    fn keys(r: &mut HfRadio, s: &str) {
        for c in s.chars() {
            r.press(match c {
                '0'..='9' => Key::Digit(c as u8 - b'0'),
                '.' => Key::Dot,
                'R' => Key::Rx,
                'T' => Key::Tx,
                'C' => Key::Ch,
                'E' => Key::StopEnt,
                'A' => Key::SqAdd,
                'D' => Key::ModeDel,
                'M' => Key::ModeDel,
                'P' => Key::AgcPower,
                'L' => Key::DimDummy,
                _ => panic!("{c}"),
            });
        }
    }

    #[test]
    fn itu_channels() {
        assert_eq!(itu_channel(401), Some((4357.0, 4065.0)));
        assert_eq!(itu_channel(1203), Some((13083.0, 12236.0)));
        assert_eq!(itu_channel(609), None);
    }

    #[test]
    fn frequency_and_channel_entry() {
        let mut r = radio();
        keys(&mut r, "R8414.5E");
        assert_eq!(r.rx_khz, 8414.5);
        keys(&mut r, "C401E");
        assert_eq!((r.rx_khz, r.tx_khz), (4357.0, 4065.0));
        keys(&mut r, "C12A");
        r.press(Key::Ch2182);
        assert_eq!((r.rx_khz, r.mode), (2182.0, Mode::H3E));
        keys(&mut r, "C12E");
        assert_eq!(r.tx_khz, 4065.0);
        keys(&mut r, "C12M");
        assert!(r.channels[12].is_none());
        keys(&mut r, "R99999E"); // out of range: ignored
        assert_eq!(r.rx_khz, 4357.0);
    }

    #[test]
    fn tx_functions_and_timeout() {
        let mut r = radio();
        r.press(Key::Tx);
        assert!(r.tx_selected());
        r.press(Key::DimDummy);
        assert_eq!((r.dummy_load, r.tx_khz), (true, 2206.4));
        r.tick(11.0);
        assert!(!r.tx_selected());
        r.press(Key::DimDummy); // now DIM
        assert_eq!(r.dim, 2);
    }

    #[test]
    fn two_tone_alarm() {
        let mut r = radio();
        keys(&mut r, "R4000E");
        r.press(Key::SendAlarm);
        r.tick(1.0);
        r.press(Key::TestAlarm);
        assert!(r.transmitting() && r.rx_khz == 2182.0);
        r.press(Key::StopEnt);
        assert!(!r.transmitting());
        r.tick(10.0);
        r.press(Key::TestAlarm); // alone: local test only
        assert!(r.alarm_sound() && !r.transmitting());
    }
}

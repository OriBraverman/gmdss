//! VHF radiotelephone (the "Compact VHF RT2048" unit): channel selection, channel 16, private
//! channels, US mode, reduced power, dual watch, scanning, selcall, display dimming.
//!
//! Keys: digits select a two-digit channel, `16` selects channel 16, `SHIFT` makes the next key
//! do its orange function: 1 SCAN, 2 SCAN ADD, 3 SCAN DELETE, 4 SELCALL test/reset,
//! 5 SCAN TIME (two digits, seconds), 6 D.W., 7 DIM, 8 U.S., 9 1W, 0 P (private channel P0-P9).
//! The two-character read-out shows the channel, `P3`, `--` while scanning, `no` for an empty
//! scan programme, the first digit of an entry followed by `-`, and the selcall `CA`/`C0`.

/// (channel, ship receive kHz, ship transmit kHz, simplex in US mode), from the original's table.
pub const CHANNELS: [(u8, u32, u32, bool); 55] = [
    (1, 160650, 156050, true), (2, 160700, 156100, false), (3, 160750, 156150, false), (4, 160800, 156200, false),
    (5, 160850, 156250, true), (6, 156300, 156300, false), (7, 160950, 156350, true), (8, 156400, 156400, false),
    (9, 156450, 156450, false), (10, 156500, 156500, false), (11, 156550, 156550, false), (12, 156600, 156600, false),
    (13, 156650, 156650, false), (14, 156700, 156700, false), (15, 156750, 156750, false), (16, 156800, 156800, false),
    (17, 156850, 156850, false), (18, 161500, 156900, true), (19, 161550, 156950, true), (20, 161600, 157000, false),
    (21, 161650, 157050, true), (22, 161700, 157100, true), (23, 161750, 157150, true), (24, 161800, 157200, false),
    (25, 161850, 157250, false), (26, 161900, 157300, false), (27, 161950, 157350, false), (28, 162000, 157400, false),
    (60, 160625, 156025, false), (61, 160675, 156075, false), (62, 160725, 156125, false), (63, 160775, 156175, true),
    (64, 160825, 156225, false), (65, 160875, 156275, true), (66, 160925, 156325, true), (67, 156375, 156375, false),
    (68, 156425, 156425, false), (69, 156475, 156475, false), (70, 156525, 156525, false), (71, 156575, 156575, false),
    (72, 156625, 156625, false), (73, 156675, 156675, false), (74, 156725, 156725, false), (77, 156875, 156875, false),
    (78, 161525, 156925, true), (79, 161575, 156975, true), (80, 161625, 157025, true), (81, 161675, 157075, true),
    (82, 161725, 157125, true), (83, 161775, 157175, true), (84, 161825, 157225, false), (85, 161875, 157275, false),
    (86, 161925, 157325, false), (87, 161975, 157375, false), (88, 162025, 157425, true),
];

/// Private channels P0-P9 are simplex on these frequencies (illustrative; the original reads
/// them from the unit's set-up).
const PRIVATE_BASE_KHZ: u32 = 155_500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Intl(u8),
    Private(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit(u8),
    Ch16,
    Shift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    None,
    Channel(u8),
    Private,
    ScanTime(Option<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selcall {
    /// Individual call, "CA", alarm for 10 s.
    Individual,
    /// All ships call, "C0", alarm until reset.
    AllShips,
}

#[derive(Debug, Clone)]
pub struct VhfRadio {
    pub on: bool,
    /// 0-10.
    pub volume: u8,
    /// 0-10; a signal is heard when its level exceeds the squelch.
    pub squelch: u8,
    pub channel: Channel,
    pub us_mode: bool,
    pub low_power: bool,
    pub shift: bool,
    pub dual_watch: bool,
    pub scanning: bool,
    pub scan_list: Vec<Channel>,
    /// Seconds a scanned channel with a signal is listened to (1-99).
    pub scan_time: u8,
    /// Display brightness step 0-3 (0 = keyboard light only).
    pub dim: u8,
    /// Press-to-talk on the handset.
    pub ptt: bool,
    pub selcall: Option<Selcall>,
    entry: Entry,
    /// Temporary read-out and until when (seconds).
    flash: Option<(&'static str, f64)>,
    time: f64,
    scan_pos: usize,
    /// While a scanned channel holds a signal: listen until this time.
    scan_hold: Option<(Channel, f64)>,
    alarm_until: f64,
}

impl Default for VhfRadio {
    fn default() -> Self {
        VhfRadio {
            on: false,
            volume: 5,
            squelch: 3,
            channel: Channel::Intl(16),
            us_mode: false,
            low_power: false,
            shift: false,
            dual_watch: false,
            scanning: false,
            scan_list: vec![],
            scan_time: 5,
            dim: 3,
            ptt: false,
            selcall: None,
            entry: Entry::None,
            flash: None,
            time: 0.0,
            scan_pos: 0,
            scan_hold: None,
            alarm_until: 0.0,
        }
    }
}

/// Reads the channel table.
pub fn channel_info(ch: u8) -> Option<(u32, u32, bool)> {
    CHANNELS.iter().find(|c| c.0 == ch).map(|&(_, rx, tx, us)| (rx, tx, us))
}

impl VhfRadio {
    /// (receive kHz, transmit kHz) of a channel in the current mode.
    pub fn frequencies(&self, ch: Channel) -> (u32, u32) {
        match ch {
            Channel::Private(p) => {
                let f = PRIVATE_BASE_KHZ + 25 * u32::from(p);
                (f, f)
            }
            Channel::Intl(n) => {
                let (rx, tx, us) = channel_info(n).unwrap_or((156_800, 156_800, false));
                if self.us_mode && us { (tx, tx) } else { (rx, tx) }
            }
        }
    }

    /// The channel the receiver is on right now (dual watch / scanning switch it).
    pub fn listening(&self) -> Channel {
        if let Some((ch, _)) = self.scan_hold {
            return ch;
        }
        if self.scanning && !self.scan_list.is_empty() {
            return self.scan_list[self.scan_pos % self.scan_list.len()];
        }
        if self.dual_watch && self.dual_watch_on_16() {
            return Channel::Intl(16);
        }
        self.channel
    }

    /// Transmitting is possible except on the DSC channel 70.
    pub fn transmitting(&self) -> bool {
        self.on && self.ptt && self.channel != Channel::Intl(70)
    }

    pub fn alarm(&self) -> bool {
        self.on && (self.time < self.alarm_until || self.selcall == Some(Selcall::AllShips))
    }

    /// Dual watch: channel 16 is checked for 0.1 s every 1.2 s.
    fn dual_watch_on_16(&self) -> bool {
        self.time % 1.2 >= 1.1
    }

    pub fn power(&mut self, on: bool) {
        self.on = on;
        self.entry = Entry::None;
        self.shift = false;
        self.ptt = false;
    }

    pub fn press(&mut self, key: Key) {
        if !self.on {
            return;
        }
        if key == Key::Shift {
            self.shift = !self.shift;
            return;
        }
        let shifted = std::mem::replace(&mut self.shift, false);
        match (self.entry, key) {
            (Entry::Channel(first), Key::Digit(d)) => {
                self.entry = Entry::None;
                let n = first * 10 + d;
                if channel_info(n).is_some() {
                    self.select(Channel::Intl(n));
                } else {
                    self.show("--", 1.0);
                }
            }
            (Entry::Private, Key::Digit(d)) => {
                self.entry = Entry::None;
                self.select(Channel::Private(d));
            }
            (Entry::ScanTime(None), Key::Digit(d)) => self.entry = Entry::ScanTime(Some(d)),
            (Entry::ScanTime(Some(first)), Key::Digit(d)) => {
                self.entry = Entry::None;
                self.scan_time = (first * 10 + d).max(1);
            }
            (_, Key::Ch16) => {
                self.entry = Entry::None;
                self.select(Channel::Intl(16));
            }
            (_, Key::Digit(d)) if shifted => self.function(d),
            (_, Key::Digit(d)) => self.entry = Entry::Channel(d),
            (_, Key::Shift) => unreachable!(),
        }
    }

    /// Channel change from outside (the DSC unit's serial interface).
    pub fn set_channel(&mut self, ch: Channel) {
        self.select(ch);
    }

    fn select(&mut self, ch: Channel) {
        self.channel = ch;
        self.scanning = false;
        self.scan_hold = None;
    }

    fn show(&mut self, text: &'static str, secs: f64) {
        self.flash = Some((text, self.time + secs));
    }

    fn function(&mut self, d: u8) {
        match d {
            1 => {
                if self.scan_list.is_empty() {
                    self.show("no", 2.5);
                } else {
                    self.scanning = !self.scanning;
                    self.scan_hold = None;
                    self.dual_watch = false;
                }
            }
            2 => {
                if !self.scan_list.contains(&self.channel) && self.channel != Channel::Intl(16) {
                    self.scan_list.push(self.channel);
                }
            }
            3 => self.scan_list.retain(|c| *c != self.channel),
            4 => match self.selcall {
                Some(_) => {
                    self.selcall = None;
                    self.alarm_until = 0.0;
                }
                None => {
                    // Test: read-out alternates "CA" with the channel, alarm sounds.
                    self.receive_selcall(Selcall::Individual);
                }
            },
            5 => self.entry = Entry::ScanTime(None),
            6 => {
                self.dual_watch = !self.dual_watch;
                self.scanning = false;
            }
            7 => self.dim = (self.dim + 3) % 4,
            8 => self.us_mode = !self.us_mode,
            9 => self.low_power = !self.low_power,
            0 => self.entry = Entry::Private,
            _ => {}
        }
    }

    /// A selective call addressed to this ship.
    pub fn receive_selcall(&mut self, kind: Selcall) {
        self.selcall = Some(kind);
        if kind == Selcall::Individual {
            self.alarm_until = self.time + 10.0;
        }
    }

    /// Advances time; `signal(rx_khz)` tells whether a signal above the squelch is on a frequency.
    pub fn tick(&mut self, now: f64, signal: &dyn Fn(u32) -> bool) {
        self.time = now;
        if self.flash.is_some_and(|(_, until)| now >= until) {
            self.flash = None;
        }
        if self.ptt {
            // Keying the transmitter stops dual watch and scanning.
            self.dual_watch = false;
            self.scanning = false;
            self.scan_hold = None;
        }
        if !self.scanning || self.scan_list.is_empty() {
            return;
        }
        if let Some((ch, until)) = self.scan_hold {
            let busy = signal(self.frequencies(ch).0);
            if busy && now < until {
                return;
            }
            self.scan_hold = None;
        }
        // About 10 channels a second.
        self.scan_pos = (now * 10.0) as usize % self.scan_list.len();
        let ch = self.scan_list[self.scan_pos];
        if signal(self.frequencies(ch).0) {
            self.scan_hold = Some((ch, now + f64::from(self.scan_time)));
        }
    }

    /// The two-character LED read-out.
    pub fn display(&self) -> String {
        if !self.on {
            return String::new();
        }
        if let Some((t, _)) = self.flash {
            return t.into();
        }
        match self.entry {
            Entry::Channel(d) => return format!("{d}-"),
            Entry::Private => return "P ".into(),
            Entry::ScanTime(None) => return "--".into(),
            Entry::ScanTime(Some(d)) => return format!("{d}-"),
            Entry::None => {}
        }
        if let Some(s) = self.selcall {
            if self.time % 1.0 < 0.5 {
                return if s == Selcall::Individual { "CA" } else { "C0" }.into();
            }
        }
        let ch = if self.scanning && self.scan_hold.is_none() {
            return "--".into();
        } else {
            self.listening()
        };
        match ch {
            Channel::Intl(n) => format!("{n:2}"),
            Channel::Private(p) => format!("P{p}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radio() -> VhfRadio {
        let mut r = VhfRadio::default();
        r.power(true);
        r
    }

    fn keys(r: &mut VhfRadio, ks: &[Key]) {
        for k in ks {
            r.press(*k);
        }
    }

    #[test]
    fn channel_entry() {
        let mut r = radio();
        keys(&mut r, &[Key::Digit(2), Key::Digit(3)]);
        assert_eq!((r.channel, r.display()), (Channel::Intl(23), "23".into()));
        r.press(Key::Digit(7));
        assert_eq!(r.display(), "7-");
        r.press(Key::Digit(6)); // no channel 76
        assert_eq!((r.channel, r.display()), (Channel::Intl(23), "--".into()));
        r.tick(2.0, &|_| false);
        r.press(Key::Ch16);
        assert_eq!(r.display(), "16");
    }

    #[test]
    fn shifted_functions() {
        let mut r = radio();
        keys(&mut r, &[Key::Shift, Key::Digit(0), Key::Digit(3)]);
        assert_eq!(r.display(), "P3");
        keys(&mut r, &[Key::Shift, Key::Digit(9), Key::Shift, Key::Digit(8)]);
        assert!(r.low_power && r.us_mode);
        keys(&mut r, &[Key::Digit(2), Key::Digit(2)]);
        assert_eq!(r.frequencies(r.channel), (157_100, 157_100)); // 22A simplex in US mode
        keys(&mut r, &[Key::Shift, Key::Digit(5), Key::Digit(1), Key::Digit(2)]);
        assert_eq!(r.scan_time, 12);
    }

    #[test]
    fn scanning_stops_on_signal() {
        let mut r = radio();
        keys(&mut r, &[Key::Shift, Key::Digit(1)]);
        assert_eq!(r.display(), "no"); // empty scan programme
        for ch in [[0, 6], [1, 1], [7, 0]] {
            keys(&mut r, &[Key::Digit(ch[0]), Key::Digit(ch[1]), Key::Shift, Key::Digit(2)]);
        }
        r.tick(3.0, &|_| false);
        keys(&mut r, &[Key::Shift, Key::Digit(1)]);
        assert!(r.scanning);
        r.tick(3.1, &|_| false);
        assert_eq!(r.display(), "--");
        let ch11 = |f: u32| f == 156_550;
        for i in 0..10 {
            r.tick(4.0 + f64::from(i) * 0.1, &ch11);
        }
        assert_eq!(r.display(), "11");
        r.tick(20.0, &|_| false); // signal gone and scan time over: scanning resumes
        r.tick(20.05, &|_| false);
        assert_eq!(r.display(), "--");
    }

    #[test]
    fn dual_watch_and_ptt() {
        let mut r = radio();
        keys(&mut r, &[Key::Digit(0), Key::Digit(6), Key::Shift, Key::Digit(6)]);
        r.tick(1.15, &|_| false);
        assert_eq!(r.display(), "16");
        r.tick(1.3, &|_| false);
        assert_eq!(r.display(), " 6");
        r.ptt = true;
        r.tick(1.4, &|_| false);
        assert!(!r.dual_watch && r.transmitting());
        keys(&mut r, &[Key::Digit(7), Key::Digit(0)]);
        assert!(!r.transmitting()); // no voice on the DSC channel
    }
}

//! Digital Selective Calling: the call format and the VHF DSC controller (the "Compact" DSC unit
//! with a 2 x 24 character display).
//!
//! Keys: `CALL` composes an individual call (MMSI, priority, telecommand, work channel), `DISTRESS`
//! composes a distress alert (nature, position), arrows change the flashing choice, digits enter
//! numbers, `NEXT` accepts a step, `LAST` goes back, `SEND` transmits ("Press SEND for call"); a
//! distress alert needs `DISTRESS` and `SEND` together (here: SEND within 3 s of DISTRESS).
//! `RX MSG` shows received calls, `STOP` silences the alarm (and, pressed again while waiting,
//! cancels a distress alert). Unacknowledged distress alerts repeat every 3.5-4.5 minutes.
//! Display texts are the original's.

/// Which DSC system the controller belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// Channel 70.
    Vhf,
    /// MF/HF DSC frequencies.
    MfHf,
}

/// MF/HF DSC distress frequencies (kHz).
pub const DISTRESS_DSC_KHZ: [f64; 6] = [2187.5, 4207.5, 6312.0, 8414.5, 12577.0, 16804.5];

/// The radiotelephony distress frequency paired with a DSC distress frequency.
pub fn rt_frequency(dsc_khz: f64) -> f64 {
    let pairs = [(2187.5, 2182.0), (4207.5, 4125.0), (6312.0, 6215.0), (8414.5, 8291.0), (12577.0, 12290.0), (16804.5, 16420.0)];
    pairs.iter().find(|p| p.0 == dsc_khz).map_or(2182.0, |p| p.1)
}

/// Display width.
pub const COLS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Distress,
    AllShips,
    Group,
    Area,
    Individual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Distress,
    Urgency,
    Safety,
    ShipsBusiness,
    Routine,
}

impl Category {
    pub const SELECTABLE: [Category; 4] = [Category::Routine, Category::Safety, Category::Urgency, Category::ShipsBusiness];

    pub fn name(self) -> &'static str {
        match self {
            Category::Distress => "Distress",
            Category::Urgency => "Urgency",
            Category::Safety => "Safety",
            Category::ShipsBusiness => "Ships business",
            Category::Routine => "Routine",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nature {
    Fire,
    Flooding,
    Collision,
    Grounding,
    Listing,
    Sinking,
    Disabled,
    Abandoning,
    Epirb,
    Undesignated,
}

impl Nature {
    pub const ALL: [Nature; 10] = [
        Nature::Undesignated, Nature::Fire, Nature::Flooding, Nature::Collision, Nature::Grounding,
        Nature::Listing, Nature::Sinking, Nature::Disabled, Nature::Abandoning, Nature::Epirb,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Nature::Fire => "Fire, explosion",
            Nature::Flooding => "Flooding",
            Nature::Collision => "Collision",
            Nature::Grounding => "Grounding",
            Nature::Listing => "Listing, capsizing",
            Nature::Sinking => "Sinking",
            Nature::Disabled => "Disabled & adrift",
            Nature::Abandoning => "Abandoning ship",
            Nature::Epirb => "EPIRB emission",
            Nature::Undesignated => "Undesignated distress",
        }
    }
}

/// First telecommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Telecommand {
    F3ESimplex,
    F3EDuplex,
    DistressAck,
    DistressRelay,
    Test,
    Unable,
    NoInfo,
}

impl Telecommand {
    pub const SELECTABLE: [Telecommand; 3] = [Telecommand::F3ESimplex, Telecommand::F3EDuplex, Telecommand::Test];

    pub fn name(self) -> &'static str {
        match self {
            Telecommand::F3ESimplex => "F3E simplex",
            Telecommand::F3EDuplex => "F3E duplex",
            Telecommand::DistressAck => "Dist. acknow",
            Telecommand::DistressRelay => "Dist. relay",
            Telecommand::Test => "Dist. test",
            Telecommand::Unable => "Can't comply",
            Telecommand::NoInfo => "No info",
        }
    }
}

/// Position in whole minutes of arc (north and east positive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub lat_min: i32,
    pub lon_min: i32,
}

impl Position {
    /// "N:52°30' E:004°15'" style, as on the display.
    pub fn text(self) -> String {
        let (ns, la) = if self.lat_min >= 0 { ('N', self.lat_min) } else { ('S', -self.lat_min) };
        let (ew, lo) = if self.lon_min >= 0 { ('E', self.lon_min) } else { ('W', -self.lon_min) };
        format!("{ns}:{:02}\u{b0}{:02}' {ew}:{:03}\u{b0}{:02}'", la / 60, la % 60, lo / 60, lo % 60)
    }
}

/// End of sequence: does the call ask for an acknowledgement, or is it one?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eos {
    AckRequested,
    Ack,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub format: Format,
    pub category: Category,
    pub from: String,
    /// Called station for individual calls.
    pub to: Option<String>,
    pub tc1: Telecommand,
    pub channel: Option<u8>,
    /// MF/HF working or distress frequency (kHz).
    pub freq_khz: Option<f64>,
    pub nature: Option<Nature>,
    pub position: Option<Position>,
    /// UTC (hour, minute) the position was valid.
    pub utc: Option<(u8, u8)>,
    /// Ship in distress, for acknowledgements and relays.
    pub distress_ship: Option<String>,
    pub eos: Eos,
}

impl Call {
    pub fn distress(from: &str, nature: Nature, position: Option<Position>, utc: Option<(u8, u8)>) -> Call {
        Call {
            format: Format::Distress,
            category: Category::Distress,
            from: from.into(),
            to: None,
            tc1: Telecommand::F3ESimplex,
            channel: None,
            freq_khz: None,
            nature: Some(nature),
            position,
            utc,
            distress_ship: None,
            eos: Eos::Other,
        }
    }

    /// A coast station's acknowledgement of `alert`.
    pub fn distress_ack(from: &str, alert: &Call) -> Call {
        Call {
            format: Format::AllShips,
            category: Category::Distress,
            from: from.into(),
            to: None,
            tc1: Telecommand::DistressAck,
            channel: None,
            freq_khz: alert.freq_khz,
            nature: alert.nature,
            position: alert.position,
            utc: alert.utc,
            distress_ship: Some(alert.from.clone()),
            eos: Eos::Other,
        }
    }

    /// The acknowledgement of an individual call (same telecommand and channel).
    pub fn ack(from: &str, call: &Call) -> Call {
        Call { from: from.into(), to: Some(call.from.clone()), eos: Eos::Ack, ..call.clone() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit(u8),
    Dot,
    Func,
    RxMsg,
    Edit,
    Last,
    Next,
    Up,
    Down,
    Left,
    Right,
    Distress,
    Call,
    Send,
    StopEnt,
}

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Idle,
    CallMid(String),
    CallCategory(usize),
    CallTc(usize),
    CallChannel(String),
    CallSend,
    WaitAck { until: f64 },
    Message([String; 2], f64),
    DistNature(usize),
    DistPos,
    DistFreq(usize),
    DistSend,
    DistWait,
    Received(usize),
    RxLog(usize),
    SendAck(usize),
}

/// A received call with its time.
#[derive(Debug, Clone, PartialEq)]
pub struct Logged {
    pub call: Call,
    pub utc: (u8, u8),
}

pub struct Controller {
    pub mmsi: String,
    pub band: Band,
    pub position: Option<Position>,
    pub log: Vec<Logged>,
    pub alarm: bool,
    screen: Screen,
    composing: Call,
    distress_pressed: f64,
    next_distress: Option<f64>,
    distress_call: Option<Call>,
    time: f64,
    utc: (u8, u8, u8),
    random: u32,
}

/// How long an individual call waits for its acknowledgement.
const ACK_WAIT: f64 = 300.0;

impl Controller {
    pub fn new(mmsi: &str, band: Band) -> Controller {
        Controller {
            mmsi: mmsi.into(),
            band,
            position: None,
            log: vec![],
            alarm: false,
            screen: Screen::Idle,
            composing: Call::distress(mmsi, Nature::Undesignated, None, None),
            distress_pressed: f64::NEG_INFINITY,
            next_distress: None,
            distress_call: None,
            time: 0.0,
            utc: (0, 0, 0),
            random: 0x1234_5678,
        }
    }

    /// A distress alert is active (sent, not yet acknowledged or cancelled).
    pub fn distress_active(&self) -> bool {
        self.next_distress.is_some()
    }

    fn rand01(&mut self) -> f64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        f64::from(self.random % 1000) / 1000.0
    }

    fn message(&mut self, a: &str, b: &str, secs: f64) {
        self.screen = Screen::Message([a.into(), b.into()], self.time + secs);
    }

    /// Advances time; returns the calls to transmit now.
    pub fn tick(&mut self, now: f64, utc: (u8, u8, u8)) -> Vec<Call> {
        self.time = now;
        self.utc = utc;
        let mut out = vec![];
        if let Screen::Message(_, until) = self.screen {
            if now >= until {
                self.screen = if self.distress_active() { Screen::DistWait } else { Screen::Idle };
            }
        }
        if let Screen::WaitAck { until } = self.screen {
            if now >= until {
                self.message("Call incomplete. No ack.", "from called station.", 5.0);
            }
        }
        if let (Some(at), Some(call)) = (self.next_distress, &self.distress_call) {
            if now >= at {
                out.push(call.clone());
                let wait = 210.0 + self.rand01() * 60.0;
                self.next_distress = Some(now + wait);
            }
        }
        out
    }

    fn new_distress(&self) -> Call {
        let mut c = Call::distress(&self.mmsi, Nature::Undesignated, self.position, Some((self.utc.0, self.utc.1)));
        if self.band == Band::MfHf {
            c.freq_khz = Some(DISTRESS_DSC_KHZ[0]);
        }
        c
    }

    /// The nature of distress being composed or selected, if a distress alert is being set up.
    pub fn composing_nature(&self) -> Option<Nature> {
        match self.screen {
            Screen::DistNature(i) => Some(Nature::ALL[i]),
            Screen::DistPos | Screen::DistFreq(_) | Screen::DistSend => self.composing.nature,
            _ => None,
        }
    }

    /// The transmitter could not send the last call (the VHF is switched off).
    pub fn tx_failed(&mut self) {
        self.message("VHF Tx power on error.", "Call aborted !!!", 4.0);
    }

    /// A call heard on channel 70.
    pub fn receive(&mut self, call: Call) {
        let for_us = match call.format {
            Format::Individual => call.to.as_deref() == Some(self.mmsi.as_str()),
            _ => true,
        };
        if !for_us || call.from == self.mmsi {
            return;
        }
        if call.tc1 == Telecommand::DistressAck && call.distress_ship.as_deref() == Some(self.mmsi.as_str()) {
            self.next_distress = None;
            self.distress_call = None;
        }
        let ack_for_our_call = call.eos == Eos::Ack && matches!(self.screen, Screen::WaitAck { .. });
        self.log.push(Logged { call: call.clone(), utc: (self.utc.0, self.utc.1) });
        let i = self.log.len() - 1;
        self.alarm = call.category == Category::Distress || call.category == Category::Urgency || call.eos == Eos::AckRequested || ack_for_our_call;
        self.screen = if call.eos == Eos::AckRequested { Screen::SendAck(i) } else { Screen::Received(i) };
    }

    pub fn press(&mut self, key: Key) -> Vec<Call> {
        let now = self.time;
        match key {
            Key::StopEnt if self.alarm => {
                self.alarm = false;
                return vec![];
            }
            Key::StopEnt if self.screen == Screen::DistWait => {
                self.next_distress = None;
                self.distress_call = None;
                self.screen = Screen::Idle;
                return vec![];
            }
            Key::Distress => {
                self.distress_pressed = now;
                if !matches!(self.screen, Screen::DistNature(_) | Screen::DistPos | Screen::DistFreq(_) | Screen::DistSend) && !self.distress_active() {
                    self.composing = self.new_distress();
                    self.screen = Screen::DistNature(0);
                }
                return vec![];
            }
            Key::Send if now - self.distress_pressed <= 3.0 && !self.distress_active() => {
                // DISTRESS + SEND: alert with whatever nature/position is composed (undesignated by default).
                if !matches!(self.screen, Screen::DistNature(_) | Screen::DistPos | Screen::DistFreq(_) | Screen::DistSend) {
                    self.composing = self.new_distress();
                }
                let call = self.composing.clone();
                self.distress_call = Some(call.clone());
                self.next_distress = Some(now + 210.0 + self.rand01() * 60.0);
                self.message("Distress call in", "progress !!!", 2.0);
                return vec![call];
            }
            Key::Call => {
                self.composing = Call {
                    format: Format::Individual,
                    category: Category::Routine,
                    from: self.mmsi.clone(),
                    to: None,
                    tc1: Telecommand::F3ESimplex,
                    channel: None,
                    freq_khz: None,
                    nature: None,
                    position: None,
                    utc: None,
                    distress_ship: None,
                    eos: Eos::AckRequested,
                };
                self.screen = Screen::CallMid(String::new());
                return vec![];
            }
            Key::RxMsg => {
                self.screen = Screen::RxLog(self.log.len().saturating_sub(1));
                return vec![];
            }
            _ => {}
        }
        let screen = self.screen.clone();
        match (screen, key) {
            (Screen::CallMid(mut s), Key::Digit(d)) if s.len() < 9 => {
                s.push((b'0' + d) as char);
                self.screen = Screen::CallMid(s);
            }
            (Screen::CallMid(mut s), Key::Left) => {
                s.pop();
                self.screen = Screen::CallMid(s);
            }
            (Screen::CallMid(s), Key::Next) if s.len() == 9 => {
                self.composing.to = Some(s);
                self.screen = Screen::CallCategory(0);
            }
            (Screen::CallCategory(i), Key::Up | Key::Right) => self.screen = Screen::CallCategory((i + 1) % 4),
            (Screen::CallCategory(i), Key::Down | Key::Left) => self.screen = Screen::CallCategory((i + 3) % 4),
            (Screen::CallCategory(i), Key::Next) => {
                self.composing.category = Category::SELECTABLE[i];
                self.screen = Screen::CallTc(0);
            }
            (Screen::CallTc(i), Key::Up | Key::Right) => self.screen = Screen::CallTc((i + 1) % 3),
            (Screen::CallTc(i), Key::Down | Key::Left) => self.screen = Screen::CallTc((i + 2) % 3),
            (Screen::CallTc(i), Key::Next) => {
                self.composing.tc1 = Telecommand::SELECTABLE[i];
                self.screen = if self.composing.tc1 == Telecommand::Test { Screen::CallSend } else { Screen::CallChannel(String::new()) };
            }
            (Screen::CallChannel(mut s), Key::Digit(d)) if s.len() < if self.band == Band::Vhf { 2 } else { 7 } => {
                s.push((b'0' + d) as char);
                self.screen = Screen::CallChannel(s);
            }
            (Screen::CallChannel(s), Key::Dot) if self.band == Band::MfHf && !s.contains('.') => self.screen = Screen::CallChannel(s + "."),
            (Screen::CallChannel(s), Key::Next) if self.band == Band::MfHf => {
                match s.parse::<f64>().ok().filter(|f| (1600.0..=27_500.0).contains(f)) {
                    Some(f) => {
                        self.composing.freq_khz = Some(f);
                        self.screen = Screen::CallSend;
                    }
                    None => self.screen = Screen::CallChannel(String::new()),
                }
            }
            (Screen::CallChannel(s), Key::Next) => {
                let ch = s.parse().ok().filter(|c| crate::vhf::channel_info(*c).is_some() && *c != 70);
                if let Some(ch) = ch {
                    self.composing.channel = Some(ch);
                    self.screen = Screen::CallSend;
                } else {
                    self.screen = Screen::CallChannel(String::new());
                }
            }
            (Screen::CallSend, Key::Send) => {
                self.screen = Screen::WaitAck { until: now + ACK_WAIT };
                return vec![self.composing.clone()];
            }
            (Screen::DistNature(i), Key::Up | Key::Right) => self.screen = Screen::DistNature((i + 1) % Nature::ALL.len()),
            (Screen::DistNature(i), Key::Down | Key::Left) => self.screen = Screen::DistNature((i + Nature::ALL.len() - 1) % Nature::ALL.len()),
            (Screen::DistNature(i), Key::Next) => {
                self.composing.nature = Some(Nature::ALL[i]);
                self.screen = Screen::DistPos;
            }
            (Screen::DistPos, Key::Next) => self.screen = if self.band == Band::MfHf { Screen::DistFreq(0) } else { Screen::DistSend },
            (Screen::DistFreq(i), Key::Up | Key::Right) => self.screen = Screen::DistFreq((i + 1) % DISTRESS_DSC_KHZ.len()),
            (Screen::DistFreq(i), Key::Down | Key::Left) => self.screen = Screen::DistFreq((i + DISTRESS_DSC_KHZ.len() - 1) % DISTRESS_DSC_KHZ.len()),
            (Screen::DistFreq(i), Key::Next) => {
                self.composing.freq_khz = Some(DISTRESS_DSC_KHZ[i]);
                self.screen = Screen::DistSend;
            }
            (Screen::RxLog(i), Key::Up) if i + 1 < self.log.len() => self.screen = Screen::RxLog(i + 1),
            (Screen::RxLog(i), Key::Down) if i > 0 => self.screen = Screen::RxLog(i - 1),
            (Screen::SendAck(i), Key::Send) => {
                let ack = Call::ack(&self.mmsi, &self.log[i].call);
                self.message("Acknowledge transmitted.", "", 3.0);
                return vec![ack];
            }
            (Screen::Received(_) | Screen::SendAck(_) | Screen::RxLog(_), Key::Last | Key::StopEnt) => self.screen = Screen::Idle,
            (_, Key::Last) => self.screen = if self.distress_active() { Screen::DistWait } else { Screen::Idle },
            _ => {}
        }
        vec![]
    }

    /// The two display lines, each padded to 24 characters.
    pub fn display(&self) -> [String; 2] {
        let (a, b) = self.lines();
        [pad(&a), pad(&b)]
    }

    fn lines(&self) -> (String, String) {
        let (h, m, s) = self.utc;
        match &self.screen {
            Screen::Idle => (if self.band == Band::Vhf { "Watching CH 70" } else { "Scanning DSC frequencies" }.into(), format!("Time {h:02}.{m:02}.{s:02}")),
            Screen::CallMid(d) => (format!("Call MID: <{d:<9}>"), "Enter MID, press NEXT".into()),
            Screen::CallCategory(i) => ("Type of priority:".into(), format!("> {}", Category::SELECTABLE[*i].name())),
            Screen::CallTc(i) => ("Message 1 (Telecmd 1):".into(), format!("> {}", Telecommand::SELECTABLE[*i].name())),
            Screen::CallChannel(s) if self.band == Band::MfHf => (format!("Work freq: {s}_ kHz"), "Enter frequency, NEXT".into()),
            Screen::CallChannel(s) => (format!("Work channel: {s:_<2}"), "Enter channel, NEXT".into()),
            Screen::DistFreq(i) => ("Distress frequency:".into(), format!("> {:.1} kHz", DISTRESS_DSC_KHZ[*i])),
            Screen::CallSend => ("Press SEND for call".into(), format!("Call: <{}>", self.composing.to.as_deref().unwrap_or(""))),
            Screen::WaitAck { .. } => ("Waiting for acknowledge.".into(), String::new()),
            Screen::Message([a, b], _) => (a.clone(), b.clone()),
            Screen::DistNature(i) => ("Nature of distress:".into(), format!("> {}", Nature::ALL[*i].name())),
            Screen::DistPos => match self.composing.position {
                Some(p) => (format!("Distress pos at: {:02}.{:02}", self.composing.utc.map_or(0, |u| u.0), self.composing.utc.map_or(0, |u| u.1)), p.text()),
                None => ("No position information".into(), "in call. Press NEXT".into()),
            },
            Screen::DistSend => ("Press DISTRESS and SEND".into(), "for distress call".into()),
            Screen::DistWait => ("Waiting for distress".into(), "acknowledge.".into()),
            Screen::Received(i) | Screen::SendAck(i) => {
                let l = &self.log[*i];
                let c = &l.call;
                let top = match (c.tc1, c.format, c.eos) {
                    (Telecommand::DistressAck, ..) => "Distress acknowledge".to_string(),
                    (Telecommand::DistressRelay, ..) => "Distress relay".into(),
                    (_, Format::Distress, _) => "Distress call received !".into(),
                    (_, _, Eos::Ack) => "Acknowledge received".into(),
                    (_, Format::AllShips, _) => format!("All ships: {}", c.category.name()),
                    _ => format!("Call: {}", c.category.name()),
                };
                let bottom = match &self.screen {
                    Screen::SendAck(_) => "Press SEND for ack.".into(),
                    _ => match (c.nature, c.distress_ship.as_ref()) {
                        (Some(n), Some(ship)) if c.tc1 == Telecommand::DistressAck => format!("{ship} {}", short(n)),
                        (Some(n), _) => format!("<{}> {}", c.from, short(n)),
                        _ => format!("From stn: <{}>", c.from),
                    },
                };
                (top, bottom)
            }
            Screen::RxLog(i) => match self.log.get(*i) {
                None => ("No messages received".into(), String::new()),
                Some(l) => (
                    format!("{:02}:{:02} {}", l.utc.0, l.utc.1, if l.call.category == Category::Distress { "Distress" } else { l.call.category.name() }),
                    format!("From stn: <{}>", l.call.from),
                ),
            },
        }
    }
}

fn short(n: Nature) -> &'static str {
    // The 16-character names used on the receive screens.
    match n {
        Nature::Fire => "Fire",
        Nature::Listing => "Listing",
        Nature::Disabled => "Disabled",
        Nature::Undesignated => "Undesignated",
        other => other.name(),
    }
}

fn pad(s: &str) -> String {
    let mut s: String = s.chars().take(COLS).collect();
    while s.chars().count() < COLS {
        s.push(' ');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "428123000";
    const COAST: &str = "002191000";

    fn ctl() -> Controller {
        let mut c = Controller::new(ME, Band::Vhf);
        c.position = Some(Position { lat_min: 32 * 60 + 50, lon_min: 34 * 60 + 58 });
        c.tick(0.0, (10, 15, 0));
        c
    }

    fn press(c: &mut Controller, keys: &[Key]) -> Vec<Call> {
        keys.iter().flat_map(|k| c.press(*k)).collect()
    }

    #[test]
    fn individual_call_and_ack() {
        let mut c = ctl();
        let mut keys = vec![Key::Call];
        keys.extend(COAST.bytes().map(|b| Key::Digit(b - b'0')));
        keys.extend([Key::Next, Key::Up, Key::Next, Key::Next, Key::Digit(2), Key::Digit(6), Key::Next]);
        assert!(press(&mut c, &keys).is_empty());
        assert_eq!(c.display()[0].trim_end(), "Press SEND for call");
        let sent = press(&mut c, &[Key::Send]);
        assert_eq!(sent.len(), 1);
        let call = &sent[0];
        assert_eq!((call.to.as_deref(), call.category, call.channel), (Some(COAST), Category::Safety, Some(26)));
        assert_eq!(c.display()[0].trim_end(), "Waiting for acknowledge.");
        c.receive(Call::ack(COAST, call));
        assert!(c.alarm);
        assert_eq!(c.display()[0].trim_end(), "Acknowledge received");
    }

    #[test]
    fn unanswered_call_times_out() {
        let mut c = ctl();
        let mut keys = vec![Key::Call];
        keys.extend("123456789".bytes().map(|b| Key::Digit(b - b'0')));
        keys.extend([Key::Next, Key::Next, Key::Next, Key::Digit(0), Key::Digit(6), Key::Next, Key::Send]);
        press(&mut c, &keys);
        c.tick(301.0, (10, 20, 1));
        assert_eq!(c.display()[0].trim_end(), "Call incomplete. No ack.");
    }

    #[test]
    fn distress_alert_repeats_until_acknowledged() {
        let mut c = ctl();
        press(&mut c, &[Key::Distress, Key::Up, Key::Next]);
        assert_eq!(c.display()[1].trim_end(), "N:32\u{b0}50' E:034\u{b0}58'");
        press(&mut c, &[Key::Next]);
        assert_eq!(c.display()[0].trim_end(), "Press DISTRESS and SEND");
        c.tick(10.0, (10, 15, 10));
        assert!(press(&mut c, &[Key::Send]).is_empty()); // SEND alone is not enough
        let sent = press(&mut c, &[Key::Distress, Key::Send]);
        assert_eq!((sent[0].format, sent[0].nature), (Format::Distress, Some(Nature::Fire)));
        c.tick(13.0, (10, 15, 13));
        assert_eq!(c.display()[0].trim_end(), "Waiting for distress");
        assert!(c.tick(200.0, (10, 18, 30)).is_empty());
        assert_eq!(c.tick(300.0, (10, 20, 10)).len(), 1); // repeated after 3.5-4.5 min
        c.receive(Call::distress_ack(COAST, &sent[0]));
        assert!(!c.distress_active() && c.alarm);
        assert_eq!(c.display()[0].trim_end(), "Distress acknowledge");
        assert!(c.tick(1000.0, (10, 32, 0)).is_empty());
    }

    #[test]
    fn quick_distress_and_cancel() {
        let mut c = ctl();
        let sent = press(&mut c, &[Key::Distress, Key::Send]);
        assert_eq!(sent[0].nature, Some(Nature::Undesignated));
        c.tick(5.0, (10, 15, 5));
        press(&mut c, &[Key::StopEnt]);
        assert!(!c.distress_active());
    }

    #[test]
    fn received_distress_and_individual_call() {
        let mut c = ctl();
        c.receive(Call::distress("211000000", Nature::Sinking, None, None));
        assert!(c.alarm);
        assert_eq!(c.display()[0].trim_end(), "Distress call received !");
        press(&mut c, &[Key::StopEnt]);
        assert!(!c.alarm);
        let mut call = Call::distress("002191000", Nature::Fire, None, None);
        call.format = Format::Individual;
        call.category = Category::Routine;
        call.to = Some(ME.into());
        call.eos = Eos::AckRequested;
        call.nature = None;
        call.channel = Some(26);
        c.receive(call);
        assert_eq!(c.display()[1].trim_end(), "Press SEND for ack.");
        let ack = press(&mut c, &[Key::Send]);
        assert_eq!((ack[0].eos, ack[0].to.as_deref(), ack[0].channel), (Eos::Ack, Some("002191000"), Some(26)));
        press(&mut c, &[Key::RxMsg]);
        assert_eq!(c.display()[1].trim_end(), "From stn: <002191000>");
    }
}

#[cfg(test)]
mod mf_hf_tests {
    use super::*;

    #[test]
    fn mf_hf_distress_frequency_and_work_frequency() {
        let mut c = Controller::new("428123000", Band::MfHf);
        c.tick(0.0, (8, 0, 0));
        assert_eq!(c.display()[0].trim_end(), "Scanning DSC frequencies");
        for k in [Key::Distress, Key::Next, Key::Next, Key::Up, Key::Up, Key::Up, Key::Next] {
            c.press(k);
        }
        let sent: Vec<Call> = [Key::Distress, Key::Send].iter().flat_map(|k| c.press(*k)).collect();
        assert_eq!(sent[0].freq_khz, Some(8414.5));
        assert_eq!(rt_frequency(8414.5), 8291.0);
        let mut c = Controller::new("428123000", Band::MfHf);
        let mut keys = vec![Key::Call];
        keys.extend("002191000".bytes().map(|b| Key::Digit(b - b'0')));
        keys.extend([Key::Next, Key::Next, Key::Next, Key::Digit(4), Key::Digit(1), Key::Digit(2), Key::Digit(5), Key::Dot, Key::Digit(5), Key::Next]);
        for k in keys {
            c.press(k);
        }
        let sent = c.press(Key::Send);
        assert_eq!(sent[0].freq_khz, Some(4125.5));
    }
}

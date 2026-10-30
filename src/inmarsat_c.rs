//! Inmarsat-C ship earth station with its message terminal ("Capsat" type).
//!
//! The terminal logs in to an ocean region (NCS), sends store-and-forward messages through a land
//! earth station (LES) and gets a delivery notification after 3-6 minutes, receives shore-to-ship
//! messages and EGC SafetyNET broadcasts (from the original `INM_EGC.CTL` schedule) for its ocean
//! region and NAVAREA. The distress alert is sent with the transceiver's DISTRESS button; the
//! terminal only sets the distress message (nature, position). The original program's terminal
//! code is not readable, so this follows its manual, menu texts and data files.

use crate::ces_list::OceanRegion;
use crate::dsc::Nature;
use crate::egc::{self, Broadcast};

/// Ocean region of an Inmarsat-C LES code (first digit: 0 AOR-W, 1 AOR-E, 2 POR, 3 IOR).
pub fn les_region(les: u16) -> Option<OceanRegion> {
    match les / 100 {
        0 => Some(OceanRegion::AtlanticWest),
        1 => Some(OceanRegion::AtlanticEast),
        2 => Some(OceanRegion::Pacific),
        3 => Some(OceanRegion::Indian),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    LoggedOut,
    LoggingIn,
    LoggedIn,
    LoggingOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub file: String,
    pub ref_no: u32,
    /// (hour, minute)
    pub time: (u8, u8),
    pub kind: String,
    pub priority: &'static str,
    pub les: u16,
    pub status: String,
}

#[derive(Debug, Clone)]
enum Pending {
    LoginDone,
    LogoutDone,
    Sent(usize),
    Delivered(usize),
    DistressSent,
    DistressAck,
}

pub struct Terminal {
    pub transceiver_on: bool,
    pub link: Link,
    pub region: OceanRegion,
    /// LES used for sending.
    pub les: u16,
    /// NAVAREA/METAREA numbers the EGC receiver accepts.
    pub areas: Vec<u8>,
    pub status: String,
    pub files: Vec<(String, String)>,
    pub transmit_log: Vec<LogEntry>,
    pub receive_log: Vec<LogEntry>,
    pub egc_log: Vec<LogEntry>,
    pub distress_nature: Nature,
    pub distress_active: bool,
    pending: Vec<(f64, Pending)>,
    ref_no: u32,
    egc_done: Vec<(usize, u16)>,
    random: u32,
}

/// Clock and position from the ship.
#[derive(Debug, Clone, Copy)]
pub struct Now {
    pub secs: f64,
    pub hour: u8,
    pub minute: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

impl Terminal {
    pub fn new() -> Terminal {
        Terminal {
            transceiver_on: false,
            link: Link::LoggedOut,
            region: OceanRegion::AtlanticEast,
            les: 102,
            areas: vec![3],
            status: "Idle".into(),
            files: vec![],
            transmit_log: vec![],
            receive_log: vec![],
            egc_log: vec![],
            distress_nature: Nature::Undesignated,
            distress_active: false,
            pending: vec![],
            ref_no: 4000,
            egc_done: vec![],
            random: 0x2545_f491,
        }
    }

    fn rand(&mut self, lo: f64, hi: f64) -> f64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        lo + (hi - lo) * f64::from(self.random % 1000) / 1000.0
    }

    pub fn login(&mut self, region: OceanRegion, now: &Now) {
        if !self.transceiver_on || self.link != Link::LoggedOut {
            return;
        }
        self.region = region;
        self.link = Link::LoggingIn;
        self.status = "<LOGIN>".into();
        let t = now.secs + self.rand(20.0, 45.0);
        self.pending.push((t, Pending::LoginDone));
    }

    pub fn logout(&mut self, now: &Now) {
        if self.link == Link::LoggedIn {
            self.link = Link::LoggingOut;
            self.status = "<LOGOUT>".into();
            let t = now.secs + self.rand(10.0, 20.0);
            self.pending.push((t, Pending::LogoutDone));
        }
    }

    /// Sends the file `name` to `destination`; false if not logged in.
    pub fn transmit(&mut self, name: &str, destination: &str, now: &Now) -> bool {
        if self.link != Link::LoggedIn || !self.transceiver_on {
            self.status = "Not logged in".into();
            return false;
        }
        self.ref_no += 1;
        self.transmit_log.push(LogEntry {
            file: name.into(),
            ref_no: self.ref_no,
            time: (now.hour, now.minute),
            kind: format!("Telex to {destination}"),
            priority: "Normal",
            les: self.les,
            status: "Sending".into(),
        });
        self.status = "<SENDING>".into();
        let i = self.transmit_log.len() - 1;
        let t = now.secs + self.rand(15.0, 40.0);
        self.pending.push((t, Pending::Sent(i)));
        true
    }

    /// The transceiver's DISTRESS button (held long enough).
    pub fn distress_button(&mut self, now: &Now) {
        if !self.transceiver_on || self.distress_active {
            return;
        }
        self.distress_active = true;
        self.status = format!(
            "Sending distress            {:02}-{}-{:4}  {:02}:{:02}:{:02}",
            now.day, MONTHS[usize::from(now.month.clamp(1, 12)) - 1], now.year, now.hour, now.minute, (now.secs as u64 % 60)
        );
        let t = now.secs + 5.0;
        self.pending.push((t, Pending::DistressSent));
    }

    /// A shore-to-ship message arrives.
    pub fn receive(&mut self, name: &str, text: &str, now: &Now, les: u16) {
        self.ref_no += 1;
        self.files.push((name.into(), text.into()));
        self.receive_log.push(LogEntry { file: name.into(), ref_no: self.ref_no, time: (now.hour, now.minute), kind: "Telex".into(), priority: "Normal", les, status: "Received".into() });
        self.status = "Normal priority           message received.".into();
    }

    /// Advances time: completes pending actions and receives scheduled EGC broadcasts.
    /// `egc_text(file)` returns the template of an EGC message file.
    pub fn tick(&mut self, now: &Now, schedule: &[Broadcast], egc_text: &dyn Fn(&str) -> Option<String>) {
        let (due, later): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|(t, _)| *t <= now.secs);
        self.pending = later;
        for (_, p) in due {
            match p {
                Pending::LoginDone => {
                    self.link = Link::LoggedIn;
                    self.status = "Login succesful".into();
                    self.egc_done.clear();
                }
                Pending::LogoutDone => {
                    self.link = Link::LoggedOut;
                    self.status = "Logout succesful".into();
                }
                Pending::Sent(i) => {
                    self.transmit_log[i].status = "Sent".into();
                    self.status = format!("The message is sent to \"{}\"", self.transmit_log[i].kind.trim_start_matches("Telex to "));
                    let t = now.secs + self.rand(180.0, 360.0);
                    self.pending.push((t, Pending::Delivered(i)));
                }
                Pending::Delivered(i) => {
                    self.transmit_log[i].status = "Delivered".into();
                    let name = format!("DLV.{:03}", self.transmit_log[i].ref_no % 1000);
                    let text = format!("DELIVERY NOTIFICATION\nMSG REF {} DELIVERED TO {}\n", self.transmit_log[i].ref_no, self.transmit_log[i].kind);
                    self.receive(&name, &text, now, self.les);
                }
                Pending::DistressSent => {
                    self.status = "Sending Distress successful.".into();
                    let t = now.secs + self.rand(60.0, 120.0);
                    self.pending.push((t, Pending::DistressAck));
                }
                Pending::DistressAck => {
                    self.ref_no += 1;
                    let name = format!("DIS.{:03}", self.ref_no % 1000);
                    self.files.push((name.clone(), format!("DISTRESS ALERT ACKNOWLEDGED BY LES {:03}\nNATURE: {}\nRCC HAS BEEN INFORMED\n", self.les, self.distress_nature.name().to_uppercase())));
                    self.receive_log.push(LogEntry { file: name, ref_no: self.ref_no, time: (now.hour, now.minute), kind: "Telex".into(), priority: "Distress", les: self.les, status: "Received".into() });
                    self.status = "Distress priority         message received.".into();
                    self.distress_active = false;
                }
            }
        }
        if self.link != Link::LoggedIn || !self.transceiver_on {
            return;
        }
        for (i, b) in schedule.iter().enumerate() {
            let in_region = les_region(b.les) == Some(self.region);
            let due_now = b.time == (now.hour, now.minute);
            if !in_region || !due_now || self.egc_done.contains(&(i, b.les)) || !self.wants(b) {
                continue;
            }
            self.egc_done.push((i, b.les));
            self.ref_no += 1;
            let date = format!("{:02}-{}-{:02}", now.day, MONTHS[usize::from(now.month.clamp(1, 12)) - 1], now.year % 100);
            let time = format!("{:02}:{:02}", now.hour, now.minute);
            let ref_no = self.ref_no.to_string();
            let les = format!("{:03}", b.les);
            let text = egc_text(&b.file).map(|t| {
                egc::render(&t, |v| match v {
                    "m_les" => Some(les.clone()),
                    "m_ref_no" => Some(ref_no.clone()),
                    "m_ncs_date" => Some(date.clone()),
                    "m_ncs_time" => Some(time.clone()),
                    _ => None,
                })
            });
            let name = format!("EGC.{:03}", self.ref_no % 1000);
            self.files.push((name.clone(), text.unwrap_or_default()));
            let priority = match b.priority {
                0 => "Routine",
                1 => "Safety",
                _ => "Urgency",
            };
            self.egc_log.push(LogEntry { file: name, ref_no: self.ref_no, time: (now.hour, now.minute), kind: service_name(b.service).into(), priority, les: b.les, status: "Received".into() });
            self.status = format!("{priority} priority EGC message received.");
        }
    }

    /// SafetyNET filter: general calls, our NAVAREA/METAREA numbers, and circular areas.
    fn wants(&self, b: &Broadcast) -> bool {
        match b.service {
            0 | 11 | 14 | 24 => true,
            31 | 13 => b.address.get(..2).and_then(|a| a.parse::<u8>().ok()).is_some_and(|a| self.areas.contains(&a)),
            _ => false,
        }
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

fn service_name(code: u8) -> &'static str {
    match code {
        0 => "General call",
        11 => "System message",
        13 => "Coastal warning",
        14 | 24 => "Distress to area",
        31 => "MetWarn/Forecast",
        _ => "EGC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: f64, hour: u8, minute: u8) -> Now {
        Now { secs, hour, minute, day: 14, month: 2, year: 1997 }
    }

    fn schedule() -> Vec<Broadcast> {
        let ctl = b"EGC\n1:31:03:22:00\nEGC.005\n102\n1400\nEGC\n1:31:06:22:00\nEGC.006\n102\n1800\nEGC\n0:00:0:22:00\nEGC.003\n203\n1808\n";
        egc::parse_control(ctl).unwrap().into_iter().filter_map(|r| match r {
            egc::ControlRecord::Broadcast(b) => Some(b),
            _ => None,
        }).collect()
    }

    #[test]
    fn login_send_deliver() {
        let mut t = Terminal::new();
        t.login(OceanRegion::AtlanticEast, &at(0.0, 10, 0));
        assert_eq!(t.link, Link::LoggedOut); // transceiver off
        t.transceiver_on = true;
        t.login(OceanRegion::AtlanticEast, &at(0.0, 10, 0));
        t.tick(&at(50.0, 10, 1), &[], &|_| None);
        assert_eq!((t.link, t.status.as_str()), (Link::LoggedIn, "Login succesful"));
        assert!(t.transmit("MSG001", "OWNER", &at(60.0, 10, 1)));
        t.tick(&at(110.0, 10, 2), &[], &|_| None);
        assert_eq!(t.transmit_log[0].status, "Sent");
        t.tick(&at(500.0, 10, 8), &[], &|_| None);
        assert_eq!(t.transmit_log[0].status, "Delivered");
        assert_eq!(t.receive_log.len(), 1);
    }

    #[test]
    fn egc_for_region_and_area() {
        let mut t = Terminal::new();
        t.transceiver_on = true;
        t.login(OceanRegion::AtlanticEast, &at(0.0, 21, 59));
        t.tick(&at(50.0, 21, 59), &[], &|_| None);
        let s = schedule();
        let text = |f: &str| Some(format!("LES \\m_les\\ - MSG \\m_ref_no\\ - {f} \\m_ncs_date\\"));
        t.tick(&at(60.0, 22, 0), &s, &text);
        t.tick(&at(61.0, 22, 0), &s, &text); // not twice
        // area 3 from LES 102 (AOR-E); area 6 filtered out; LES 203 is POR.
        assert_eq!(t.egc_log.len(), 1);
        assert_eq!(t.files.last().unwrap().1, "LES 102 - MSG 4001 - EGC.005 14-FEB-97");
    }

    #[test]
    fn distress_alert_acknowledged() {
        let mut t = Terminal::new();
        t.transceiver_on = true;
        t.distress_nature = Nature::Fire;
        t.distress_button(&at(0.0, 9, 0));
        assert!(t.status.starts_with("Sending distress"));
        t.tick(&at(6.0, 9, 0), &[], &|_| None);
        assert_eq!(t.status, "Sending Distress successful.");
        t.tick(&at(200.0, 9, 3), &[], &|_| None);
        assert_eq!(t.receive_log[0].priority, "Distress");
        assert!(!t.distress_active);
    }
}

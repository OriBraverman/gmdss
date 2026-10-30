//! NAVTEX receiver with printer (the "NT-900" unit): 518 kHz broadcasts are printed when the
//! transmitting station (B1) is selected, the message type (B2) is not excluded and the message
//! (B1 B2 B3B4) has not been printed before. Control is done through menus whose texts are
//! printed on the paper roll.
//!
//! Keys: POWER, MENU (next control menu), MONITOR / DIMMER (scroll A->Z / Z->A in a menu,
//! otherwise monitor audio / dim lamps), SELECT, EXCLUDE, LINE FEED (leave the menu),
//! ALARM STOP. Types A, B, D and L cannot be excluded; number 00 is always printed;
//! type D sets the alarm until ALARM STOP (A, B, L too when that alarm is selected).

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Transmitting station letter.
    pub b1: char,
    /// Subject letter.
    pub b2: char,
    /// Serial number 0-99.
    pub number: u8,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Power,
    Menu,
    Select,
    Exclude,
    LineFeed,
    AlarmStop,
    /// MONITOR / up.
    Up,
    /// DIMMER / down.
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Menu {
    Stations(Option<char>),
    Types(Option<char>),
    StatusPrint,
    SelfTest,
    AblAlarm,
    MemoryClear,
}

pub fn subject(b2: char) -> &'static str {
    match b2 {
        'A' => "NAVIGATIONAL WARNING",
        'B' => "METEOROLOGICAL WARNING",
        'C' => "ICE REPORT",
        'D' => "SEARCH AND RESCUE INFORMATION",
        'E' => "METEOROLOGICAL FORECAST",
        'F' => "PILOT SERVICE MESSAGE",
        'G' => "DECCA MESSAGE",
        'H' => "LORAN MESSAGE",
        'I' => "OMEGA MESSAGE",
        'J' => "SATNAV MESSAGE",
        'K' => "OTHER ELECTRONIC NAVAID MESSAGE",
        'L' => "NAVIGATIONAL WARNING",
        'V' | 'W' | 'X' | 'Y' => "SPECIAL SERVICES",
        'Z' => "NO MESSAGE ON HAND",
        _ => "",
    }
}

const MANDATORY: [char; 4] = ['A', 'B', 'D', 'L'];

#[derive(Debug, Clone, Default)]
pub struct Navtex {
    pub on: bool,
    /// Lamp brightness 0-3.
    pub dim: u8,
    pub alarm: bool,
    /// Everything printed so far.
    pub paper: String,
    excluded_stations: BTreeSet<char>,
    excluded_types: BTreeSet<char>,
    printed: BTreeSet<(char, char, u8)>,
    abl_alarm: bool,
    menu: Option<Menu>,
}

impl Navtex {
    fn print(&mut self, s: &str) {
        self.paper.push_str(s);
        if !s.ends_with('\n') {
            self.paper.push('\n');
        }
    }

    pub fn press(&mut self, key: Key) {
        if key == Key::Power {
            self.on = !self.on;
            if self.on {
                self.dim = 3;
                self.print("\u{2591}\u{2591}\u{2591} ALL DATA RETAINED \u{2591}\u{2591}\u{2591}");
            }
            self.menu = None;
            return;
        }
        if !self.on {
            return;
        }
        match (self.menu, key) {
            (_, Key::AlarmStop) => self.alarm = false,
            (None, Key::Menu) => self.enter(Menu::Stations(None)),
            (Some(m), Key::Menu) => self.enter(match m {
                Menu::Stations(_) => Menu::Types(None),
                Menu::Types(_) => Menu::StatusPrint,
                Menu::StatusPrint => Menu::SelfTest,
                Menu::SelfTest => Menu::AblAlarm,
                Menu::AblAlarm => Menu::MemoryClear,
                Menu::MemoryClear => Menu::Stations(None),
            }),
            (None, Key::Up) => {} // MONITOR: audio only
            (None, Key::Down) => self.dim = (self.dim + 3) % 4,
            (Some(Menu::Stations(c)), Key::Up | Key::Down) => {
                let c = scroll(c, key == Key::Up);
                self.menu = Some(Menu::Stations(Some(c)));
                let state = if self.excluded_stations.contains(&c) { "EXCLUDED" } else { "SELECTED" };
                self.print(&format!("'{c}' STATION {state}"));
            }
            (Some(Menu::Stations(Some(c))), Key::Exclude | Key::Select) => {
                let exclude = key == Key::Exclude;
                if exclude { self.excluded_stations.insert(c) } else { self.excluded_stations.remove(&c) };
                self.print(&format!("'{c}' STATION: CHANGED TO {}", if exclude { "EXCLUSION" } else { "SELECTION" }));
            }
            (Some(Menu::Types(c)), Key::Up | Key::Down) => {
                let c = scroll(c, key == Key::Up);
                self.menu = Some(Menu::Types(Some(c)));
                let state = if self.excluded_types.contains(&c) { "EXCLUDED" } else { "SELECTED" };
                self.print(&format!("'{c}' : {}:   {state}", subject(c)));
            }
            (Some(Menu::Types(Some(c))), Key::Exclude) if !MANDATORY.contains(&c) => {
                self.excluded_types.insert(c);
                self.print("CHANGED TO EXCLUSION");
            }
            (Some(Menu::Types(Some(c))), Key::Select) => {
                self.excluded_types.remove(&c);
                self.print("CHANGED TO SELECTION");
            }
            (Some(Menu::StatusPrint), Key::Select) => {
                let status = self.status();
                self.print(&status);
                self.leave();
            }
            (Some(Menu::SelfTest), Key::Select) => {
                self.print(SELF_TEST);
                self.leave();
            }
            (Some(Menu::AblAlarm), Key::Select | Key::Exclude) => {
                self.abl_alarm = key == Key::Select;
                self.print(if self.abl_alarm { "CHANGED TO SELECTION" } else { "CHANGED TO EXCLUSION" });
            }
            (Some(Menu::MemoryClear), Key::Select) => {
                self.excluded_stations.clear();
                self.excluded_types.clear();
                self.printed.clear();
                self.abl_alarm = false;
                self.print("\u{2591}\u{2591}\u{2591} ALL DATA HAVE BEEN CLEARED \u{2591}\u{2591}\u{2591}");
                self.leave();
            }
            (Some(Menu::StatusPrint | Menu::SelfTest | Menu::MemoryClear), Key::Exclude) | (Some(_), Key::LineFeed) => self.leave(),
            (None, Key::LineFeed) => self.paper.push('\n'),
            _ => {} // ignored with a tone
        }
    }

    fn enter(&mut self, m: Menu) {
        self.menu = Some(m);
        let title = match m {
            Menu::Stations(_) => "NAVTEX STATION SELECTING".to_string(),
            Menu::Types(_) => "MESSAGE TYPE SELECTING".into(),
            Menu::StatusPrint => "CONTROLLED STATUS PRINT SELECTING".into(),
            Menu::SelfTest => "SELF-DIAGNOSTICS SELECTING".into(),
            Menu::AblAlarm => format!("MESSAGE 'A','B','L' ALARM:  {}", if self.abl_alarm { "SELECTED" } else { "EXCLUDED" }),
            Menu::MemoryClear => "MEMORY CLEAR SELECTING".into(),
        };
        self.print(&title);
    }

    fn leave(&mut self) {
        self.menu = None;
        self.print("\u{2591}\u{2591}\u{2591} FUNCTION TERMINATED \u{2591}\u{2591}\u{2591}");
    }

    fn status(&self) -> String {
        let list = |set: &BTreeSet<char>| if set.is_empty() { "NONE".to_string() } else { set.iter().collect() };
        format!(
            "  *** CONTROLLED STATUS PRINT ***\nREJECTED STATIONS                 {}\nEXCLUDED MESSAGE TYPES            {}\nMESSAGE 'A','B','L' ALARM:  {}AVAILABLE\n",
            list(&self.excluded_stations),
            list(&self.excluded_types),
            if self.abl_alarm { "" } else { "NOT " }
        )
    }

    pub fn type_excluded(&self, b2: char) -> bool {
        self.excluded_types.contains(&b2)
    }

    /// A broadcast on 518 kHz; returns whether it was printed.
    pub fn receive(&mut self, m: &Message) -> bool {
        if !self.on || self.excluded_stations.contains(&m.b1) {
            return false;
        }
        if self.excluded_types.contains(&m.b2) && !MANDATORY.contains(&m.b2) {
            return false;
        }
        if m.number != 0 && !self.printed.insert((m.b1, m.b2, m.number)) {
            return false;
        }
        self.print(&format!("ZCZC {}{}{:02}\n\n{}\nNNNN\n", m.b1, m.b2, m.number, m.text.trim_end()));
        if m.b2 == 'D' || (self.abl_alarm && matches!(m.b2, 'A' | 'B' | 'L')) {
            self.alarm = true;
        }
        true
    }
}

fn scroll(c: Option<char>, up: bool) -> char {
    match (c, up) {
        (None, true) => 'A',
        (None, false) => 'Z',
        (Some('Z'), true) => 'A',
        (Some('A'), false) => 'Z',
        (Some(c), true) => (c as u8 + 1) as char,
        (Some(c), false) => (c as u8 - 1) as char,
    }
}

const SELF_TEST: &str = "\n\u{2591}\u{2591}\u{2591} SELF-DIAGNOSTICS REPORT \u{2591}\u{2591}\u{2591}
INDICATOR LAMP AND AUDIBLE ALARM TEST:
  CHECK THE FRONT PANEL AND THE ALARM BOX
PRINTER TEST:    CHECK TEST PRINT BELOW
ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890,.;:/
MEMORY TEST:                           OK
RECEIVER TEST: RYRYRYRYRYRYRYRYRY      OK
";

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(b1: char, b2: char, n: u8) -> Message {
        Message { b1, b2, number: n, text: "TEST".into() }
    }

    #[test]
    fn filtering_and_duplicates() {
        let mut n = Navtex::default();
        n.press(Key::Power);
        assert!(n.paper.contains("ALL DATA RETAINED"));
        assert!(n.receive(&msg('R', 'E', 5)));
        assert!(!n.receive(&msg('R', 'E', 5))); // already printed
        assert!(n.receive(&msg('R', 'A', 0)) && n.receive(&msg('R', 'A', 0))); // 00 always printed
        // exclude station B and type E
        n.press(Key::Menu);
        n.press(Key::Up);
        n.press(Key::Up);
        n.press(Key::Exclude);
        assert!(n.paper.contains("'B' STATION: CHANGED TO EXCLUSION"));
        n.press(Key::Menu);
        for _ in 0..5 {
            n.press(Key::Up);
        }
        n.press(Key::Exclude);
        n.press(Key::LineFeed);
        assert!(n.paper.ends_with("FUNCTION TERMINATED \u{2591}\u{2591}\u{2591}\n"));
        assert!(!n.receive(&msg('B', 'A', 1)));
        assert!(!n.receive(&msg('R', 'E', 6)));
        assert!(n.receive(&msg('R', 'A', 7)));
    }

    #[test]
    fn mandatory_types_and_alarm() {
        let mut n = Navtex::default();
        n.press(Key::Power);
        n.press(Key::Menu);
        n.press(Key::Menu);
        for _ in 0..4 {
            n.press(Key::Up);
        }
        n.press(Key::Exclude); // 'D' cannot be excluded
        n.press(Key::LineFeed);
        assert!(n.receive(&msg('S', 'D', 3)) && n.alarm);
        n.press(Key::AlarmStop);
        assert!(!n.alarm);
    }

    #[test]
    fn status_print() {
        let mut n = Navtex::default();
        n.press(Key::Power);
        for _ in 0..3 {
            n.press(Key::Menu);
        }
        n.press(Key::Select);
        assert!(n.paper.contains("REJECTED STATIONS                 NONE"));
    }
}

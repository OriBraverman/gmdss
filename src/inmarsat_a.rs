//! Inmarsat-A ship earth station (the "JUE-45" type): telex calls through a coast earth station.
//!
//! Set-up (SES CONT): ocean region, CES id, antenna azimuth/elevation, priority. LINE requests a
//! channel: if the antenna points at the satellite, the CES header and "GA+" follow within
//! ~10 s. Then dial service code + destination + number + `+` (e.g. `0064213812+`): the
//! subscriber's answerback follows within ~15 s, or a fault code (NP, OCC, NC). `.....` ends
//! the call with date, time and duration. Distress priority connects straight to the RCC.
//! RLS releases the line, WRU asks the far end's answerback, HERE IS sends ours.

use crate::ces_list::{Ces, CesList, OceanRegion};
use crate::subscriber::Subscriber;

/// Sub-satellite longitudes (degrees east) of the four ocean regions.
pub fn satellite_longitude(r: OceanRegion) -> f64 {
    match r {
        OceanRegion::AtlanticEast => -15.5,
        OceanRegion::AtlanticWest => -54.0,
        OceanRegion::Pacific => 178.0,
        OceanRegion::Indian => 64.5,
    }
}

/// Antenna azimuth (true, degrees) and elevation (degrees) towards a geostationary satellite.
pub fn look_angles(lat_deg: f64, lon_deg: f64, sat_lon_deg: f64) -> (f64, f64) {
    let (lat, dlon) = (lat_deg.to_radians(), (sat_lon_deg - lon_deg).to_radians());
    // Initial great-circle bearing towards the sub-satellite point on the equator.
    let az = dlon.sin().atan2(-lat.sin() * dlon.cos()).to_degrees().rem_euclid(360.0);
    let cos_g = lat.cos() * dlon.cos();
    let el = ((cos_g - 0.1512) / (1.0 - cos_g * cos_g).sqrt()).atan().to_degrees();
    (az, el)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Routine,
    Distress,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub region: OceanRegion,
    pub ces_id: String,
    pub azimuth: f64,
    pub elevation: f64,
    pub priority: Priority,
}

#[derive(Debug, Clone, PartialEq)]
enum Line {
    Idle,
    Requesting { at: f64 },
    Connected { since: f64 },
    Dialling { at: f64, number: String },
    ToSubscriber { since: f64, sub: Option<usize> },
}

pub struct Ses {
    pub on: bool,
    pub setup: Setup,
    pub answerback: String,
    /// Everything shown on the VDU.
    pub screen: String,
    line: Line,
    typed: String,
    time: f64,
}

/// What `tick`/`press` need from the ship and the world.
pub struct Env<'a> {
    pub now: f64,
    /// (day, month, year, hour, minute)
    pub date: (u8, u8, u16, u8, u8),
    pub lat: f64,
    pub lon: f64,
    pub ces: &'a CesList,
    pub subscribers: &'a mut Vec<Subscriber>,
}

impl Ses {
    pub fn new(answerback: &str) -> Ses {
        Ses {
            on: false,
            setup: Setup { region: OceanRegion::AtlanticEast, ces_id: "02".into(), azimuth: 0.0, elevation: 0.0, priority: Priority::Routine },
            answerback: answerback.into(),
            screen: String::new(),
            line: Line::Idle,
            typed: String::new(),
            time: 0.0,
        }
    }

    pub fn online(&self) -> bool {
        !matches!(self.line, Line::Idle)
    }

    fn print(&mut self, s: &str) {
        self.screen.push_str(s);
    }

    fn ces<'c>(&self, list: &'c CesList) -> Option<&'c Ces> {
        list.region(self.setup.region).iter().find(|c| c.id == self.setup.ces_id)
    }

    /// The antenna is within 5 degrees of the satellite.
    pub fn aligned(&self, lat: f64, lon: f64) -> bool {
        let (az, el) = look_angles(lat, lon, satellite_longitude(self.setup.region));
        let daz = ((self.setup.azimuth - az + 540.0) % 360.0 - 180.0).abs();
        el > 0.0 && daz <= 5.0 && (self.setup.elevation - el).abs() <= 5.0
    }

    /// LINE: request a telex channel.
    pub fn line(&mut self, env: &Env) {
        if self.on && matches!(self.line, Line::Idle) {
            self.line = Line::Requesting { at: env.now };
        }
    }

    /// RLS: back to idle.
    pub fn release(&mut self, env: &mut Env) {
        if let Line::ToSubscriber { sub: Some(i), .. } = self.line {
            env.subscribers[i].busy = false;
        }
        if self.online() {
            self.print("\n");
        }
        self.line = Line::Idle;
        self.typed.clear();
    }

    /// HERE IS: send our answerback.
    pub fn here_is(&mut self) {
        if self.online() {
            let ab = self.answerback.clone();
            self.print(&format!("\n{ab}\n"));
        }
    }

    /// WRU: ask the far end's answerback.
    pub fn wru(&mut self, env: &Env) {
        match &self.line {
            Line::ToSubscriber { sub: Some(i), .. } => {
                let ab = env.subscribers[*i].answerback.clone();
                self.print(&format!("\n{ab}\n"));
            }
            Line::Connected { .. } | Line::ToSubscriber { .. } => {
                if let Some(c) = self.ces(env.ces) {
                    let ab = if self.setup.priority == Priority::Distress { c.answerback.clone() } else { format!("{} {}", c.name.trim(), c.id) };
                    self.print(&format!("\n{ab}\n"));
                }
            }
            _ => {}
        }
    }

    /// A character typed on the keyboard (sent to the line when on-line).
    pub fn type_char(&mut self, c: char, env: &mut Env) {
        if !self.on {
            return;
        }
        let c = c.to_ascii_uppercase();
        self.print(&c.to_string());
        match self.line {
            Line::Connected { .. } => {
                self.typed.push(c);
                if c == '+' {
                    let number = std::mem::take(&mut self.typed).trim().to_string();
                    self.line = Line::Dialling { at: env.now, number };
                }
            }
            Line::ToSubscriber { since, .. } => {
                self.typed.push(c);
                if self.typed.ends_with(".....") {
                    let (d, m, y, h, mi) = env.date;
                    let mins = (env.now - since) / 60.0;
                    self.print(&format!("\n{d:02}.{m:02}.{y} {h:02}:{mi:02}  DURATION {mins:.1} MIN\n"));
                    self.release(env);
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, env: &mut Env) {
        self.time = env.now;
        match self.line.clone() {
            Line::Requesting { at } if env.now - at >= 8.0 => {
                let ces = self.ces(env.ces).cloned();
                match (ces, self.aligned(env.lat, env.lon)) {
                    (Some(c), true) => {
                        let (d, m, y, h, mi) = env.date;
                        let header = c.header_format.replace("y", &format!("{:02}", y % 100)).replace("m", &format!("{m:02}")).replace("d", &format!("{d:02}")).replace("h", &format!("{h:02}")).replace("n", &format!("{mi:02}"));
                        self.print(&format!("\n{}\n", header.trim()));
                        if self.setup.priority == Priority::Distress {
                            // Distress priority: straight to the rescue co-ordination centre.
                            self.print(&format!("{}\nGA+\n", c.answerback.trim()));
                            self.line = Line::ToSubscriber { since: env.now, sub: None };
                        } else {
                            self.print("GA+\n");
                            self.line = Line::Connected { since: env.now };
                        }
                    }
                    (None, _) => {
                        self.print("\nCES NOT AVAILABLE IN THIS OCEAN REGION\n");
                        self.line = Line::Idle;
                    }
                    (_, false) => {
                        self.print("\nNO RESPONSE FROM CES - CHECK ANTENNA\n");
                        self.line = Line::Idle;
                    }
                }
            }
            Line::Dialling { at, number } if env.now - at >= 6.0 => {
                let result = dial(&number, env.subscribers);
                match result {
                    Ok(i) => {
                        env.subscribers[i].busy = true;
                        let ab = env.subscribers[i].answerback.clone();
                        self.print(&format!("\n{ab}\n"));
                        self.line = Line::ToSubscriber { since: env.now, sub: Some(i) };
                    }
                    Err(code) => {
                        self.print(&format!("\n{code}\n"));
                        self.line = Line::Connected { since: env.now };
                    }
                }
            }
            _ => {}
        }
    }
}

/// Service 00 (automatic) to a land subscriber: "00" + country + number + "+".
fn dial(number: &str, subs: &[Subscriber]) -> Result<usize, &'static str> {
    let Some(rest) = number.strip_prefix("00") else {
        return Err(if number.starts_with("91") { "LINE TEST OK" } else { "NA" });
    };
    if ["581", "582", "583", "584"].iter().any(|r| rest.starts_with(r)) {
        return Err("NC"); // ship-to-ship not available in the trainer
    }
    match subs.iter().position(|s| s.number == rest) {
        None => Err("NP"),
        Some(i) if subs[i].busy => Err("OCC"),
        Some(i) => Ok(i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CES: &str = "[ATLANTIC EAST]\n04,EIK,NORWAY,y-m-d h:n EIK NORWAY 04,\n02,GOONHILLY,UK,GH INMARSAT h:n,FALMCS G\n";

    fn sub() -> Vec<Subscriber> {
        vec![Subscriber { name: "Agent".into(), number: "64213812+".into(), answerback: "213812 PSC SU".into(), busy: false }]
    }

    #[test]
    fn look_angles_point_at_satellite() {
        // Haifa area towards AOR-East (15.5 W): roughly west, low.
        let (az, el) = look_angles(32.9, 34.7, -15.5);
        assert!((240.0..270.0).contains(&az) && (5.0..30.0).contains(&el), "{az} {el}");
        // On the equator under the satellite: straight up.
        let (_, el) = look_angles(0.0, -15.5, -15.5);
        assert!(el > 89.0);
    }

    #[test]
    fn telex_call_through_ces() {
        let ces = CesList::parse(CES.as_bytes());
        let mut subs = sub();
        let mut ses = Ses::new("1234567 SEASTAR X");
        ses.on = true;
        let (az, el) = look_angles(32.9, 34.7, -15.5);
        ses.setup = Setup { region: OceanRegion::AtlanticEast, ces_id: "04".into(), azimuth: az, elevation: el, priority: Priority::Routine };
        let mut env = Env { now: 0.0, date: (14, 2, 1997, 16, 5), lat: 32.9, lon: 34.7, ces: &ces, subscribers: &mut subs };
        ses.line(&env);
        env.now = 9.0;
        ses.tick(&mut env);
        assert!(ses.screen.contains("97-02-14 16:05 EIK NORWAY 04\nGA+"), "{}", ses.screen);
        for c in "0064999999+".chars() {
            ses.type_char(c, &mut env);
        }
        env.now = 16.0;
        ses.tick(&mut env);
        assert!(ses.screen.ends_with("\nNP\n"));
        for c in "0064213812+".chars() {
            ses.type_char(c, &mut env);
        }
        env.now = 23.0;
        ses.tick(&mut env);
        assert!(ses.screen.ends_with("213812 PSC SU\n") && env.subscribers[0].busy);
        env.now = 143.0;
        for c in "HELLO.....".chars() {
            ses.type_char(c, &mut env);
        }
        assert!(ses.screen.contains("DURATION 2.0 MIN") && !env.subscribers[0].busy && !ses.online());
    }

    #[test]
    fn misaligned_antenna_and_distress() {
        let ces = CesList::parse(CES.as_bytes());
        let mut subs = sub();
        let mut ses = Ses::new("X");
        ses.on = true;
        let mut env = Env { now: 0.0, date: (1, 1, 2000, 0, 0), lat: 32.9, lon: 34.7, ces: &ces, subscribers: &mut subs };
        ses.line(&env);
        env.now = 9.0;
        ses.tick(&mut env);
        assert!(ses.screen.contains("CHECK ANTENNA"));
        let (az, el) = look_angles(32.9, 34.7, -15.5);
        ses.setup.azimuth = az;
        ses.setup.elevation = el;
        ses.setup.priority = Priority::Distress;
        ses.line(&env);
        env.now = 18.0;
        ses.tick(&mut env);
        assert!(ses.screen.contains("GH INMARSAT 00:00\nFALMCS G\nGA+"), "{}", ses.screen);
    }
}

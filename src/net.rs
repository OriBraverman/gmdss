//! Instructor / student network messages, one text line each (tab separated), so the protocol is
//! readable in a terminal and needs no serialization library.
//!
//! ```text
//! HELLO   name  mmsi  lat_min  lon_min        student -> instructor, on connect and after changes
//! EX      exercise                             instructor -> students (two_tone, sart_radar, ...)
//! DSC     lat_min  lon_min  call fields...     a ship's DSC transmission, relayed to the others
//! NOTE    text                                 student -> instructor log
//! ```

use crate::dsc::{Call, Category, Eos, Format, Nature, Position, Telecommand};

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Hello { name: String, mmsi: String, lat_min: i32, lon_min: i32 },
    Exercise(String),
    /// A DSC call sent by the ship at the position (minutes of arc).
    Dsc { lat_min: i32, lon_min: i32, call: Call },
    Note(String),
}

const FORMATS: [Format; 5] = [Format::Distress, Format::AllShips, Format::Group, Format::Area, Format::Individual];
const CATEGORIES: [Category; 5] = [Category::Distress, Category::Urgency, Category::Safety, Category::ShipsBusiness, Category::Routine];
const TELECOMMANDS: [Telecommand; 7] = [
    Telecommand::F3ESimplex,
    Telecommand::F3EDuplex,
    Telecommand::DistressAck,
    Telecommand::DistressRelay,
    Telecommand::Test,
    Telecommand::Unable,
    Telecommand::NoInfo,
];
const EOS: [Eos; 3] = [Eos::AckRequested, Eos::Ack, Eos::Other];

fn named<T: std::fmt::Debug + Copy>(all: &[T], name: &str) -> Option<T> {
    all.iter().copied().find(|v| format!("{v:?}") == name)
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map_or("-".into(), |v| v.to_string())
}

fn parse_opt<T: std::str::FromStr>(s: &str) -> Option<Option<T>> {
    if s == "-" { Some(None) } else { s.parse().ok().map(Some) }
}

/// Tabs and line breaks cannot occur inside a field.
fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

impl Msg {
    pub fn to_line(&self) -> String {
        match self {
            Msg::Hello { name, mmsi, lat_min, lon_min } => format!("HELLO\t{}\t{}\t{lat_min}\t{lon_min}", clean(name), clean(mmsi)),
            Msg::Exercise(e) => format!("EX\t{}", clean(e)),
            Msg::Note(t) => format!("NOTE\t{}", clean(t)),
            Msg::Dsc { lat_min, lon_min, call: c } => [
                "DSC".to_string(),
                lat_min.to_string(),
                lon_min.to_string(),
                format!("{:?}", c.format),
                format!("{:?}", c.category),
                clean(&c.from),
                opt(c.to.as_deref().map(clean)),
                format!("{:?}", c.tc1),
                opt(c.channel),
                opt(c.freq_khz),
                opt(c.nature.map(|n| format!("{n:?}"))),
                opt(c.position.map(|p| format!("{},{}", p.lat_min, p.lon_min))),
                opt(c.utc.map(|(h, m)| format!("{h}:{m}"))),
                opt(c.distress_ship.as_deref().map(clean)),
                format!("{:?}", c.eos),
            ]
            .join("\t"),
        }
    }

    pub fn from_line(line: &str) -> Option<Msg> {
        let f: Vec<&str> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
        match f.as_slice() {
            ["HELLO", name, mmsi, lat, lon] => Some(Msg::Hello { name: name.to_string(), mmsi: mmsi.to_string(), lat_min: lat.parse().ok()?, lon_min: lon.parse().ok()? }),
            ["EX", e] => Some(Msg::Exercise(e.to_string())),
            ["NOTE", t] => Some(Msg::Note(t.to_string())),
            ["DSC", lat, lon, format, category, from, to, tc1, channel, freq, nature, pos, utc, distress, eos] => {
                let position = match *pos {
                    "-" => None,
                    p => {
                        let (a, b) = p.split_once(',')?;
                        Some(Position { lat_min: a.parse().ok()?, lon_min: b.parse().ok()? })
                    }
                };
                let utc = match *utc {
                    "-" => None,
                    u => {
                        let (h, m) = u.split_once(':')?;
                        Some((h.parse().ok()?, m.parse().ok()?))
                    }
                };
                let call = Call {
                    format: named(&FORMATS, format)?,
                    category: named(&CATEGORIES, category)?,
                    from: from.to_string(),
                    to: parse_opt(to)?,
                    tc1: named(&TELECOMMANDS, tc1)?,
                    channel: parse_opt(channel)?,
                    freq_khz: parse_opt(freq)?,
                    nature: match *nature {
                        "-" => None,
                        n => Some(named(&Nature::ALL, n)?),
                    },
                    position,
                    utc,
                    distress_ship: parse_opt(distress)?,
                    eos: named(&EOS, eos)?,
                };
                Some(Msg::Dsc { lat_min: lat.parse().ok()?, lon_min: lon.parse().ok()?, call })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut call = Call::distress("211234560", Nature::Fire, Some(Position { lat_min: 1975, lon_min: -2100 }), Some((13, 5)));
        call.freq_khz = Some(2187.5);
        let msgs = [
            Msg::Hello { name: "SEA\tSTAR".into(), mmsi: "428123000".into(), lat_min: 1975, lon_min: 2080 },
            Msg::Exercise("two_tone".into()),
            Msg::Note("distress alert sent".into()),
            Msg::Dsc { lat_min: 1975, lon_min: 2080, call: call.clone() },
            Msg::Dsc { lat_min: 0, lon_min: 0, call: Call::distress_ack("004281000", &call) },
        ];
        for m in msgs {
            let back = Msg::from_line(&m.to_line()).unwrap();
            match (&m, &back) {
                (Msg::Hello { name, .. }, Msg::Hello { name: n2, .. }) => assert_eq!(n2, &name.replace('\t', " ")),
                _ => assert_eq!(back, m),
            }
        }
        assert_eq!(Msg::from_line("DSC\tbad"), None);
        assert_eq!(Msg::from_line("HELLO\tx\ty\tnot a number\t0"), None);
    }
}

//! Inmarsat-C EGC (Enhanced Group Call) broadcasts: the schedule `INM_EGC.CTL` and the message
//! text templates (`EGC.nnn`, `DISTEGC.001`).
//!
//! The original reads these in a module whose code is not available, so this format comes from
//! the data alone. Field meanings marked "confirmed" match the header line of the message they
//! point to in all 40 shipped records; the others are not known.
//!
//! The control file is a sequence of non-blank lines grouped into records by a type line:
//! ```text
//! Distress_EGC        file, number
//! distegc.001
//! 180
//! EGC                 params, file, LES, number
//! 1:31:02:22:00
//! EGC.002
//! 121
//! 1654
//! ```

use crate::dos::{fgets_lines, latin1};

#[derive(Debug, Clone, PartialEq)]
pub enum ControlRecord {
    /// The distress alert relay message.
    Distress { file: String, number: u32 },
    Broadcast(Broadcast),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Broadcast {
    /// 0 on all routine/system calls, 1 on safety calls, but the two distress calls use 1
    /// (EGC.001) and 2 (EGC.019), so the exact meaning is not known.
    pub priority: u8,
    /// EGC service code. Confirmed: 0 general call, 11 system message, 13 coastal warning,
    /// 14 and 24 call to a circular area, 31 MetWarn/forecast to a NAVAREA/METAREA.
    pub service: u8,
    /// Address, depending on the service: "0" (all), an area number ("02"), a circular area
    /// "08N052E500" (lat, lon, radius in nm), or a coastal area ("03HB").
    pub address: String,
    /// Time of day the broadcast is due (hour, minute). Meaning inferred, not confirmed.
    pub time: (u8, u8),
    pub file: String,
    /// Confirmed: the station shown as "LES nnn" in the message header. Probably an ocean
    /// region digit + two-digit station id, but some codes (121, 322) match no `INMA.CES` entry.
    pub les: u16,
    /// Meaning unknown (not the message size).
    pub number: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    /// 1-based index among the non-blank lines.
    pub line: usize,
    pub message: String,
}

/// Parses `INM_EGC.CTL`.
pub fn parse_control(bytes: &[u8]) -> Result<Vec<ControlRecord>, ParseError> {
    let lines: Vec<(usize, String)> = fgets_lines(bytes, 256)
        .iter()
        .map(|l| latin1(l).trim().to_string())
        .filter(|l| !l.is_empty())
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .collect();
    let mut records = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (n, kind) = &lines[i];
        let fields = |count: usize| -> Result<Vec<&str>, ParseError> {
            let f: Vec<&str> = lines[i + 1..].iter().take(count).map(|(_, s)| s.as_str()).collect();
            if f.len() < count {
                return Err(err(*n, format!("`{kind}` record needs {count} more lines")));
            }
            Ok(f)
        };
        match kind.as_str() {
            "Distress_EGC" => {
                let f = fields(2)?;
                records.push(ControlRecord::Distress { file: f[0].into(), number: num(n + 2, f[1])? });
                i += 3;
            }
            "EGC" => {
                let f = fields(4)?;
                let (priority, service, address, time) = parse_params(n + 1, f[0])?;
                records.push(ControlRecord::Broadcast(Broadcast {
                    priority,
                    service,
                    address,
                    time,
                    file: f[1].into(),
                    les: num(n + 3, f[2])?,
                    number: num(n + 4, f[3])?,
                }));
                i += 5;
            }
            other => return Err(err(*n, format!("expected `EGC` or `Distress_EGC`, found `{other}`"))),
        }
    }
    Ok(records)
}

/// `priority:service:address:HH:MM`
fn parse_params(line: usize, s: &str) -> Result<(u8, u8, String, (u8, u8)), ParseError> {
    let p: Vec<&str> = s.split(':').collect();
    if p.len() != 5 {
        return Err(err(line, format!("expected priority:service:address:HH:MM, found `{s}`")));
    }
    Ok((num(line, p[0])?, num(line, p[1])?, p[2].into(), (num(line, p[3])?, num(line, p[4])?)))
}

fn num<T: std::str::FromStr>(line: usize, s: &str) -> Result<T, ParseError> {
    s.parse().map_err(|_| err(line, format!("not a number: `{s}`")))
}

fn err(line: usize, message: String) -> ParseError {
    ParseError { line, message }
}

/// Fills `\name\` placeholders in a message template from `value(name)`. Names without a value
/// are left as written; the shipped EGC.018 has a misspelled `\m_reg_no\` that stays that way
/// (how the original shows it is not known).
///
/// Names seen in the program: `m_les m_ref_no m_ncs_date m_ncs_time m_proc` and, for the distress
/// relay, `d_mes_egc_title d_ncs_egc_title d_mes_number d_distress_nature d_lat d_lon d_course
/// d_speed d_utc d_position_status`.
pub fn render(template: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('\\') {
        out += &rest[..start];
        let after = &rest[start + 1..];
        let name_len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
        let name = &after[..name_len];
        match (after[name_len..].starts_with('\\'), value(name)) {
            (true, Some(v)) if !name.is_empty() => {
                out += &v;
                rest = &after[name_len + 1..];
            }
            _ => {
                out.push('\\');
                rest = after;
            }
        }
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\r\nDistress_EGC\r\ndist.001\r\n90\r\nEGC\r\n2:14:10S020W100:07:30\r\nMSG.001\r\n305\r\n42\r\n\
                          EGC\r\n0:00:0:18:00\r\nMSG.002\r\n001\r\n7";

    #[test]
    fn parses_control_file() {
        let recs = parse_control(SAMPLE.as_bytes()).unwrap();
        assert_eq!(recs, vec![
            ControlRecord::Distress { file: "dist.001".into(), number: 90 },
            ControlRecord::Broadcast(Broadcast {
                priority: 2, service: 14, address: "10S020W100".into(), time: (7, 30),
                file: "MSG.001".into(), les: 305, number: 42,
            }),
            ControlRecord::Broadcast(Broadcast {
                priority: 0, service: 0, address: "0".into(), time: (18, 0),
                file: "MSG.002".into(), les: 1, number: 7,
            }),
        ]);
    }

    #[test]
    fn control_errors() {
        assert_eq!(parse_control(b"EGC\n1:2:3\nF\n1\n2\n").unwrap_err().line, 2);
        assert_eq!(parse_control(b"EGC\n1:2:3:4:5\nF\n").unwrap_err().line, 1);
        assert_eq!(parse_control(b"NAVTEX\n").unwrap_err().line, 1);
    }

    #[test]
    fn renders_templates() {
        let vars = |n: &str| match n {
            "m_les" => Some("102".to_string()),
            "m_ref_no" => Some("4711".to_string()),
            _ => None,
        };
        assert_eq!(render(r"LES \m_les\ - MSG \m_ref_no\ \m_typo\ a\b \\ end\", vars),
                   r"LES 102 - MSG 4711 \m_typo\ a\b \\ end\");
    }
}

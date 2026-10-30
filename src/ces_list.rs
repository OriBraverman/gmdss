//! `INMA.CES`: Inmarsat-C coast earth stations (CES, also called LES) per ocean region.
//!
//! ```text
//! [ATLANTIC EAST]
//! 12,NORTHPORT,     ATLANTIS,,
//! 02,SOUTHCAPE,     ATLANTIS,   SC INMARSAT h:n,     SCAPES X
//! ```
//! A line equal to a region header selects the region; later lines are stations in it.
//! A station is `id,name,country,header format,answerback`. Leading spaces of each field are
//! skipped, trailing ones kept. The first four fields must end in a comma; the answerback is the
//! rest of the line. A row with a field that is too long or missing is silently skipped, as are
//! rows before the first header and rows beyond 20 per region.

use crate::dos::{fgets_lines, latin1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OceanRegion {
    AtlanticEast = 0,
    AtlanticWest = 1,
    Pacific = 2,
    Indian = 3,
}

impl OceanRegion {
    pub const ALL: [OceanRegion; 4] = [Self::AtlanticEast, Self::AtlanticWest, Self::Pacific, Self::Indian];

    /// The section header in `INMA.CES`.
    pub fn header(self) -> &'static str {
        ["[ATLANTIC EAST]", "[ATLANTIC WEST]", "[PACIFIC]", "[INDIAN]"][self as usize]
    }

    /// The name the program shows for the region.
    pub fn display_name(self) -> &'static str {
        ["ATLANTIC OCEAN EAST", "ATLANTIC OCEAN WEST", "PACIFIC OCEAN", "INDIAN OCEAN"][self as usize]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ces {
    /// Two-digit station id within the region, e.g. "12".
    pub id: String,
    pub name: String,
    pub country: String,
    /// Header line format of the station's messages, e.g. "y-m-d h:n EIK NORWAY 04".
    pub header_format: String,
    pub answerback: String,
}

/// Stations per region, indexed by [`OceanRegion`] as usize.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CesList {
    pub regions: [Vec<Ces>; 4],
}

const MAX_PER_REGION: usize = 20;
const MAX_LEN: [usize; 5] = [2, 20, 20, 24, 24];

impl CesList {
    pub fn region(&self, r: OceanRegion) -> &[Ces] {
        &self.regions[r as usize]
    }

    pub fn parse(bytes: &[u8]) -> CesList {
        let mut list = CesList::default();
        let mut region: Option<usize> = None;
        for raw in fgets_lines(bytes, 128) {
            let line = match raw.iter().position(|&c| c == b'\n') {
                Some(nl) => &raw[..nl],
                None => &raw[..],
            };
            if line.is_empty() {
                continue;
            }
            if let Some(r) = OceanRegion::ALL.iter().position(|r| r.header().as_bytes() == line) {
                region = Some(r);
                continue;
            }
            let Some(r) = region else { continue };
            if list.regions[r].len() < MAX_PER_REGION {
                if let Some(ces) = parse_row(line) {
                    list.regions[r].push(ces);
                }
            }
        }
        list
    }
}

fn parse_row(line: &[u8]) -> Option<Ces> {
    let mut fields: Vec<&[u8]> = Vec::with_capacity(5);
    let mut rest = line;
    for i in 0..5 {
        let start = rest.iter().position(|&c| c != b' ').unwrap_or(rest.len());
        rest = &rest[start..];
        let field = if i < 4 {
            let comma = rest.iter().position(|&c| c == b',')?;
            let f = &rest[..comma];
            rest = &rest[comma + 1..];
            f
        } else {
            rest
        };
        if field.len() > MAX_LEN[i] {
            return None;
        }
        fields.push(field);
    }
    let name = latin1(fields[1]);
    let answerback = latin1(fields[4]);
    let header_format = if fields[3].is_empty() {
        default_header(&name, &answerback)
    } else {
        latin1(fields[3])
    };
    Some(Ces { id: latin1(fields[0]), name, country: latin1(fields[2]), header_format, answerback })
}

/// Empty format becomes "y-m-d h:n " + name. The original builds it in a 25-byte field that is
/// followed by the answerback field, so a name over 14 chars runs into it: the visible format is
/// then its first 25 chars followed by the answerback. No shipped station hits this.
fn default_header(name: &str, answerback: &str) -> String {
    let s = format!("y-m-d h:n {name}");
    if s.len() <= 24 { s } else { format!("{}{answerback}", &s[..25]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "ignored,before,any,region,\r\n\
[PACIFIC]\r\n\
07,  HARBOUR,   NOWHERE,  d h:n HB 07,  TEST ANSWER  \r\n\
08,QUIET,NOWHERE,,\r\n\
123,TOO LONG ID,X,,\r\n\
09,MISSING COMMAS,X\r\n\
\r\n\
[INDIAN]\r\n\
01,A VERY LONG NAME XX,LAND,,AB\r\n";

    fn ces(id: &str, name: &str, country: &str, fmt: &str, ab: &str) -> Ces {
        Ces { id: id.into(), name: name.into(), country: country.into(), header_format: fmt.into(), answerback: ab.into() }
    }

    #[test]
    fn parses_regions_and_rows() {
        let list = CesList::parse(SAMPLE.as_bytes());
        assert!(list.region(OceanRegion::AtlanticEast).is_empty());
        assert_eq!(list.region(OceanRegion::Pacific), &[
            ces("07", "HARBOUR", "NOWHERE", "d h:n HB 07", "TEST ANSWER  "),
            ces("08", "QUIET", "NOWHERE", "y-m-d h:n QUIET", ""),
        ]);
        // 19-char name: default header overflows into the answerback field
        assert_eq!(list.region(OceanRegion::Indian), &[
            ces("01", "A VERY LONG NAME XX", "LAND", "y-m-d h:n A VERY LONG NAMAB", "AB"),
        ]);
    }

    #[test]
    fn region_limit() {
        let mut text = String::from("[INDIAN]\n");
        for i in 0..25 {
            text += &format!("{i:02},N,C,,\n");
        }
        assert_eq!(CesList::parse(text.as_bytes()).region(OceanRegion::Indian).len(), 20);
    }
}

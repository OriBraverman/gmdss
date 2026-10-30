//! `COAST_RS.DAT`, the chart's coast radio station positions.
//!
//! Two text lines, `^Z`, `u16 count`, then `count` records
//! `{f32 latitude, f32 longitude (minutes of arc, north/east positive), 4-char id, u8 length, name}`.
//! Id ranges: 8000-8999 NAVTEX stations, 9000-9999 Inmarsat earth stations, the rest coast radio
//! stations.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    CoastRadio,
    Navtex,
    EarthStation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub lat_min: f32,
    pub lon_min: f32,
    pub kind: Kind,
}

pub fn parse(bytes: &[u8]) -> Option<Vec<Station>> {
    let z = bytes.iter().position(|&c| c == 0x1a)?;
    let count = usize::from(u16::from_le_bytes([*bytes.get(z + 2)?, *bytes.get(z + 3)?]));
    let mut p = z + 4;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let f = |o: usize| bytes.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let (lat, lon) = (f(p)?, f(p + 4)?);
        let id = crate::dos::latin1(bytes.get(p + 8..p + 12)?);
        let len = usize::from(*bytes.get(p + 12)?);
        let name = crate::dos::latin1(bytes.get(p + 13..p + 13 + len)?).trim().to_string();
        p += 13 + len;
        let kind = match id.parse::<u32>() {
            Ok(8000..=8999) => Kind::Navtex,
            Ok(9000..=9999) => Kind::EarthStation,
            _ => Kind::CoastRadio,
        };
        out.push(Station { id, name, lat_min: lat, lon_min: lon, kind });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records() {
        let mut b = b"Coast Radio Stations\r\nAux\r\n\x1a\x00\x02\x00".to_vec();
        for (lat, lon, id, name) in [(3089.0f32, -168.0f32, "0055", "PORTISHEAD"), (1968.0, 2100.0, "8028", "Haifa")] {
            b.extend(lat.to_le_bytes());
            b.extend(lon.to_le_bytes());
            b.extend(id.bytes());
            b.push(name.len() as u8);
            b.extend(name.bytes());
        }
        let s = parse(&b).unwrap();
        assert_eq!((s[0].name.as_str(), s[0].lat_min, s[0].kind), ("PORTISHEAD", 3089.0, Kind::CoastRadio));
        assert_eq!((s[1].id.as_str(), s[1].kind), ("8028", Kind::Navtex));
    }
}

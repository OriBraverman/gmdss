//! Radio ranges. With the original `ETHER.DAT` (loaded by `use_ether`) levels follow the
//! original engine (E_STUD `signal_level`); without it, a simple own model: VHF is line of sight,
//! MF ground wave reaches a few hundred miles, NAVTEX (518 kHz) about 400 nm, HF sky wave depends
//! on band and time of day.
//!
//! `ETHER.DAT`, one block per band (MHz):
//! ```text
//! Frequency  lo hi  P b c             reference power P (W); level c at b nm without absorption
//!   Latitude 50.0 / Month 12          nearest is used; the file has one of each
//!     Hour h  d l  d l ...  end       level l at distance d (nm), by local hour at the midpoint
//! ```
//! Level at `d` nm, from the row of the nearest hour: up to the first point (d1, l1), spreading
//! and absorption `b c e^(-k d) / d` (d at least 0.5) with `k` set so that it meets the point; then
//! straight lines between the points; nothing beyond the last one. Times `sqrt(power / P)`.
//! The original receives a signal when it beats the sum of all signals on the frequency plus the
//! receiver noise (bandwidth in Hz x 0.0007), divided by 1.5 (`decoded`): alone, when it is over
//! twice the noise. Receiver bandwidths: VHF DSC 500 Hz, MF/HF DSC 200 Hz, voice 1400 Hz.

use std::sync::OnceLock;

/// One band of `ETHER.DAT`.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    pub mhz: (f64, f64),
    /// Transmitter power (W) the table is for.
    pub ref_w: f64,
    /// b x c: level x distance near the transmitter.
    pub near: f64,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub hour: f64,
    /// (distance nm, level); distances are whole miles.
    pub points: Vec<(f64, f64)>,
    /// Absorption per mile, from the first point.
    pub k: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ether {
    pub bands: Vec<Band>,
}

/// Level when the band has no row (as the original).
const NO_ROW: f64 = 100.0;

/// Receiver noise level for a bandwidth (E_STUD `T_watch::nature_noise_level`).
pub fn noise(bandwidth_hz: f64) -> f64 {
    bandwidth_hz * 0.0007
}

/// The DSC receivers' bandwidth (SP_RADIO watch set-up): VHF 500 Hz, MF/HF 200 Hz. NAVTEX, the
/// same narrow FSK as MF DSC, uses 200 Hz too.
pub fn dsc_bandwidth_hz(khz: f64) -> f64 {
    if khz > 30_000.0 { 500.0 } else { 200.0 }
}

/// Which of the signals on one frequency the receiver decodes (E_STUD `std_cf`): the strongest,
/// when it beats the sum of them all plus the noise, divided by 1.5. None: noise or a collision.
pub fn decoded(levels: &[f64], noise: f64) -> Option<usize> {
    let (i, &max) = levels.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    ((levels.iter().sum::<f64>() + noise) / 1.5 < max).then_some(i)
}

impl Ether {
    pub fn parse(text: &str) -> Option<Ether> {
        let mut bands: Vec<Band> = vec![];
        for line in text.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| w.get(i).and_then(|t| t.parse::<f64>().ok());
            match w.first() {
                Some(&"Frequency") => {
                    bands.push(Band { mhz: (num(1)?, num(2)?), ref_w: num(3)?, near: num(4)? * num(5)?, rows: vec![] });
                }
                Some(&"Hour") => {
                    let band = bands.last_mut()?;
                    let nums: Vec<f64> = w.get(2..)?.iter().map_while(|t| t.parse().ok()).collect();
                    let points: Vec<(f64, f64)> = nums.chunks_exact(2).map(|p| (p[0].trunc(), p[1])).collect();
                    let &(d1, l1) = points.first()?;
                    let k = -(d1 * l1 / band.near).ln() / d1;
                    band.rows.push(Row { hour: num(1)?, points, k });
                }
                _ => {}
            }
        }
        (!bands.is_empty()).then_some(Ether { bands })
    }

    fn band(&self, khz: f64) -> Option<&Band> {
        let mhz = khz / 1000.0;
        self.bands.iter().find(|b| (b.mhz.0..=b.mhz.1).contains(&mhz))
    }

    /// Signal level `nm` from a transmitter of `watts` on `khz` at local `hour`; None outside the
    /// bands in the file.
    pub fn level(&self, khz: f64, hour: f64, nm: f64, watts: f64) -> Option<f64> {
        let band = self.band(khz)?;
        let apart = |r: &&Row| {
            let d = (r.hour - hour).abs() % 24.0;
            d.min(24.0 - d)
        };
        let Some(row) = band.rows.iter().min_by(|a, b| apart(a).total_cmp(&apart(b))) else { return Some(NO_ROW) };
        let (p, last) = (&row.points, row.points[row.points.len() - 1]);
        let level = if nm < p[0].0 {
            let d = nm.max(0.5);
            band.near * (-row.k * d).exp() / d
        } else if nm >= last.0 {
            if nm == last.0 { last.1 } else { 0.0 }
        } else {
            let w = p.windows(2).find(|w| nm <= w[1].0)?;
            w[0].1 + (nm - w[0].0) * (w[1].1 - w[0].1) / (w[1].0 - w[0].0)
        };
        Some(level * (watts / band.ref_w).sqrt())
    }

    /// The band's reference power, which ship and coast transmitters are taken to have.
    pub fn ref_watts(&self, khz: f64) -> Option<f64> {
        self.band(khz).map(|b| b.ref_w)
    }
}

/// Local hour (0-23) at the midpoint between two positions (degrees) at UTC `hour`: one hour per
/// 15 degrees of the mean longitude, taken the short way round.
pub fn midpoint_hour(a: (f64, f64), b: (f64, f64), hour: u8) -> f64 {
    let mut lon = (a.1 + b.1) / 2.0;
    if (a.1 - b.1).abs() > 180.0 {
        lon += if lon > 0.0 { -180.0 } else { 180.0 };
    }
    f64::from((i32::from(hour) + (lon / 15.0) as i32).rem_euclid(24))
}

/// ponytail: one table for the whole program (it is fixed data); per-world tables if a lesson
/// ever needs other propagation.
static ETHER: OnceLock<Ether> = OnceLock::new();

/// Uses the original propagation tables from now on.
pub fn use_ether(e: Ether) {
    let _ = ETHER.set(e);
}

/// Great-circle distance in nautical miles between two positions given in degrees.
pub fn distance_nm(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let c = la1.sin() * la2.sin() + la1.cos() * la2.cos() * (lo2 - lo1).cos();
    c.clamp(-1.0, 1.0).acos().to_degrees() * 60.0
}

/// VHF radio horizon (nm) between antennas `h1`, `h2` metres high: 2.5 (sqrt h1 + sqrt h2).
pub fn vhf_range_nm(h1_m: f64, h2_m: f64) -> f64 {
    2.5 * (h1_m.sqrt() + h2_m.sqrt())
}

/// Usable range (nm) of a transmission on `khz` at local hour `hour` (0-23, at the midpoint).
pub fn range_nm(khz: f64, hour: u8) -> f64 {
    let night = !(6..18).contains(&hour);
    match khz {
        k if k < 300.0 => 300.0,
        k if k < 600.0 => 400.0,         // NAVTEX 518 / 490 kHz
        k if k < 3000.0 => if night { 300.0 } else { 150.0 }, // MF: sea area A2
        k if k < 30_000.0 => {
            // HF sky wave: lower bands at night, higher bands by day.
            let good = if night { k < 9000.0 } else { k > 7000.0 };
            if good { 4000.0 } else { 1200.0 }
        }
        _ => vhf_range_nm(20.0, 100.0), // VHF ship to coast station
    }
}

/// Whether a station at `from` is heard at `to` on `khz` at UTC `hour` (own model: local hour).
/// A DSC (or NAVTEX) receiver with no other signal on the frequency: over twice its noise.
/// ponytail: one signal at a time; `decoded` with every signal's level if calls ever overlap.
pub fn heard(from: (f64, f64), to: (f64, f64), khz: f64, hour: u8) -> bool {
    let nm = distance_nm(from, to);
    let ether = ETHER.get().and_then(|e| e.level(khz, midpoint_hour(from, to, hour), nm, e.ref_watts(khz)?));
    match ether {
        Some(level) => decoded(&[level], noise(dsc_bandwidth_hz(khz))).is_some(),
        None => nm <= range_nm(khz, hour),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert!((vhf_range_nm(16.0, 100.0) - 35.0).abs() < 0.01);
        assert!((distance_nm((0.0, 0.0), (0.0, 1.0)) - 60.0).abs() < 0.01);
        let haifa = (32.8, 35.0);
        assert!(heard(haifa, (32.9, 34.7), 156_525.0, 12));
        assert!(!heard(haifa, (34.5, 33.0), 156_525.0, 12)); // Cyprus area: out of VHF range
        assert!(heard(haifa, (34.5, 33.0), 518.0, 12));
        assert!(range_nm(2187.5, 23) > range_nm(2187.5, 12));
    }

    #[test]
    fn ether_tables() {
        let e = Ether::parse(
            "Frequency  60.0 300.0  25  30 10.0\n  Latitude 50.0\n    Month 1\n       Hour 0  28 5 33 0 end\n       Hour 12  28 3 30 0 end\n    end\n  end\nend\n\nFrequency  0.4 1.5  250  550 4\nend\n",
        )
        .unwrap();
        assert_eq!(e.bands.len(), 2);
        let vhf = 156_525.0;
        let level = |hour: f64, nm: f64| e.level(vhf, hour, nm, 25.0).unwrap();
        // Spreading and absorption meet the first point; then straight to the last; then nothing.
        assert!((level(0.0, 28.0) - 5.0).abs() < 1e-9);
        assert!(level(0.0, 10.0) > level(0.0, 20.0) && level(0.0, 20.0) > 5.0);
        assert!((level(0.0, 0.0) - 300.0 * (-e.bands[0].rows[0].k * 0.5).exp() / 0.5).abs() < 1e-9);
        assert!((level(0.0, 30.5) - 2.5).abs() < 1e-9);
        assert_eq!(level(0.0, 34.0), 0.0);
        // Nearest hour, not interpolated: 5 h is the 0 h row, 7 h the 12 h row, 20 h the 0 h row.
        assert!(level(5.0, 31.0) > 0.0 && level(7.0, 31.0) == 0.0 && level(20.0, 31.0) > 0.0);
        // Four times the power, twice the level.
        assert!((e.level(vhf, 0.0, 28.0, 100.0).unwrap() - 10.0).abs() < 1e-9);
        // A band without rows, and a frequency outside the file.
        assert_eq!(e.level(518.0, 5.0, 400.0, 250.0), Some(100.0));
        assert_eq!(e.level(5000.0, 0.0, 1.0, 1.0), None);
    }

    #[test]
    fn noise_and_capture() {
        let n = noise(dsc_bandwidth_hz(156_525.0));
        assert!((n - 0.35).abs() < 1e-9);
        // Alone: twice the noise.
        assert_eq!(decoded(&[0.71], n), Some(0));
        assert_eq!(decoded(&[0.69], n), None);
        // Two calls at once: the stronger is read only when it is about twice the other.
        assert_eq!(decoded(&[3.0, 10.0], n), Some(1));
        assert_eq!(decoded(&[6.0, 8.0], n), None);
        assert_eq!(decoded(&[], n), None);
    }

    #[test]
    fn local_hour_at_the_midpoint() {
        assert_eq!(midpoint_hour((0.0, 30.0), (0.0, 60.0), 12), 15.0);
        assert_eq!(midpoint_hour((0.0, -100.0), (0.0, -50.0), 3), 22.0);
        // Across the date line: mean longitude 180, not 0.
        assert_eq!(midpoint_hour((0.0, 170.0), (0.0, -170.0), 0), 12.0);
    }
}

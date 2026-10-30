//! Stations around the ship when no instructor is connected: a VHF DSC coast station that
//! acknowledges distress alerts and answers individual calls.

use crate::dsc::{Call, Eos, Format, Telecommand};

pub struct CoastDsc {
    pub name: String,
    pub mmsi: String,
    /// Position (degrees north, east).
    pub pos: (f64, f64),
}

pub struct World {
    pub coast: CoastDsc,
    /// Calls on their way to the ship: (arrival time, call).
    pending: Vec<(f64, Call)>,
    random: u32,
    /// Everything the ship transmitted, for the network to relay (taken by the app).
    pub sent: Vec<Call>,
}

impl World {
    pub fn new(coast: CoastDsc) -> World {
        World { coast, pending: vec![], random: 0x9e37_79b9, sent: vec![] }
    }

    fn rand01(&mut self) -> f64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        f64::from(self.random % 1000) / 1000.0
    }

    /// A call from elsewhere (an exercise) reaching the ship at `at`.
    pub fn inject(&mut self, call: Call, at: f64) {
        self.pending.push((at, call));
    }

    /// The ship at `ship` transmitted `call` at `now` (UTC `hour`); the coast station only answers
    /// what it can hear.
    pub fn transmit(&mut self, call: &Call, now: f64, ship: (f64, f64), hour: u8) {
        self.sent.push(call.clone());
        let khz = call.freq_khz.unwrap_or(156_525.0);
        if !crate::propagation::heard(ship, self.coast.pos, khz, hour) {
            return;
        }
        let already_acked = |w: &World| w.pending.iter().any(|(_, c)| c.distress_ship.as_deref() == Some(call.from.as_str()));
        match call.format {
            Format::Distress if !already_acked(self) => {
                let delay = 20.0 + self.rand01() * 40.0;
                self.pending.push((now + delay, Call::distress_ack(&self.coast.mmsi, call)));
            }
            Format::Individual if call.to.as_deref() == Some(self.coast.mmsi.as_str()) && call.eos == Eos::AckRequested => {
                let delay = 5.0 + self.rand01() * 10.0;
                let mut ack = Call::ack(&self.coast.mmsi, call);
                if call.tc1 == Telecommand::Test {
                    ack.channel = None;
                }
                self.pending.push((now + delay, ack));
            }
            _ => {}
        }
    }

    /// Calls arriving at the ship by `now`.
    pub fn tick(&mut self, now: f64) -> Vec<Call> {
        self.tick_where(now, |_| true)
    }

    /// Calls arriving by `now` on one band: MF/HF calls carry a frequency, VHF calls do not.
    pub fn tick_band(&mut self, now: f64, mf_hf: bool) -> Vec<Call> {
        self.tick_where(now, |c| c.freq_khz.is_some() == mf_hf)
    }

    fn tick_where(&mut self, now: f64, wanted: impl Fn(&Call) -> bool) -> Vec<Call> {
        let (due, later): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|(t, c)| *t <= now && wanted(c));
        self.pending = later;
        due.into_iter().map(|(_, c)| c).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsc::Nature;

    #[test]
    fn coast_station_acknowledges() {
        let mut w = World::new(CoastDsc { name: "COAST RADIO".into(), mmsi: "002191000".into(), pos: (32.8, 35.0) });
        let alert = Call::distress("428123000", Nature::Fire, None, None);
        let ship = (32.9, 34.7);
        w.transmit(&alert, 0.0, ship, 12);
        w.transmit(&alert, 1.0, ship, 12);
        w.transmit(&alert, 2.0, (40.0, 20.0), 12); // far away: not heard // a repetition does not cause a second acknowledgement
        assert!(w.tick(19.0).is_empty());
        let acks = w.tick(61.0);
        assert_eq!(acks.len(), 1);
        assert_eq!((acks[0].tc1, acks[0].distress_ship.as_deref()), (Telecommand::DistressAck, Some("428123000")));
    }
}

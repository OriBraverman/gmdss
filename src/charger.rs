//! Reserve source of energy: a 24 V lead-acid battery and its charger ("N2174" type) with the
//! remote control unit (ammeter, voltmeter, OFF / MAN / AUTO switch).
//!
//! AUTO charges with constant current until the upper voltage, then trickle charges; when the
//! load pulls the voltage below the lower level it returns to normal charge. MAN charges with
//! up to ~30 A limited to 29 V and needs supervision. The battery is discharged at ~22 V.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Off,
    Manual,
    Auto,
}

#[derive(Debug, Clone)]
pub struct Charger {
    pub switch: Switch,
    /// Ship's mains present.
    pub mains: bool,
    /// Battery state of charge 0-1.
    pub charge: f64,
    pub capacity_ah: f64,
    /// Current drawn by the radio equipment (A).
    pub load_a: f64,
    trickle: bool,
}

const UPPER_V: f64 = 28.6;
const LOWER_V: f64 = 26.4;
const BULK_A: f64 = 25.0;
const TRICKLE_A: f64 = 1.5;
const MANUAL_A: f64 = 30.0;

impl Default for Charger {
    fn default() -> Self {
        Charger { switch: Switch::Auto, mains: true, charge: 0.85, capacity_ah: 210.0, load_a: 0.0, trickle: false }
    }
}

impl Charger {
    /// Open-circuit voltage of the battery (23.0 V empty to 25.6 V full).
    fn rest_voltage(&self) -> f64 {
        23.0 + 2.6 * self.charge
    }

    /// Terminal voltage with current `i` (A, + = charging): internal resistance 0.02 ohm, and a
    /// charging rise that grows steeply near full charge (gassing).
    fn voltage_at(&self, i: f64) -> f64 {
        let rise = if i > 0.0 { 3.0 * self.charge.powi(4) * (i / 5.0).min(1.0) } else { 0.0 };
        (self.rest_voltage() + 0.02 * i + rise).clamp(18.0, 30.0)
    }

    /// Charger output current (A).
    pub fn charging_a(&self) -> f64 {
        if !self.mains {
            return 0.0;
        }
        match self.switch {
            Switch::Off => 0.0,
            Switch::Manual if self.voltage_at(MANUAL_A - self.load_a) < 29.0 => MANUAL_A,
            Switch::Manual => 0.0,
            Switch::Auto if self.trickle => TRICKLE_A,
            Switch::Auto => BULK_A,
        }
    }

    /// Ammeter: charging (+) or discharging (-) battery current.
    pub fn battery_a(&self) -> f64 {
        self.charging_a() - self.load_a
    }

    /// Voltmeter: battery terminal voltage.
    pub fn voltage(&self) -> f64 {
        self.voltage_at(self.battery_a())
    }

    pub fn tick(&mut self, dt: f64) {
        let ah = self.battery_a() * dt / 3600.0;
        self.charge = (self.charge + ah / self.capacity_ah).clamp(0.0, 1.0);
        if self.switch == Switch::Auto {
            let v = self.voltage();
            if v >= UPPER_V {
                self.trickle = true;
            } else if v < LOWER_V {
                self.trickle = false;
            }
        }
    }

    /// The equipment can be powered.
    pub fn power_available(&self) -> bool {
        self.mains || self.voltage() > 21.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_mode_switches_to_trickle_and_back() {
        let mut c = Charger { charge: 0.5, ..Default::default() };
        assert_eq!(c.charging_a(), BULK_A);
        for _ in 0..300 {
            c.tick(60.0);
        }
        assert_eq!(c.charging_a(), TRICKLE_A);
        c.mains = false;
        c.load_a = 30.0;
        for _ in 0..300 {
            c.tick(60.0);
        }
        assert!(c.voltage() < 24.0 && c.battery_a() < 0.0);
        c.mains = true;
        c.tick(1.0);
        assert_eq!(c.charging_a(), BULK_A);
    }
}

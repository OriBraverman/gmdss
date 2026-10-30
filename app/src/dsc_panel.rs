//! The DSC controller front panel (same unit for VHF and MF/HF).

use crate::panel::{Plate, LCD_INK};
use crate::svg_art::Art;
use eframe::egui::{self, Color32};
use gmdss::dsc::{Call, Controller, Eos, Format, Key as K, Telecommand};

/// The unit's artwork with its model name (`VHF DSC RM2042`, ...) on the title band.
pub fn art(name: &'static str, title: &str) -> Art {
    Art::new(name, &include_str!("../assets/panels/dsc.svg").replace("{TITLE}", title))
}

/// Draws the unit; returns the calls its keys asked to transmit.
pub fn show(ui: &mut egui::Ui, scope: &str, art: &mut Art, ctl: &mut Controller, vol: &mut u8, width: f32) -> Vec<Call> {
    let p = Plate::with_art(ui, scope, art, width);
    let mut out = vec![];
    if p.knob_spot(ui, "VOL_OFF", art.rect("knob-VOL_OFF"), vol, 10) && *vol == 0 {
        ctl.alarm = false;
    }
    let on = *vol > 0;
    // LCD: lit while on; an alarm makes the backlight flash.
    let glass = p.place(art.rect("lcd"));
    let painter = ui.painter().clone();
    if on {
        let flash = ctl.alarm && (ui.input(|i| i.time) % 0.6) < 0.3;
        painter.rect_filled(glass, 0.0, Color32::from_rgba_unmultiplied(235, 245, 150, if flash { 110 } else { 40 }));
        if ctl.alarm {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let h = glass.height() / 2.0;
        // Monospace glyphs are about 0.6 em wide: fit 24 columns.
        let size = (h * 0.62).min((glass.width() - 8.0 * p.rect.width() / 471.0) / (24.0 * 0.6));
        for (i, line) in ctl.display().iter().enumerate() {
            painter.text(glass.left_top() + egui::vec2(4.0 * p.rect.width() / 471.0, h * (i as f32 + 0.5)), egui::Align2::LEFT_CENTER, line, egui::FontId::monospace(size), LCD_INK);
        }
    }
    let keys = [
        ("FUNC", K::Func), ("RX_MSG", K::RxMsg), ("EDIT", K::Edit), ("LAST", K::Last), ("Up", K::Up), ("NEXT", K::Next),
        ("DISTRESS", K::Distress), ("CALL", K::Call), ("SEND", K::Send), ("Left", K::Left), ("Down", K::Down), ("Right", K::Right),
        ("1", K::Digit(1)), ("2", K::Digit(2)), ("3", K::Digit(3)), ("4", K::Digit(4)), ("5", K::Digit(5)),
        ("6", K::Digit(6)), ("7", K::Digit(7)), ("8", K::Digit(8)), ("9", K::Digit(9)), ("0", K::Digit(0)),
        ("Dot", K::Dot), ("STOP_ENT", K::StopEnt),
    ];
    for (name, key) in keys {
        if p.spot(ui, name, art.rect(&format!("key-{name}"))).0 && on {
            out.extend(ctl.press(key));
        }
    }
    out
}

/// One line for the radio log.
pub fn describe(c: &Call) -> String {
    let at = c.freq_khz.map_or(String::new(), |f| format!(" on {f:.1} kHz"));
    let what = match (c.format, c.tc1, c.eos) {
        (Format::Distress, ..) => format!("DISTRESS ALERT from {} ({})", c.from, c.nature.map_or("", |n| n.name())),
        (_, Telecommand::DistressAck, _) => format!("distress acknowledgement from {} for {}", c.from, c.distress_ship.as_deref().unwrap_or("?")),
        (_, _, Eos::Ack) => format!(
            "acknowledgement from {}{}",
            c.from,
            match (c.channel, c.freq_khz) {
                (Some(ch), _) => format!(", work channel {ch}"),
                _ => String::new(),
            }
        ),
        _ => format!("{} call from {} to {}", c.category.name(), c.from, c.to.as_deref().unwrap_or("all ships")),
    };
    what + &at
}

//! Equipment front panels: SVG artwork (`svg_art`) with the controls, lamps and read-outs on it,
//! and the lesson hooks every control goes through.
//! Coordinates are the SVG units of each panel's artwork, scaled to the space given.

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

// Colours measured from the original panels (VGA palette values).
pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xeb, 0x00);
pub const GREEN: Color32 = Color32::from_rgb(0x00, 0xeb, 0xb2);
pub const RED: Color32 = Color32::from_rgb(0xdf, 0x00, 0x00);
pub const LCD_INK: Color32 = Color32::from_rgb(28, 32, 18);

/// Between the lesson player and the controls, kept in egui memory for the frame.
#[derive(Clone, Default)]
pub struct Bus {
    /// Control the demonstration presses (lesson name), and for how many frames it waited.
    pub press: Option<String>,
    pub press_age: u32,
    /// Control the demonstration cursor is on.
    pub point: Option<String>,
    /// Control the demonstration pressed and holds down until its `release` (`scope.name`).
    pub held: Option<String>,
    /// Device selected last: lesson names without `scope.` are looked up there first.
    pub scope: String,
    /// Controls the student used, as `scope.name`.
    pub used: Vec<String>,
}

pub fn bus<R>(ctx: &egui::Context, f: impl FnOnce(&mut Bus) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<Bus>(egui::Id::new("lesson_bus"))))
}

/// Names the original's lessons use for controls that are called otherwise here:
/// (scope, original name, name here).
const ALIASES: [(&str, &str, &str); 8] = [
    ("VHF", "Power", "VOL_OFF"),
    ("R_501", "OFF", "AF_GAIN"),
    ("SART", "Key", "TEST"),
    ("SART", "OFF", "switch"),
    ("SART", "ON", "switch"),
    ("Navtex", "Monitor", "Up"),
    ("Navtex", "Dimmer", "Down"),
    ("Inmarsat_A", "On_Off", "POWER"),
];

/// Whether lesson name `want` means control `scope.name` now. ponytail: a name without scope must
/// match the selected device in the first frame, any device after that.
fn means(b: &Bus, want: &str, scope: &str, name: &str) -> bool {
    let named = gmdss::script::names(want, &format!("{scope}.{name}"))
        || ALIASES.iter().any(|&(s, alias, n)| s == scope && n == name && gmdss::script::names(want, &format!("{s}.{alias}")));
    named && (want.contains('.') || b.press_age > 0 || b.scope.eq_ignore_ascii_case(scope))
}

/// Lesson hooks for one control: draws the demonstration ring, reports a student click, and
/// returns true when the demonstration presses it.
pub fn control(painter: &egui::Painter, rect: Rect, scope: &str, name: &str, clicked: bool) -> bool {
    // Paint outside `bus`: painting while egui memory is locked would deadlock.
    let (ring, pressed) = bus(painter.ctx(), |b| {
        if clicked {
            b.used.push(format!("{scope}.{name}"));
        }
        let pressed = b.press.as_deref().is_some_and(|p| means(b, p, scope, name));
        if pressed {
            b.press = None;
            b.held = Some(format!("{scope}.{name}"));
        }
        (b.point.as_deref().is_some_and(|p| means(b, p, scope, name)), pressed)
    });
    if ring {
        painter.rect_stroke(rect.expand(4.0), CornerRadius::same(6), Stroke::new(3.0, YELLOW), StrokeKind::Outside);
    }
    pressed
}

/// Lesson hooks for a device in the equipment list, called by any of `names` (its units): true
/// when it is selected by the student or the demonstration; the unit named becomes the scope.
pub fn device(ui: &Ui, rect: Rect, names: &[&str], clicked: bool) -> bool {
    let named = |p: &Option<String>| p.as_deref().and_then(|p| names.iter().find(|n| gmdss::script::names(p, n)).copied());
    let (ring, demo) = bus(ui.ctx(), |b| {
        let demo = named(&b.press);
        if let Some(n) = demo {
            b.scope = n.to_string();
            b.press = None;
        } else if clicked {
            b.scope = names[0].to_string();
            b.used.push(names[0].to_string());
        }
        (named(&b.point).is_some(), demo.is_some())
    });
    if ring {
        ui.painter().rect_stroke(rect.expand(3.0), CornerRadius::same(4), Stroke::new(3.0, YELLOW), StrokeKind::Outside);
    }
    demo || clicked
}

/// Whether a key is held down now, by the student or by the demonstration (press ... release).
pub fn down(resp: &Response, scope: &str, name: &str) -> bool {
    let full = format!("{scope}.{name}");
    resp.is_pointer_button_down_on() || bus(&resp.ctx, |b| b.held.as_deref() == Some(full.as_str()))
}

/// Lesson hooks for an ordinary egui button: true when clicked or pressed by a demonstration.
pub fn hook(resp: &Response, scope: &str, name: &str) -> bool {
    control(&resp.ctx.layer_painter(resp.layer_id), resp.rect, scope, name, resp.clicked()) || resp.clicked()
}

/// Width for `n` stacked plates of height/width `aspect` (plus `extra` points of other rows) to fit
/// the visible height, at most `max`.
pub fn fit_width(ui: &Ui, aspect: f32, n: f32, extra: f32, max: f32) -> f32 {
    let visible = ui.clip_rect().bottom() - ui.cursor().top();
    ((visible - extra) / n / aspect).clamp(300.0, max)
}

/// A plate of `w` x `h` panel units, drawn at the width available (max `max_w` pixels).
pub struct Plate {
    pub rect: Rect,
    scale: f32,
    /// Lesson scope of the controls on it (`VHF`, `VHF_DSC`, ...).
    scope: String,
}

impl Plate {
    pub fn pos(&self, x: f32, y: f32) -> Pos2 {
        self.rect.min + Vec2::new(x, y) * self.scale
    }

    pub fn area(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.pos(x, y), Vec2::new(w, h) * self.scale)
    }

    pub fn font(&self, size: f32) -> FontId {
        FontId::proportional(size * self.scale)
    }

    pub fn mono(&self, size: f32) -> FontId {
        FontId::monospace(size * self.scale)
    }

    /// Knob behaviour on `r`: drag up/down or scroll to turn; a click switches on/off, or with
    /// `thirds` a click on the right / left third turns one step. Lesson names: `name` (on/off),
    /// `name_right`, `name_left`. Returns true if `value` changed.
    fn turn(&self, ui: &mut Ui, name: &str, r: Rect, value: &mut u8, max: u8, thirds: bool) -> bool {
        let resp = ui.interact(r, ui.id().with(&self.scope).with(name), Sense::click_and_drag());
        let zone = resp.interact_pointer_pos().map_or(0.5, |p| (p.x - r.left()) / r.width());
        let click = |z: bool| resp.clicked() && z;
        let (mid, right_click, left_click) = if thirds { (click((0.38..=0.62).contains(&zone)), click(zone > 0.62), click(zone < 0.38)) } else { (resp.clicked(), false, false) };
        let mut v = f32::from(*value);
        if control(ui.painter(), r, &self.scope, name, mid) || mid {
            // Switch: off -> middle, on -> off.
            v = if *value == 0 { f32::from(max / 2) } else { 0.0 };
        }
        if resp.dragged() {
            v -= resp.drag_delta().y / 8.0;
        }
        if resp.hovered() {
            v += ui.input(|i| i.smooth_scroll_delta.y) / 30.0;
        }
        let turned = v.round() as i32 - i32::from(*value);
        let (right, left) = (format!("{name}_right"), format!("{name}_left"));
        if control(ui.painter(), r, &self.scope, &right, right_click || turned > 0 && !mid) || right_click {
            v += 1.0;
        }
        if control(ui.painter(), r, &self.scope, &left, left_click || turned < 0 && !mid) || left_click {
            v -= 1.0;
        }
        let new = v.round().clamp(0.0, f32::from(max)) as u8;
        let changed = new != *value;
        *value = new;
        resp.on_hover_text(format!("{name}: {new}/{max}. Click: on/off{}. Drag or scroll to turn", if thirds { ", right/left side: one step" } else { "" }));
        changed
    }

    /// A plate whose static parts are SVG artwork; the code adds the controls and moving parts.
    pub fn with_art(ui: &mut Ui, scope: &str, art: &mut crate::svg_art::Art, max_w: f32) -> Plate {
        let size = art.size();
        let width = ui.available_width().min(max_w);
        let scale = width / size.x;
        let (rect, _) = ui.allocate_exact_size(size * scale, Sense::hover());
        let px = (width * ui.ctx().pixels_per_point()).round() as u32;
        let texture = art.texture(ui.ctx(), px);
        ui.painter().image(texture.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        Plate { rect, scale, scope: scope.to_string() }
    }

    /// An artwork rectangle (SVG units) on screen.
    pub fn place(&self, r: Rect) -> Rect {
        self.area(r.min.x, r.min.y, r.width(), r.height())
    }

    /// A key the artwork draws at `r` (SVG units): pressed look and lesson hooks. True when
    /// clicked or pressed by a demonstration.
    pub fn spot(&self, ui: &mut Ui, name: &str, r: Rect) -> (bool, Response) {
        let r = self.place(r);
        let resp = ui.interact(r, ui.id().with(&self.scope).with(name), Sense::click());
        let demo = control(ui.painter(), r, &self.scope, name, resp.clicked());
        if demo || down(&resp, &self.scope, name) {
            ui.painter().rect_filled(r.shrink(self.scale), CornerRadius::same((3.0 * self.scale) as u8), Color32::from_white_alpha(45));
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        (resp.clicked() || demo, resp)
    }

    /// A knob key the artwork draws at `r` (SVG units), see `turn`.
    pub fn knob_spot(&self, ui: &mut Ui, name: &str, r: Rect, value: &mut u8, max: u8) -> bool {
        self.turn(ui, name, self.place(r), value, max, true)
    }

    /// A lamp the artwork draws at `r` (SVG units), lit in `colour` when `on`; `tip` on hover.
    pub fn lamp_glow(&self, ui: &mut Ui, r: Rect, on: bool, colour: Color32, tip: &str) {
        let r = self.place(r);
        let p = ui.painter();
        let c = r.center();
        let rad = r.width().min(r.height()) * 0.42;
        if on {
            p.circle_filled(c, rad * 1.9, colour.gamma_multiply(0.25));
            p.circle_filled(c, rad, colour);
            p.circle_filled(c - Vec2::splat(rad * 0.3), rad * 0.35, Color32::from_white_alpha(150));
        }
        ui.interact(r, ui.id().with(&self.scope).with(tip), Sense::hover()).on_hover_text(tip);
    }

    /// Pointer line on a round knob the artwork draws at `r` (SVG units), at `value` of 0..=max.
    pub fn knob_pointer(&self, ui: &Ui, r: Rect, value: u8, max: u8) {
        let r = self.place(r);
        let a = std::f32::consts::PI * (0.75 + 1.5 * f32::from(value) / f32::from(max.max(1)));
        let rad = r.width() / 2.0;
        ui.painter().line_segment([r.center() + Vec2::angled(a) * rad * 0.35, r.center() + Vec2::angled(a) * rad * 0.9], Stroke::new(2.0 * self.scale, Color32::WHITE));
    }
}

//! Tutor: lessons in the original tutor's script structure (`gmdss::script`). Demonstration plays
//! the lesson on the equipment, practice lets the student do it with the messages shown, the exam
//! is timed and shows only the tasks.

use crate::panel::bus;
use eframe::egui::{self, Color32, RichText};
use gmdss::script::{self, Entry, Input, Key, Mode, Out, Player, Script};

const INDEX: &str = include_str!("../../lessons/index.txt");
pub const LESSONS: [&str; 13] = [
    include_str!("../../lessons/01_vhf_distress.txt"),
    include_str!("../../lessons/02_hf_telex.txt"),
    include_str!("../../lessons/03_navtex.txt"),
    include_str!("../../lessons/04_inmarsat_c.txt"),
    include_str!("../../lessons/05_vhf_dsc_call.txt"),
    include_str!("../../lessons/06_mf_distress.txt"),
    include_str!("../../lessons/07_r501.txt"),
    include_str!("../../lessons/08_sart.txt"),
    include_str!("../../lessons/09_epirb.txt"),
    include_str!("../../lessons/10_charger.txt"),
    include_str!("../../lessons/11_inmarsat_c_distress.txt"),
    include_str!("../../lessons/12_hf_alarm_test.txt"),
    include_str!("../../lessons/13_portable.txt"),
];

pub struct Run {
    player: Player,
    task: String,
    msg: Option<(String, i32, i32)>,
    note: String,
    /// The help topic for this lesson (file, topic name).
    help: Option<(String, String)>,
    /// Further controls of an `A+B` press, pressed one per frame.
    queue: Vec<String>,
}

impl Run {
    #[cfg(test)]
    pub fn finished(&self) -> bool {
        self.player.finished()
    }
}

pub struct Tutor {
    index: Vec<Entry>,
    lessons: Vec<Script>,
    pub run: Option<Run>,
    results: Vec<String>,
    /// Controls a demonstration tried to press that were not on screen (a lesson error).
    pub missing: Vec<String>,
    /// A help topic the student asked to see (file, topic name), for the Books page.
    pub help_request: Option<(String, String)>,
}

/// Lesson key names <-> egui keys.
const KEYS: [(&str, egui::Key); 12] = [
    ("Enter", egui::Key::Enter),
    ("Esc", egui::Key::Escape),
    ("Tab", egui::Key::Tab),
    ("Left", egui::Key::ArrowLeft),
    ("Right", egui::Key::ArrowRight),
    ("Up", egui::Key::ArrowUp),
    ("Down", egui::Key::ArrowDown),
    ("PgUp", egui::Key::PageUp),
    ("PgDn", egui::Key::PageDown),
    ("Backspace", egui::Key::Backspace),
    ("Del", egui::Key::Delete),
    ("Home", egui::Key::Home),
];

fn egui_key(name: &str) -> Option<egui::Key> {
    KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, k)| *k).or_else(|| egui::Key::from_name(name))
}

fn lesson_key(key: egui::Key) -> String {
    KEYS.iter().find(|(_, k)| *k == key).map_or_else(|| key.name().to_string(), |(n, _)| n.to_string())
}

impl Tutor {
    pub fn new() -> Tutor {
        let lessons = LESSONS.iter().map(|t| script::parse(t).expect("built-in lesson")).collect();
        Tutor { index: script::parse_index(INDEX), lessons, run: None, results: vec![], missing: vec![], help_request: None }
    }

    /// Adds a lesson (tests: the original's lessons, when present locally); returns its number.
    #[cfg(test)]
    pub fn push_lesson(&mut self, s: Script) -> usize {
        self.lessons.push(s);
        self.lessons.len() - 1
    }

    /// Starts lesson `i` (in `LESSONS` order).
    pub fn start(&mut self, ctx: &egui::Context, i: usize, mode: Mode, now: f64) {
        let help = self.index.iter().find(|e| e.title == self.lessons[i].title).and_then(|e| e.help.clone());
        self.run = Some(Run { player: Player::new(self.lessons[i].clone(), mode, now), task: String::new(), msg: None, note: String::new(), help, queue: vec![] });
        bus(ctx, |b| *b = Default::default());
    }

    /// Before the equipment is drawn: runs the lesson and hands its actions to the controls.
    pub fn before(&mut self, ctx: &egui::Context, now: f64) {
        let Some(run) = &mut self.run else { return };
        if !run.queue.is_empty() && bus(ctx, |b| b.press.is_none()) {
            let next = run.queue.remove(0);
            bus(ctx, |b| {
                b.press = Some(next);
                b.press_age = 0;
            });
        }
        let outs = run.player.tick(now);
        self.apply(ctx, outs);
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    /// After the equipment is drawn: what the student did goes to the lesson.
    pub fn after(&mut self, ctx: &egui::Context, now: f64) {
        let (used, lost) = bus(ctx, |b| {
            let mut lost = None;
            if b.press.is_some() {
                b.press_age += 1;
                if b.press_age > 3 {
                    lost = b.press.take(); // no such control on screen
                }
            }
            (std::mem::take(&mut b.used), lost)
        });
        if let Some(name) = lost {
            self.missing.push(name.clone());
            if let Some(run) = &mut self.run {
                run.note = format!("Control not on screen: {name}");
            }
        }
        let Some(run) = &mut self.run else { return };
        if run.player.mode == Mode::Demo || run.player.finished() {
            return;
        }
        let mut inputs: Vec<Input> = used.into_iter().map(Input::Press).collect();
        ctx.input(|i| {
            let m = |alt: bool| (Some(alt), Some(i.modifiers.ctrl), Some(i.modifiers.shift));
            for e in &i.events {
                match e {
                    egui::Event::Text(t) => inputs.extend(t.chars().map(|c| {
                        let (alt, ctrl, shift) = m(i.modifiers.alt);
                        Input::Key(Key { key: c.to_string(), alt, ctrl, shift })
                    })),
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        let name = lesson_key(*key);
                        if name.chars().count() > 1 {
                            inputs.push(Input::Key(Key { key: name, alt: Some(modifiers.alt), ctrl: Some(modifiers.ctrl), shift: Some(modifiers.shift) }));
                        }
                    }
                    _ => {}
                }
            }
        });
        let mut outs = vec![];
        for input in inputs {
            outs.extend(run.player.input(input, now));
        }
        self.apply(ctx, outs);
    }

    fn apply(&mut self, ctx: &egui::Context, outs: Vec<Out>) {
        let Some(run) = &mut self.run else { return };
        for out in outs {
            match out {
                Out::Point(x) => bus(ctx, |b| b.point = Some(x)),
                Out::Press(x) => {
                    let mut parts: Vec<String> = x.split('+').map(str::to_string).collect();
                    let first = parts.remove(0);
                    run.queue = parts;
                    bus(ctx, |b| {
                        b.point = Some(first.clone());
                        b.press = Some(first);
                        b.press_age = 0;
                    });
                }
                Out::Release => bus(ctx, |b| {
                    b.point = None;
                    b.held = None;
                }),
                Out::Key(k) => ctx.input_mut(|i| match egui_key(&k.key) {
                    Some(key) if k.key.chars().count() > 1 => {
                        let modifiers = egui::Modifiers { alt: k.alt == Some(true), ctrl: k.ctrl == Some(true), shift: k.shift == Some(true), ..Default::default() };
                        i.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                    }
                    _ => i.events.push(egui::Event::Text(k.key)),
                }),
                Out::Msg { text, x, y } => run.msg = Some((text, x, y)),
                Out::CloseMsg => run.msg = None,
                Out::Task(t) => run.task = t,
                Out::Help { file, topic } => run.help = Some((file, topic)),
                Out::Wrong(x) => run.note = format!("Not expected: {x}"),
                Out::Finished { completed, errors, secs } => {
                    let mode = match run.player.mode {
                        Mode::Demo => "demonstration",
                        Mode::Practice => "practice",
                        Mode::Exam => "exam",
                    };
                    let how = if completed { "completed" } else { "time is up" };
                    self.results.push(format!("{}: {mode} {how}, {errors} errors, {:.1} min", run.player.title(), secs / 60.0));
                    run.msg = None;
                    bus(ctx, |b| {
                        b.point = None;
                        b.held = None;
                    });
                }
            }
        }
    }

    /// The task strip shown above every view while a lesson runs, and the message box.
    pub fn strip(&mut self, ui: &mut egui::Ui, now: f64) {
        let Some(run) = &mut self.run else { return };
        let mut stop = false;
        egui::Frame::new().fill(Color32::from_rgb(60, 50, 20)).inner_margin(6).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(run.player.title()).strong().color(Color32::from_rgb(255, 220, 120)));
                if let Some(left) = run.player.time_left(now) {
                    ui.label(RichText::new(format!("{:02}:{:02}", left as u32 / 60, left as u32 % 60)).monospace().color(Color32::WHITE));
                }
                if run.player.mode != Mode::Demo {
                    ui.label(RichText::new(format!("errors {}", run.player.errors)).color(Color32::LIGHT_GRAY));
                }
                let task = if run.player.finished() { "Finished - see the results on the Tutor page" } else { run.task.as_str() };
                ui.label(RichText::new(task).color(Color32::WHITE));
                if !run.note.is_empty() {
                    ui.label(RichText::new(&run.note).italics().color(Color32::from_rgb(255, 160, 140)));
                }
                if run.help.is_some() && ui.button("Help").clicked() {
                    self.help_request = run.help.clone();
                }
                if ui.button(if run.player.finished() { "Close" } else { "Stop lesson" }).clicked() {
                    stop = true;
                }
            });
        });
        if let Some((text, x, y)) = &run.msg {
            // Message positions are in the original 640 x 480 screen.
            let screen = ui.ctx().content_rect();
            let pos = screen.min + egui::vec2(screen.width() * (*x as f32 / 640.0).clamp(0.0, 0.7), 90.0 + screen.height() * (*y as f32 / 480.0).clamp(0.0, 0.7));
            egui::Area::new(egui::Id::new("lesson_msg")).fixed_pos(pos).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::from_rgb(250, 245, 210)).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.label(RichText::new(text).color(Color32::BLACK).size(14.0));
                });
            });
        }
        if stop {
            self.run = None;
            bus(ui.ctx(), |b| *b = Default::default());
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.label(RichText::new("LESSONS").small().color(Color32::GRAY));
        let mut group = "";
        let mut start = None;
        for e in &self.index {
            if e.group != group {
                group = &e.group;
                ui.add_space(6.0);
                ui.label(RichText::new(group).strong());
            }
            let Some(i) = self.lessons.iter().position(|l| l.title == e.title) else { continue };
            ui.horizontal(|ui| {
                ui.label(RichText::new(&e.title).size(15.0));
                for (label, mode) in [("Demonstration", Mode::Demo), ("Practice", Mode::Practice), ("Exam", Mode::Exam)] {
                    if ui.button(label).clicked() {
                        start = Some((i, mode));
                    }
                }
                if e.help.is_some() && ui.button("Help").clicked() {
                    self.help_request = e.help.clone();
                }
            });
        }
        if let Some((i, mode)) = start {
            self.start(ui.ctx(), i, mode, now);
        }
        ui.add_space(12.0);
        ui.label(RichText::new("RESULTS").small().color(Color32::GRAY));
        if self.results.is_empty() {
            ui.label("No lessons done yet.");
        }
        for r in &self.results {
            ui.label(r);
        }
        ui.add_space(12.0);
        ui.label(RichText::new("Start a lesson, then go to Units View: the task stays at the top of the screen.").small().color(Color32::GRAY));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn built_in_lessons_parse_and_are_indexed() {
        let index = gmdss::script::parse_index(super::INDEX);
        for text in super::LESSONS {
            let lesson = gmdss::script::parse(text).unwrap();
            assert!(index.iter().any(|e| e.title == lesson.title), "{} not in index", lesson.title);
        }
    }
}

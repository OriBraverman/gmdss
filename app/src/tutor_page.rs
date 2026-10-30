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
    /// The examinee whose exam table records this run.
    examinee: Option<usize>,
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
    /// Lesson files from `load_dir` that could not be read.
    unreadable: Vec<String>,
    /// A lesson being recorded from what the instructor does.
    recording: Option<Recording>,
    /// Where recorded lessons are saved.
    pub record_dir: std::path::PathBuf,
    examinees: Vec<Examinee>,
    examinee: Option<usize>,
    new_name: String,
    confirm_delete: bool,
    /// Where the examinees' tables are kept, one file each.
    pub exam_dir: std::path::PathBuf,
}

/// An examinee's exam table, as on the original (BX_TU_EL, BX_TU_ER): for each lesson the test
/// (practice) attempts, the errors in the last one, the exam attempts and whether the last exam
/// was passed.
pub struct Examinee {
    name: String,
    file: std::path::PathBuf,
    rows: std::collections::BTreeMap<String, Row>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
struct Row {
    tests: u32,
    last_errors: Option<u32>,
    exams: u32,
    passed: bool,
}

impl Examinee {
    /// The file: the name, then `T TR E ER title` per lesson, tab separated.
    fn to_text(&self) -> String {
        let mut s = format!("{}\n", self.name);
        for (title, r) in &self.rows {
            let tr = r.last_errors.map_or("-".into(), |e| e.to_string());
            s += &format!("{}\t{tr}\t{}\t{}\t{title}\n", r.tests, r.exams, if r.passed { "Ok" } else { "-" });
        }
        s
    }

    fn parse(text: &str, file: std::path::PathBuf) -> Option<Examinee> {
        let mut lines = text.lines();
        let name = lines.next()?.trim().to_string();
        let mut rows = std::collections::BTreeMap::new();
        for l in lines {
            let [t, tr, e, er, title] = l.splitn(5, '\t').collect::<Vec<_>>()[..] else { continue };
            rows.insert(title.to_string(), Row { tests: t.parse().ok()?, last_errors: tr.parse().ok(), exams: e.parse().ok()?, passed: er == "Ok" });
        }
        Some(Examinee { name, file, rows })
    }

    fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.file.parent().unwrap_or(std::path::Path::new(".")))?;
        std::fs::write(&self.file, self.to_text())
    }
}

/// A lesson written from what the instructor does, as the original's recorder (TU_WR_AC): before
/// each action the time since the last one, then the control pressed or the key typed.
pub struct Recording {
    text: String,
    last: f64,
    actions: usize,
}

impl Recording {
    fn add(&mut self, now: f64, line: &str) {
        self.text.push_str(&format!("delay 0 0 {:.1}\n{line}\n", (now - self.last).max(0.0)));
        self.last = now;
        self.actions += 1;
    }
}

/// The `kbd` line for a key.
fn kbd_line(k: &Key) -> String {
    let flag = |on: Option<bool>, name: &str| if on == Some(true) { name.to_string() } else { "*".into() };
    let key = if k.key.chars().count() == 1 { format!("'{}'", k.key) } else { k.key.clone() };
    // A typed character carries its own case: Shift is not part of it.
    let shift = if k.key.chars().count() == 1 { None } else { k.shift };
    format!("kbd {key} {} {} {}", flag(k.alt, "Alt"), flag(k.ctrl, "Ctrl"), flag(shift, "Shift"))
}

/// Keys typed this frame, as lessons name them.
fn keys(ctx: &egui::Context) -> Vec<Key> {
    let mut out = vec![];
    ctx.input(|i| {
        for e in &i.events {
            match e {
                egui::Event::Text(t) => out.extend(t.chars().map(|c| Key { key: c.to_string(), alt: Some(i.modifiers.alt), ctrl: Some(i.modifiers.ctrl), shift: Some(i.modifiers.shift) })),
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    let name = lesson_key(*key);
                    if name.chars().count() > 1 {
                        out.push(Key { key: name, alt: Some(modifiers.alt), ctrl: Some(modifiers.ctrl), shift: Some(modifiers.shift) });
                    }
                }
                _ => {}
            }
        }
    });
    out
}

/// Words of a lesson title, for matching index entries to lesson files.
fn words(title: &str) -> std::collections::BTreeSet<String> {
    title.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_ascii_lowercase).collect()
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
        Tutor { index: script::parse_index(INDEX), lessons, run: None, results: vec![], missing: vec![], help_request: None, unreadable: vec![], recording: None, record_dir: "recorded_lessons".into(), examinees: vec![], examinee: None, new_name: String::new(), confirm_delete: false, exam_dir: "examinees".into() }
    }

    /// While recording: the controls used and the keys typed this frame become lesson lines.
    fn record(&mut self, now: f64, used: &[String], typed: &[Key]) {
        let Some(rec) = &mut self.recording else { return };
        for name in used {
            rec.add(now, &format!("move_and_press {name}\ndelay 0 0 0.3\nrelease"));
        }
        for k in typed {
            rec.add(now, &kbd_line(k));
        }
    }

    /// Ends the recording: the lesson is saved in `record_dir` and listed under "Recorded lessons".
    fn stop_recording(&mut self) {
        let Some(rec) = self.recording.take() else { return };
        let text = format!("{}exit\n", rec.text);
        let _ = std::fs::create_dir_all(&self.record_dir);
        let n = (1..).find(|n| !self.record_dir.join(format!("recorded_{n:02}.txt")).exists()).unwrap();
        let path = self.record_dir.join(format!("recorded_{n:02}.txt"));
        let title = format!("Recorded lesson {n:02}");
        let text = text.replacen("Recorded lesson", &title, 1);
        match std::fs::write(&path, &text).map_err(|e| e.to_string()).and_then(|_| script::parse(&text).map_err(|e| e.message)) {
            Ok(s) => {
                self.index.push(Entry { group: "Recorded lessons".into(), title: s.title.clone(), help: None });
                self.lessons.push(s);
                self.results.push(format!("{title}: {} actions saved to {}", rec.actions, path.display()));
            }
            Err(e) => self.unreadable.push(format!("{}: {e}", path.display())),
        }
    }

    /// Reads the examinees' tables from `exam_dir`.
    pub fn load_examinees(&mut self) {
        let Ok(entries) = std::fs::read_dir(&self.exam_dir) else { return };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
        paths.sort();
        for p in paths {
            match std::fs::read_to_string(&p).ok().and_then(|t| Examinee::parse(&t, p.clone())) {
                Some(e) => self.examinees.push(e),
                None => self.unreadable.push(format!("{} (exam table)", p.display())),
            }
        }
    }

    fn new_examinee(&mut self) {
        let free = |n: &usize| {
            let f = self.exam_dir.join(format!("{n}.txt"));
            !f.exists() && !self.examinees.iter().any(|e| e.file == f)
        };
        let n = (1..).find(free).unwrap();
        let e = Examinee { name: self.new_name.trim().to_string(), file: self.exam_dir.join(format!("{n}.txt")), rows: Default::default() };
        if let Err(err) = e.save() {
            self.unreadable.push(format!("{}: {err}", e.file.display()));
        }
        self.examinees.push(e);
        self.examinee = Some(self.examinees.len() - 1);
        self.new_name.clear();
    }

    fn delete_examinee(&mut self, k: usize) {
        let e = self.examinees.remove(k);
        let _ = std::fs::remove_file(&e.file);
        self.examinee = None;
        self.confirm_delete = false;
        if let Some(run) = &mut self.run {
            run.examinee = None;
        }
    }

    /// The lessons in index order, with their number in `lessons`.
    fn listed(&self) -> Vec<(usize, &Entry)> {
        self.index.iter().filter_map(|e| Some((self.lessons.iter().position(|l| l.title == e.title)?, e))).collect()
    }

    /// The exam button: a lesson the examinee has not passed, chosen at random.
    fn unpassed(&self, k: usize) -> Option<usize> {
        let open: Vec<usize> = self.listed().into_iter().filter(|(_, e)| !self.examinees[k].rows.get(&e.title).is_some_and(|r| r.passed)).map(|(i, _)| i).collect();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
        (!open.is_empty()).then(|| open[seed % open.len()])
    }

    /// Adds the lessons in `dir`: scripts in the lesson language (`*.txt`), grouped by the index
    /// found there (`SCE.TUD*`, the index format), e.g. a user's own copies of the original's
    /// lessons. Index entries go to the file whose title shares the most words with theirs.
    pub fn load_dir(&mut self, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            self.unreadable.push(format!("{} (no such folder)", dir.display()));
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt"))).collect();
        paths.sort();
        let read = |p: &std::path::Path| String::from_utf8_lossy(&std::fs::read(p).unwrap_or_default()).into_owned();
        let is_index = |p: &std::path::Path| p.file_name().is_some_and(|n| n.to_string_lossy().to_ascii_uppercase().starts_with("SCE.TUD"));
        let index = paths.iter().find(|p| is_index(p)).map(|p| script::parse_index(&read(p))).unwrap_or_default();
        let mut loaded = vec![];
        for p in paths.iter().filter(|p| !is_index(p)) {
            match script::parse(&read(p)) {
                Ok(s) => loaded.push(s),
                Err(e) => self.unreadable.push(format!("{}: line {}: {}", p.file_name().unwrap_or_default().to_string_lossy(), e.line, e.message)),
            }
        }
        let mut placed = vec![false; loaded.len()];
        for mut e in index {
            let similarity = |s: &Script| {
                let (a, b) = (words(&e.title), words(&s.title));
                a.intersection(&b).count() as f32 / a.union(&b).count().max(1) as f32
            };
            let best = (0..loaded.len()).filter(|&i| !placed[i]).max_by(|&a, &b| similarity(&loaded[a]).total_cmp(&similarity(&loaded[b])));
            if let Some(i) = best.filter(|&i| similarity(&loaded[i]) >= 0.6) {
                placed[i] = true;
                loaded[i].title = e.title.clone();
                e.group = format!("{} (loaded)", e.group);
                self.index.push(e);
            }
        }
        for (s, _) in loaded.iter().zip(&placed).filter(|(_, p)| !**p) {
            self.index.push(Entry { group: "Other loaded lessons".into(), title: s.title.clone(), help: None });
        }
        self.lessons.extend(loaded);
    }

    #[cfg(test)]
    pub fn lesson_count(&self) -> usize {
        self.lessons.len()
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
        self.run = Some(Run { player: Player::new(self.lessons[i].clone(), mode, now), task: String::new(), msg: None, note: String::new(), help, queue: vec![], examinee: None });
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
        let typed = keys(ctx);
        self.record(now, &used, &typed);
        let Some(run) = &mut self.run else { return };
        if run.player.mode == Mode::Demo || run.player.finished() {
            return;
        }
        let mut inputs: Vec<Input> = used.into_iter().map(Input::Press).collect();
        inputs.extend(typed.into_iter().map(Input::Key));
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
                    let ok = script::passed(run.player.mode, completed, errors, run.player.actions);
                    let verdict = if run.player.mode == Mode::Demo { "" } else if ok { ", passed" } else { ", not passed" };
                    self.results.push(format!("{}: {mode} {how}, {errors} errors, {:.1} min{verdict}", run.player.title(), secs / 60.0));
                    if let Some(e) = run.examinee.and_then(|k| self.examinees.get_mut(k)) {
                        let r = e.rows.entry(run.player.title().to_string()).or_default();
                        if run.player.mode == Mode::Exam {
                            r.exams += 1;
                            r.passed = ok;
                        } else {
                            r.tests += 1;
                            r.last_errors = Some(errors);
                        }
                        if let Err(err) = e.save() {
                            self.unreadable.push(format!("{}: {err}", e.file.display()));
                        }
                    }
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

    /// The examinees and the chosen one's exam table: practice (test) a lesson from its row, or
    /// take an exam in a lesson not yet passed. The runs are recorded in the table.
    fn exam_table(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.label(RichText::new("EXAMINEES").small().color(Color32::GRAY));
        ui.horizontal_wrapped(|ui| {
            for (k, e) in self.examinees.iter().enumerate() {
                if ui.selectable_label(self.examinee == Some(k), &e.name).clicked() {
                    self.examinee = (self.examinee != Some(k)).then_some(k);
                    self.confirm_delete = false;
                }
            }
            ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Name").desired_width(120.0));
            if ui.add_enabled(!self.new_name.trim().is_empty(), egui::Button::new("New")).clicked() {
                self.new_examinee();
            }
        });
        let Some(k) = self.examinee else { return };
        let mut start = None;
        ui.horizontal(|ui| {
            if self.confirm_delete {
                ui.label(format!("The exam table of {} will be deleted. OK?", self.examinees[k].name));
                if ui.button("Delete").clicked() {
                    self.delete_examinee(k);
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_delete = false;
                }
                return;
            }
            match self.unpassed(k) {
                Some(i) => {
                    if ui.button("Exam").on_hover_text("An exam in a lesson not yet passed, chosen at random").clicked() {
                        start = Some((i, Mode::Exam));
                    }
                }
                None => {
                    ui.label("Every lesson passed.");
                }
            }
            if ui.button("Delete").clicked() {
                self.confirm_delete = true;
            }
        });
        let Some(e) = self.examinees.get(k) else { return };
        egui::Grid::new("exam_table").striped(true).show(ui, |ui| {
            for h in ["Lesson", "T", "TR", "E", "ER", ""] {
                ui.label(RichText::new(h).small().color(Color32::GRAY));
            }
            ui.end_row();
            for (i, entry) in self.listed() {
                let r = e.rows.get(&entry.title).copied().unwrap_or_default();
                ui.label(&entry.title);
                ui.label(r.tests.to_string());
                ui.label(r.last_errors.map_or(String::new(), |e| e.to_string()));
                ui.label(r.exams.to_string());
                ui.label(if r.passed { "Ok" } else { "" });
                if ui.small_button("Test").clicked() {
                    start = Some((i, Mode::Practice));
                }
                ui.end_row();
            }
        });
        ui.label(RichText::new("T tests (practice), TR errors in the last test, E exams, ER the last exam passed").small().color(Color32::GRAY));
        if let Some((i, mode)) = start {
            self.start(ui.ctx(), i, mode, now);
            if let Some(run) = &mut self.run {
                run.examinee = Some(k);
            }
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
        ui.add_space(10.0);
        self.exam_table(ui, now);
        ui.add_space(10.0);
        ui.label(RichText::new("RECORD A LESSON").small().color(Color32::GRAY));
        match &self.recording {
            None => {
                if ui.button("Record").on_hover_text("Every control you use and every key you type becomes a line of a new lesson").clicked() {
                    self.recording = Some(Recording { text: "Recorded lesson\n\n".into(), last: now, actions: 0 });
                }
            }
            Some(rec) => {
                let n = rec.actions;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Recording: {n} actions. Go to Units View and work the equipment.")).color(Color32::from_rgb(255, 120, 100)));
                    if ui.button("Stop and save").clicked() {
                        self.stop_recording();
                    }
                });
            }
        }
        if !self.unreadable.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Could not read: {}", self.unreadable.join("; "))).small().color(Color32::from_rgb(255, 160, 140)));
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

    /// Recording: two controls and a key, saved as a lesson that parses and is listed.
    #[test]
    fn recording_a_lesson() {
        let dir = std::env::temp_dir().join(format!("gmdss_recorded_{}", std::process::id()));
        let mut t = super::Tutor::new();
        t.record_dir = dir.clone();
        t.recording = Some(super::Recording { text: "Recorded lesson\n\n".into(), last: 10.0, actions: 0 });
        t.record(12.0, &["VHF".into(), "VHF.VOL_OFF".into()], &[]);
        let key = |k: &str, shift| gmdss::script::Key { key: k.into(), alt: Some(false), ctrl: Some(false), shift: Some(shift) };
        t.record(13.5, &[], &[key("a", false), key("Enter", true), key(" ", false)]);
        t.stop_recording();
        let text = std::fs::read_to_string(dir.join("recorded_01.txt")).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(text.starts_with("Recorded lesson 01\n"));
        assert!(text.contains("delay 0 0 2.0\nmove_and_press VHF\n"), "{text}");
        assert!(text.contains("kbd 'a' * * *\n") && text.contains("kbd Enter * * Shift\n") && text.contains("kbd ' ' * * *\n"), "{text}");
        let s = t.lessons.last().unwrap();
        assert_eq!(s.title, "Recorded lesson 01");
        assert_eq!(t.index.last().unwrap().group, "Recorded lessons");
        assert!(t.recording.is_none() && t.unreadable.is_empty(), "{:?}", t.unreadable);
    }

    /// An examinee: a test and two exams recorded, the table saved and read back.
    #[test]
    fn exam_table() {
        let dir = std::env::temp_dir().join(format!("gmdss_examinees_{}", std::process::id()));
        let ctx = eframe::egui::Context::default();
        let mut t = super::Tutor::new();
        t.exam_dir = dir.clone();
        t.new_name = "A. Student".into();
        t.new_examinee();
        let title = t.lessons[0].title.clone();
        let finish = |t: &mut super::Tutor, mode, completed, errors| {
            t.start(&ctx, 0, mode, 0.0);
            t.run.as_mut().unwrap().examinee = Some(0);
            t.apply(&ctx, vec![gmdss::script::Out::Finished { completed, errors, secs: 60.0 }]);
        };
        finish(&mut t, super::Mode::Practice, true, 4);
        finish(&mut t, super::Mode::Exam, true, 1);
        assert!(t.unpassed(0) != Some(0));
        finish(&mut t, super::Mode::Exam, true, 3);
        let mut u = super::Tutor::new();
        u.exam_dir = dir.clone();
        u.load_examinees();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(u.examinees.len(), 1);
        assert_eq!(u.examinees[0].name, "A. Student");
        assert_eq!(u.examinees[0].rows[&title], super::Row { tests: 1, last_errors: Some(4), exams: 2, passed: false });
        assert!(t.results.last().unwrap().ends_with("not passed"), "{:?}", t.results);
    }

    /// A folder of lessons: the index entry finds its file by title words, the other file goes to
    /// "Other loaded lessons", the broken one is reported.
    #[test]
    fn lessons_from_a_folder() {
        let dir = std::env::temp_dir().join(format!("gmdss_lessons_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SCE.TUD.txt"), "NUnits\r\n.-Watch receiver\r\n*--Operating with the watch receiver|r501.hlp|501\r\n").unwrap();
        std::fs::write(dir.join("29.TUE.txt"), "operating with watch receiver\r\n\r\nmove_and_press R_501\r\nrelease\r\nexit\r\n").unwrap();
        std::fs::write(dir.join("30.TUE.txt"), "something else\r\n\r\nexit\r\n").unwrap();
        std::fs::write(dir.join("31.TUE.txt"), "broken\r\n\r\nnot_a_command 1\r\n").unwrap();
        let mut t = super::Tutor::new();
        let before = (t.index.len(), t.lessons.len());
        t.load_dir(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!((t.index.len(), t.lessons.len()), (before.0 + 2, before.1 + 2));
        let e = &t.index[before.0];
        assert_eq!((e.group.as_str(), e.title.as_str()), ("Watch receiver (loaded)", "Operating with the watch receiver"));
        assert_eq!(t.lessons[before.1].title, "Operating with the watch receiver");
        assert_eq!(e.help, Some(("r501.hlp".into(), "501".into())));
        assert_eq!(t.index[before.0 + 1].group, "Other loaded lessons");
        assert!(t.unreadable.len() == 1 && t.unreadable[0].starts_with("31.TUE.txt"), "{:?}", t.unreadable);
    }
}

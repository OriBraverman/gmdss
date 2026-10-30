//! Lesson scripts, in the structure of the original tutor's lessons: the first line is the title,
//! then one command per line, `<<Label>>` lines, and text blocks that end with `_text_end`.
//!
//! ```text
//! Switching on the VHF
//! duration 60
//! task 5 0 0
//! Task 1: switch on the VHF radiotelephone.
//! _text_end
//! msg 5 0 0
//! Turn the VOL/OFF knob.
//! _text_end
//! move_and_press VHF.VOL_OFF
//! delay 0 0 0.3
//! release
//! ```
//! Commands: `move X`, `press`, `release`, `release_no_restore`, `move_and_press X` (`A+B` = both),
//! `move_xy x y`, `kbd KEY alt ctrl shift` (`*` = any; `'a'`, `Enter`, `F3`, `Esc`, `Tab`, arrows),
//! `alt`, `msg t x y`/`task t x y` + text, `delay a b c` (the demonstration waits `c` s),
//! `duration s` (exam time limit), `help file topic`, `comment begin|end`, `init begin|end` (a
//! block that is always demonstrated, also in an exam), `goto L`, `goto_if_press X L`,
//! `goto_if_kbd KEY alt ctrl shift L`, `goto_if_alt L`, `goto_if_delay L t L2` (more than `t` s
//! since label `L` was passed: go to `L2`), `label_delay L t` (in an exam: go to `L` after `t` s),
//! `set N`, `goto_if_set N L`, `error`, `exit`.
//!
//! A demonstration plays the actions by itself. In practice and exam the student performs them:
//! each `move_and_press`/`press`/`kbd` waits for that action, `goto_if_*` lines before it are the
//! accepted alternatives, anything else counts as an error. The exam shows tasks but not messages.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    /// `a`/`A` for a character, otherwise a key name such as `Enter`, `F3`, `Left`.
    pub key: String,
    /// `None` = don't care.
    pub alt: Option<bool>,
    pub ctrl: Option<bool>,
    pub shift: Option<bool>,
}

impl Key {
    /// Whether a key the student pressed (all modifiers known) is this one.
    pub fn matches(&self, k: &Key) -> bool {
        let m = |want: Option<bool>, got: Option<bool>| want.is_none_or(|w| Some(w) == got);
        // A character with shift "don't care" matches either case.
        let same = if self.key.chars().count() == 1 && self.shift.is_some() { self.key == k.key } else { self.key.eq_ignore_ascii_case(&k.key) };
        same && m(self.alt, k.alt) && m(self.ctrl, k.ctrl) && m(self.shift, k.shift)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Cmd {
    Move(String),
    Press,
    Release,
    MoveAndPress(String),
    MoveXy(i32, i32),
    Kbd(Key),
    Alt,
    Msg { secs: f64, x: i32, y: i32, text: String, task: bool },
    Delay(f64),
    Duration(f64),
    Help(String, String),
    Init(bool),
    Label(String),
    Goto(String),
    GotoIfPress(String, String),
    GotoIfKbd(Key, String),
    GotoIfAlt(String),
    GotoIfDelay(String, f64, String),
    LabelDelay(String, f64),
    Set(String),
    GotoIfSet(String, String),
    Error,
    Exit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub title: String,
    cmds: Vec<Cmd>,
    labels: HashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

/// Splits a command line into words; `'x'` is one word even when `x` is a space.
fn words(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = line.trim();
    while !rest.is_empty() {
        let b = rest.as_bytes();
        let n = if b[0] == b'\'' && b.len() >= 3 && b[2] == b'\'' { 3 } else { rest.find(char::is_whitespace).unwrap_or(rest.len()) };
        out.push(rest[..n].to_string());
        rest = rest[n..].trim_start();
    }
    out
}

fn key(w: &[String]) -> Option<Key> {
    let flag = |s: &String, name: &str| if s == "*" { Some(None) } else if s.eq_ignore_ascii_case(name) { Some(Some(true)) } else { None };
    let k = &w[0];
    let key = if k.len() == 3 && k.starts_with('\'') && k.ends_with('\'') { k[1..2].to_string() } else { k.clone() };
    Some(Key { key, alt: flag(&w[1], "Alt")?, ctrl: flag(&w[2], "Ctrl")?, shift: flag(&w[3], "Shift")? })
}

pub fn parse(text: &str) -> Result<Script, ParseError> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r')).enumerate();
    let title = lines.next().map(|(_, l)| l.trim().to_string()).unwrap_or_default();
    let mut cmds = vec![];
    let mut labels = HashMap::new();
    let mut in_comment = false;
    while let Some((i, line)) = lines.next() {
        let err = |m: String| ParseError { line: i + 1, message: m };
        let w = words(line);
        let Some(first) = w.first() else { continue };
        if in_comment {
            in_comment = !(first == "comment" && w.get(1).is_some_and(|a| a == "end"));
            continue;
        }
        if let Some(l) = first.strip_prefix("<<").and_then(|l| l.strip_suffix(">>")) {
            labels.insert(l.to_string(), cmds.len());
            cmds.push(Cmd::Label(l.to_string()));
            continue;
        }
        let arg = |n: usize| w.get(n).cloned().ok_or_else(|| err(format!("`{first}` needs {n} argument(s)")));
        let num = |n: usize| arg(n)?.parse::<f64>().map_err(|_| err(format!("`{first}`: argument {n} is not a number")));
        let int = |n: usize| num(n).map(|v| v as i32);
        let cmd = match first.as_str() {
            "move" => Cmd::Move(arg(1)?),
            "press" => Cmd::Press,
            "release" | "release_no_restore" => Cmd::Release,
            "move_and_press" => Cmd::MoveAndPress(arg(1)?),
            "move_xy" => Cmd::MoveXy(int(1)?, int(2)?),
            "kbd" => Cmd::Kbd(key(w.get(1..5).ok_or_else(|| err("`kbd KEY alt ctrl shift`".into()))?).ok_or_else(|| err("bad modifier".into()))?),
            "alt" => Cmd::Alt,
            "msg" | "task" => {
                let mut body = vec![];
                loop {
                    let (_, l) = lines.next().ok_or_else(|| err("text without `_text_end`".into()))?;
                    if l.trim() == "_text_end" {
                        break;
                    }
                    body.push(l.trim_end());
                }
                Cmd::Msg { secs: num(1)?, x: int(2)?, y: int(3)?, text: body.join("\n"), task: first == "task" }
            }
            "delay" => {
                num(1)?;
                num(2)?;
                Cmd::Delay(num(3)?)
            }
            "duration" => Cmd::Duration(num(1)?),
            "help" => Cmd::Help(arg(1)?, w.get(2).cloned().unwrap_or_default()),
            "comment" => {
                in_comment = arg(1)? == "begin";
                continue;
            }
            "init" => Cmd::Init(arg(1)? == "begin"),
            "goto" => Cmd::Goto(arg(1)?),
            "goto_if_press" => Cmd::GotoIfPress(arg(1)?, arg(2)?),
            "goto_if_kbd" => Cmd::GotoIfKbd(key(w.get(1..5).ok_or_else(|| err("`goto_if_kbd KEY alt ctrl shift L`".into()))?).ok_or_else(|| err("bad modifier".into()))?, arg(5)?),
            "goto_if_alt" => Cmd::GotoIfAlt(arg(1)?),
            "goto_if_delay" => Cmd::GotoIfDelay(arg(1)?, num(2)?, arg(3)?),
            "label_delay" => Cmd::LabelDelay(arg(1)?, num(2)?),
            "set" => Cmd::Set(arg(1)?),
            "goto_if_set" => Cmd::GotoIfSet(arg(1)?, arg(2)?),
            "error" => Cmd::Error,
            "exit" => Cmd::Exit,
            other => return Err(err(format!("unknown command `{other}`"))),
        };
        cmds.push(cmd);
    }
    let script = Script { title, cmds, labels };
    for c in &script.cmds {
        let targets: Vec<&String> = match c {
            Cmd::Goto(l) | Cmd::GotoIfPress(_, l) | Cmd::GotoIfKbd(_, l) | Cmd::GotoIfAlt(l) | Cmd::LabelDelay(l, _) | Cmd::GotoIfSet(_, l) => vec![l],
            Cmd::GotoIfDelay(a, _, b) => vec![a, b],
            _ => vec![],
        };
        if let Some(l) = targets.into_iter().find(|l| !script.labels.contains_key(*l)) {
            return Err(ParseError { line: 0, message: format!("label `{l}` not found") });
        }
    }
    Ok(script)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Demo,
    /// A demonstration that waits after each action until the student goes on (`next_step`),
    /// as the original's lesson mode.
    Lesson,
    Practice,
    Exam,
}

/// What the player asks the simulator to do or show.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Point at a control (demonstration cursor).
    Point(String),
    /// Press a control (demonstration).
    Press(String),
    Release,
    Key(Key),
    Msg { text: String, x: i32, y: i32 },
    CloseMsg,
    Task(String),
    Help { file: String, topic: String },
    /// The student did something that was not expected (exam/practice).
    Wrong(String),
    Finished { completed: bool, errors: u32, secs: f64 },
}

/// What the student did.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// A control, by its full name `scope.name`.
    Press(String),
    Key(Key),
}

#[derive(Debug, Clone, PartialEq)]
enum Wait {
    Run,
    Until(f64),
    Msg(f64),
    Press(Vec<String>),
    Key(Key),
    /// Lesson mode: the student is to go on.
    Step,
    Done,
}

/// Whether control `full` (`scope.name`) is the one a script names: `name` or `scope.name`.
pub fn names(script_name: &str, full: &str) -> bool {
    let norm = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c.to_ascii_lowercase() } else { '_' }).collect::<String>();
    let (s, f) = (norm(script_name), norm(full));
    s == f || f.rsplit_once('.').is_some_and(|(_, n)| n == s)
}

pub struct Player {
    script: Script,
    pub mode: Mode,
    pc: usize,
    wait: Wait,
    started: f64,
    deadline: Option<f64>,
    in_init: bool,
    pointed: String,
    var: String,
    passed: HashMap<String, f64>,
    alternatives: Vec<(Input, String)>,
    timers: Vec<(f64, String)>,
    pub errors: u32,
    /// The actions the student was asked for (presses and keys).
    pub actions: u32,
    /// Lesson mode: a control was pressed; wait when it is released.
    step_after_release: bool,
}

/// Whether a finished lesson is passed, as the original scores it (TUTOR.CPP): an exam with at
/// most two errors, running out of time counting as one more; practice with no errors when it
/// asks for fewer than five actions, else with at most one error in ten.
pub fn passed(mode: Mode, completed: bool, errors: u32, actions: u32) -> bool {
    match mode {
        Mode::Exam => errors + u32::from(!completed) <= 2,
        _ => completed && if actions < 5 { errors == 0 } else { errors <= (actions + 5) / 10 },
    }
}

impl Player {
    pub fn new(script: Script, mode: Mode, now: f64) -> Player {
        Player { script, mode, pc: 0, wait: Wait::Run, started: now, deadline: None, in_init: false, pointed: String::new(), var: String::new(), passed: HashMap::new(), alternatives: vec![], timers: vec![], errors: 0, actions: 0, step_after_release: false }
    }

    pub fn title(&self) -> &str {
        &self.script.title
    }

    pub fn finished(&self) -> bool {
        self.wait == Wait::Done
    }

    /// Seconds left of the exam time, if it has a limit.
    pub fn time_left(&self, now: f64) -> Option<f64> {
        self.deadline.map(|d| (d - now).max(0.0))
    }

    fn student_acts(&self) -> bool {
        !matches!(self.mode, Mode::Demo | Mode::Lesson) && !self.in_init
    }

    /// Lesson mode: waiting for the student to go on.
    pub fn at_step(&self) -> bool {
        self.wait == Wait::Step
    }

    /// Lesson mode: goes on to the next action.
    pub fn next_step(&mut self, now: f64) -> Vec<Out> {
        if self.wait != Wait::Step {
            return vec![];
        }
        self.wait = Wait::Run;
        self.tick(now)
    }

    /// Lesson mode, after a demonstrated action: wait for the student.
    fn step_wait(&mut self) {
        if self.mode == Mode::Lesson && !self.in_init {
            self.wait = Wait::Step;
        }
    }

    fn jump(&mut self, label: &str) {
        self.pc = self.script.labels[label];
        self.alternatives.clear();
        self.wait = Wait::Run;
    }

    fn finish(&mut self, completed: bool, now: f64, out: &mut Vec<Out>) {
        self.wait = Wait::Done;
        out.push(Out::Finished { completed, errors: self.errors, secs: now - self.started });
    }

    /// Runs the script up to the next wait.
    pub fn tick(&mut self, now: f64) -> Vec<Out> {
        let mut out = vec![];
        if self.wait == Wait::Done {
            return out;
        }
        if self.deadline.is_some_and(|d| now >= d) {
            self.finish(false, now, &mut out);
            return out;
        }
        match self.wait {
            Wait::Until(t) if now >= t => self.wait = Wait::Run,
            Wait::Msg(t) if now >= t => {
                out.push(Out::CloseMsg);
                self.wait = Wait::Run;
            }
            Wait::Press(_) | Wait::Key(_) => {
                if let Some(i) = self.timers.iter().position(|(t, _)| now >= *t) {
                    let (_, l) = self.timers.remove(i);
                    self.jump(&l);
                }
            }
            _ => {}
        }
        // ponytail: step limit stands in for the original's "eternal loop" check.
        for _ in 0..10_000 {
            if self.wait != Wait::Run {
                return out;
            }
            let Some(cmd) = self.script.cmds.get(self.pc).cloned() else {
                self.finish(true, now, &mut out);
                return out;
            };
            self.pc += 1;
            self.step(cmd, now, &mut out);
        }
        self.finish(false, now, &mut out);
        out
    }

    fn step(&mut self, cmd: Cmd, now: f64, out: &mut Vec<Out>) {
        let student = self.student_acts();
        match cmd {
            Cmd::Move(x) => {
                self.pointed = x.clone();
                if !student {
                    out.push(Out::Point(x));
                }
            }
            Cmd::Press => {
                let x = self.pointed.clone();
                self.press(x, student, out);
            }
            Cmd::MoveAndPress(x) => self.press(x, student, out),
            Cmd::Release if !student => {
                out.push(Out::Release);
                if std::mem::take(&mut self.step_after_release) {
                    self.step_wait();
                }
            }
            Cmd::Release | Cmd::MoveXy(..) | Cmd::Error => {}
            Cmd::Kbd(k) => {
                if student {
                    self.actions += 1;
                    self.wait = Wait::Key(k);
                } else {
                    out.push(Out::Key(k));
                    self.step_wait();
                }
            }
            Cmd::Alt => {
                let k = Key { key: "Alt".into(), alt: None, ctrl: None, shift: None };
                if student {
                    self.actions += 1;
                    self.wait = Wait::Key(k);
                } else {
                    out.push(Out::Key(k));
                    self.step_wait();
                }
            }
            Cmd::Msg { secs, x, y, text, task } => {
                if task {
                    out.push(Out::Task(text.clone()));
                }
                if !student {
                    out.push(Out::Msg { text, x, y });
                    self.wait = Wait::Msg(now + secs);
                } else if self.mode == Mode::Practice && !task {
                    out.push(Out::Msg { text, x, y });
                }
            }
            Cmd::Delay(c) => {
                if !student {
                    self.wait = Wait::Until(now + c);
                }
            }
            Cmd::Duration(s) => {
                if self.mode == Mode::Exam {
                    self.deadline = Some(now + s);
                }
            }
            Cmd::Help(file, topic) => {
                if self.mode != Mode::Exam {
                    out.push(Out::Help { file, topic });
                }
            }
            Cmd::Init(begin) => self.in_init = begin,
            Cmd::Label(l) => {
                self.passed.insert(l, now);
            }
            Cmd::Goto(l) => self.jump(&l),
            Cmd::GotoIfPress(x, l) => self.alternatives.push((Input::Press(x), l)),
            Cmd::GotoIfKbd(k, l) => self.alternatives.push((Input::Key(k), l)),
            Cmd::GotoIfAlt(l) => self.alternatives.push((Input::Key(Key { key: "Alt".into(), alt: None, ctrl: None, shift: None }), l)),
            Cmd::GotoIfDelay(l, t, to) => {
                if self.passed.get(&l).is_some_and(|at| now - at > t) {
                    self.jump(&to);
                }
            }
            Cmd::LabelDelay(l, t) => {
                if student {
                    self.timers.push((now + t, l));
                }
            }
            Cmd::Set(v) => self.var = v,
            Cmd::GotoIfSet(v, l) => {
                if self.var == v {
                    self.jump(&l);
                }
            }
            Cmd::Exit => self.finish(true, now, out),
        }
    }

    fn press(&mut self, x: String, student: bool, out: &mut Vec<Out>) {
        if student {
            self.actions += 1;
            self.wait = Wait::Press(x.split('+').map(str::to_string).collect());
        } else {
            out.push(Out::Press(x));
            self.step_after_release = self.mode == Mode::Lesson;
        }
    }

    /// The student pressed a control or a key; returns `Wrong` when it was not expected.
    pub fn input(&mut self, input: Input, now: f64) -> Vec<Out> {
        let mut out = vec![];
        let hit = |want: &Input| match (want, &input) {
            (Input::Press(w), Input::Press(got)) => names(w, got),
            (Input::Key(w), Input::Key(got)) => w.matches(got),
            _ => false,
        };
        match &mut self.wait {
            Wait::Press(left) => {
                if let Input::Press(got) = &input {
                    if let Some(i) = left.iter().position(|w| names(w, got)) {
                        left.remove(i);
                        if left.is_empty() {
                            self.alternatives.clear();
                            self.wait = Wait::Run;
                        }
                        return self.tick(now);
                    }
                }
            }
            Wait::Key(k) => {
                if let Input::Key(got) = &input {
                    if k.matches(got) {
                        self.alternatives.clear();
                        self.wait = Wait::Run;
                        return self.tick(now);
                    }
                }
            }
            _ => return out,
        }
        if let Some((_, l)) = self.alternatives.iter().find(|(w, _)| hit(w)).cloned() {
            self.jump(&l);
            return self.tick(now);
        }
        // Modifier keys alone are not mistakes.
        if !matches!(&input, Input::Key(k) if ["Alt", "Ctrl", "Shift"].contains(&k.key.as_str())) {
            self.errors += 1;
            out.push(Out::Wrong(match input {
                Input::Press(n) => n,
                Input::Key(k) => k.key,
            }));
        }
        out
    }
}

/// The lesson index (the original's `SCE.TUD` structure): `N<title>` root, `.-<group>`, and
/// `*--<lesson title>|<help file>|<help topic>` entries.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub group: String,
    pub title: String,
    pub help: Option<(String, String)>,
}

pub fn parse_index(text: &str) -> Vec<Entry> {
    let mut group = String::new();
    let mut out = vec![];
    for line in text.lines().map(str::trim) {
        if let Some(g) = line.strip_prefix(".-") {
            group = g.to_string();
        } else if let Some(e) = line.strip_prefix("*--") {
            let mut parts = e.split('|');
            let title = parts.next().unwrap_or("").to_string();
            let help = match (parts.next(), parts.next()) {
                (Some(f), Some(t)) => Some((f.to_string(), t.to_string())),
                _ => None,
            };
            out.push(Entry { group: group.clone(), title, help });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LESSON: &str = "Channel 16\nduration 30\ntask 2 0 0\nSelect channel 16.\n_text_end\nmsg 3 0 0\nPress the red 16 key.\n_text_end\ngoto_if_press SHIFT Shifted\nmove_and_press VHF.16\ndelay 0 0 0.3\nrelease\nexit\n<<Shifted>>\nmsg 1 0 0\nNo shift needed.\n_text_end\ngoto_if_press 16 Done\nmove_and_press SHIFT\n<<Done>>\n";

    #[test]
    fn parses() {
        let s = parse(LESSON).unwrap();
        assert_eq!(s.title, "Channel 16");
        assert_eq!(s.labels["Done"], s.cmds.len() - 1);
        assert_eq!(words("goto_if_kbd ' ' * * Shift L"), ["goto_if_kbd", "' '", "*", "*", "Shift", "L"]);
        assert!(parse("t\ngoto Nowhere\n").is_err());
        assert!(parse("t\nfly away\n").is_err());
        assert!(parse("t\nmsg 1 0 0\nno end\n").is_err());
    }

    #[test]
    fn demo_plays_by_itself() {
        let mut p = Player::new(parse(LESSON).unwrap(), Mode::Demo, 0.0);
        let out = p.tick(0.0);
        assert_eq!(out, [Out::Task("Select channel 16.".into()), Out::Msg { text: "Select channel 16.".into(), x: 0, y: 0 }]);
        assert_eq!(p.tick(2.0), [Out::CloseMsg, Out::Msg { text: "Press the red 16 key.".into(), x: 0, y: 0 }]);
        assert_eq!(p.tick(5.0), [Out::CloseMsg, Out::Press("VHF.16".into())]);
        assert_eq!(p.tick(5.3), [Out::Release, Out::Finished { completed: true, errors: 0, secs: 5.3 }]);
    }

    #[test]
    fn lesson_waits_after_each_action() {
        let mut p = Player::new(parse(LESSON).unwrap(), Mode::Lesson, 0.0);
        p.tick(0.0);
        p.tick(2.0);
        assert_eq!(p.tick(5.0), [Out::CloseMsg, Out::Press("VHF.16".into())]);
        assert_eq!(p.tick(5.3), [Out::Release]);
        assert!(p.at_step());
        assert_eq!(p.tick(60.0), []);
        assert_eq!(p.next_step(61.0), [Out::Finished { completed: true, errors: 0, secs: 61.0 }]);
    }

    #[test]
    fn scoring() {
        use super::{passed, Mode};
        assert!(passed(Mode::Exam, true, 2, 3) && !passed(Mode::Exam, true, 3, 30));
        assert!(passed(Mode::Exam, false, 1, 3) && !passed(Mode::Exam, false, 2, 3));
        assert!(passed(Mode::Practice, true, 0, 4) && !passed(Mode::Practice, true, 1, 4));
        assert!(passed(Mode::Practice, true, 1, 5) && !passed(Mode::Practice, true, 2, 14) && passed(Mode::Practice, true, 2, 15));
        assert!(!passed(Mode::Practice, false, 0, 9));
    }

    #[test]
    fn exam_waits_for_the_student() {
        let mut p = Player::new(parse(LESSON).unwrap(), Mode::Exam, 0.0);
        assert_eq!(p.tick(0.0), [Out::Task("Select channel 16.".into())]);
        assert_eq!(p.input(Input::Press("VHF.5".into()), 1.0), [Out::Wrong("VHF.5".into())]);
        let out = p.input(Input::Press("VHF.16".into()), 2.0);
        assert_eq!(out, [Out::Finished { completed: true, errors: 1, secs: 2.0 }]);
    }

    #[test]
    fn alternatives_and_time_limit() {
        let mut p = Player::new(parse(LESSON).unwrap(), Mode::Exam, 0.0);
        p.tick(0.0);
        assert!(p.input(Input::Press("VHF.SHIFT".into()), 1.0).is_empty());
        assert_eq!(p.input(Input::Press("VHF.16".into()), 2.0), [Out::Finished { completed: true, errors: 0, secs: 2.0 }]);
        let mut p = Player::new(parse(LESSON).unwrap(), Mode::Exam, 0.0);
        p.tick(0.0);
        assert_eq!(p.time_left(10.0), Some(20.0));
        assert_eq!(p.tick(31.0), [Out::Finished { completed: false, errors: 0, secs: 31.0 }]);
    }

    #[test]
    fn keys_and_names() {
        let want = Key { key: "a".into(), alt: Some(true), ctrl: None, shift: None };
        assert!(want.matches(&Key { key: "a".into(), alt: Some(true), ctrl: Some(false), shift: Some(false) }));
        assert!(!want.matches(&Key { key: "a".into(), alt: Some(false), ctrl: Some(false), shift: Some(false) }));
        assert!(want.matches(&Key { key: "A".into(), alt: Some(true), ctrl: Some(false), shift: Some(true) }));
        let lower = Key { key: "a".into(), alt: None, ctrl: None, shift: Some(false) };
        assert!(!lower.matches(&Key { key: "A".into(), alt: Some(false), ctrl: Some(false), shift: Some(true) }));
        assert!(names("Vol_OFF", "VHF.VOL/OFF") && names("vhf.16", "VHF.16") && !names("HF.16", "VHF.16"));
        let idx = parse_index("NGMDSS units\n.-VHF RT2048\n*--Operating|vhf.hlp|Oper\n");
        assert_eq!(idx, [Entry { group: "VHF RT2048".into(), title: "Operating".into(), help: Some(("vhf.hlp".into(), "Oper".into())) }]);
    }
}

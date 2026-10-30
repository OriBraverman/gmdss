//! A ship's radiotelex connection to an HF coast station, driven by the station's script
//! ([`crate::coast_station`]).
//!
//! The station is in a *state* named by a block kind (`'0'` after connecting). Each typed
//! character goes into the current line and into a window of the last few characters:
//! * an exact command `'KEY'` of the state's block runs as soon as the window ends with `KEY`;
//! * `+` (end of a telex line) runs the first prefix command `"KEY"` the line starts with,
//!   or the first `*` command, whichever comes first in the block. The text after `KEY` is
//!   kept as the command argument (the number to dial, for example).
//!
//! A running command queues its lines (each followed by a newline) and then the station
//! "enters" its state again: it prints the state block's text before its first command (the
//! prompt, without newlines). While text is printed, `\` escapes are expanded:
//!
//! | escape          | effect                                                          |
//! |-----------------|-----------------------------------------------------------------|
//! | `\0`-`\9` `\A`-`\Z` | switch to that state                                        |
//! | `\*` `\%` `\:`  | ship's answerback, ship's name, first word of letters of the answerback after leading digits/spaces |
//! | `\[` `\]`       | date `d.m.y` (y = year - 1900) and time `h:mm`                  |
//! | `\(` `\)`       | next message reference as `A<n>43` and `<n>` in 5 digits        |
//! | `\!`            | minutes since connecting, `%2.1f`                               |
//! | `\<` `\>`       | connected subscriber's number and answerback                    |
//! | `\|NC|OCC|`     | connect to the subscriber whose number is the argument: prints `NC` and drops the rest of the text if there is none, prints only `OCC` if busy, prints nothing and continues if connected |
//! | `\~`            | release the connected subscriber                                |
//! | `\-` `\=` `\&` `\{` `\}` | events in the output: [`Out::Disconnect`], [`Out::Pause`], [`Out::Mom`], [`Out::StartTimer`], [`Out::ElapsedMinutes`] |
//!
//! Not modelled here: frequency checks when connecting, ARQ timing and idle timeouts. Those
//! belong to the radio side; this module is the station's conversation.

use crate::coast_station::{Block, CoastStation};
use crate::subscriber::Subscriber;

/// What the station sends, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    Char(u8),
    /// `\-`: the station clears the connection once everything before it has been sent.
    /// The caller then calls [`Session::hang_up`].
    Disconnect,
    /// `\=`: about 20 s (360 BIOS ticks) of no traffic.
    Pause,
    /// `\&`: about 1-1.5 s (20-25 BIOS ticks, random) of no traffic, as after "MOM".
    Mom,
    /// `\{`: remember the time this point is sent.
    StartTimer,
    /// `\}`: send the minutes since the last [`Out::StartTimer`], formatted `%2.1f`.
    ElapsedMinutes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ship {
    pub name: String,
    pub answerback: String,
}

/// Simulation clock. `seconds` only needs to be monotonic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimTime {
    pub seconds: u32,
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

/// State shared by all coast station channels.
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub subscribers: Vec<Subscriber>,
    message_ref: u16,
}

impl Network {
    pub fn new(subscribers: Vec<Subscriber>) -> Network {
        Network { subscribers, message_ref: 143 }
    }

    /// Advances the message reference by 1-5, wrapping to 143 above 9999.
    fn next_ref(&mut self, random: &mut dyn FnMut(u16) -> u16) -> u16 {
        self.message_ref += random(5) + 1;
        if self.message_ref > 9999 {
            self.message_ref = 143;
        }
        self.message_ref
    }
}

/// Everything a call needs from outside the session.
pub struct Ctx<'a> {
    pub now: SimTime,
    pub net: &'a mut Network,
    /// `random(n)`: a number in `0..n`.
    pub random: &'a mut dyn FnMut(u16) -> u16,
}

/// Length of the typed-character window: the longest exact command over all stations.
pub fn window_len(stations: &[CoastStation]) -> usize {
    let exact_keys = stations.iter().flat_map(|s| &s.blocks).flat_map(|b| &b.exact);
    exact_keys.filter_map(|e| e.key.as_ref()).map(|k| k.len()).max().unwrap_or(0)
}

pub struct Session<'s> {
    station: &'s CoastStation,
    ship: Ship,
    state: u8,
    /// Script text waiting to be printed; consumed from `pos`. 0x06 marks bytes to skip.
    staging: Vec<u8>,
    pos: usize,
    line: Vec<u8>,
    window: Vec<u8>,
    arg: Vec<u8>,
    partner: Option<usize>,
    connected_at: u32,
    closed: bool,
}

impl<'s> Session<'s> {
    /// The station accepts the call: it sends block `+` (its identification), then the
    /// prompt of state `'0'`.
    pub fn connect(station: &'s CoastStation, ship: Ship, window_len: usize, ctx: &mut Ctx) -> (Self, Vec<Out>) {
        let mut s = Session {
            station,
            ship,
            state: b'0',
            staging: vec![],
            pos: 0,
            line: vec![],
            window: vec![0; window_len],
            arg: vec![],
            partner: None,
            connected_at: ctx.now.seconds,
            closed: false,
        };
        let mut out = vec![];
        if let Some(block) = s.block(b'+') {
            for e in &block.prefix {
                s.queue_line(e.text.as_deref());
            }
        }
        s.enter_state(ctx, &mut out);
        (s, out)
    }

    /// The current state (block kind).
    pub fn state(&self) -> u8 {
        self.state
    }

    /// The channel was cleared (unknown state while in `'0'`, or [`Session::hang_up`]).
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// One character from the ship.
    pub fn type_char(&mut self, c: u8, ctx: &mut Ctx) -> Vec<Out> {
        let mut out = vec![];
        if self.closed {
            return out;
        }
        let c = c.to_ascii_uppercase();
        if c != 0 {
            self.line.push(c);
            if !self.window.is_empty() {
                self.window.remove(0);
                self.window.push(c);
            }
        }
        self.check_exact(ctx, &mut out);
        if c == b'+' && !self.closed {
            self.window.fill(0);
            let line = std::mem::take(&mut self.line);
            self.run_line(&line, ctx, &mut out);
        }
        out
    }

    /// Several characters, one after the other.
    pub fn type_str(&mut self, s: &str, ctx: &mut Ctx) -> Vec<Out> {
        s.bytes().flat_map(|c| self.type_char(c, ctx)).collect()
    }

    /// Ends the connection and frees the subscriber it held.
    pub fn hang_up(&mut self, net: &mut Network) {
        if let Some(p) = self.partner.take() {
            net.subscribers[p].busy = false;
        }
        self.closed = true;
    }

    fn block(&self, kind: u8) -> Option<&'s Block> {
        self.station.blocks.iter().find(|b| b.kind == kind)
    }

    /// The block of the current state; an unknown state resets to `'0'`, or clears the
    /// channel if it already was `'0'`.
    fn state_block(&mut self, net: &mut Network) -> Option<&'s Block> {
        let block = self.block(self.state);
        if block.is_none() {
            if self.state == b'0' {
                self.hang_up(net);
            } else {
                self.state = b'0';
            }
        }
        block
    }

    fn check_exact(&mut self, ctx: &mut Ctx, out: &mut Vec<Out>) {
        let Some(block) = self.state_block(ctx.net) else { return };
        let window = &self.window;
        let hit = block.exact.iter().position(|e| e.key.as_ref().is_some_and(|k| window.ends_with(k.as_bytes())));
        if let Some(i) = hit {
            self.line.clear();
            self.window.fill(0);
            self.restart_staging();
            self.run_command(&block.exact, i);
            self.enter_state(ctx, out);
        }
    }

    fn run_line(&mut self, line: &[u8], ctx: &mut Ctx, out: &mut Vec<Out>) {
        let start = line.iter().position(|&c| !matches!(c, b' ' | 0 | b'\n' | b'\r' | b'\t'));
        let Some(start) = start else { return };
        let line = &line[start..];
        let Some(block) = self.state_block(ctx.net) else { return };
        self.arg.clear();
        self.restart_staging();
        for (i, e) in block.prefix.iter().enumerate() {
            let Some(key) = &e.key else { continue };
            if key.starts_with('*') {
                self.run_command(&block.prefix, i);
                break;
            }
            if let Some(rest) = line.strip_prefix(key.as_bytes()) {
                self.arg = rest.to_vec();
                self.run_command(&block.prefix, i);
                break;
            }
        }
        self.enter_state(ctx, out);
    }

    /// Queues the command at `entries[i]` and its continuation lines.
    fn run_command(&mut self, entries: &[crate::coast_station::Entry], i: usize) {
        self.queue_line(entries[i].text.as_deref());
        for e in entries[i + 1..].iter().take_while(|e| e.key.is_none()) {
            self.queue_line(e.text.as_deref());
        }
    }

    fn restart_staging(&mut self) {
        self.staging = b"\n".to_vec();
        self.pos = 0;
    }

    fn queue_line(&mut self, text: Option<&str>) {
        if let Some(t) = text {
            self.staging.extend(t.bytes());
        }
        self.staging.push(b'\n');
    }

    /// Prints what is queued, then the prompt of the (possibly new) state.
    fn enter_state(&mut self, ctx: &mut Ctx, out: &mut Vec<Out>) {
        self.drain(ctx, out);
        if let Some(block) = self.block(self.state) {
            for e in block.prefix.iter().take_while(|e| e.key.is_none()) {
                if let Some(t) = &e.text {
                    self.staging.extend(t.bytes());
                }
            }
        }
        self.drain(ctx, out);
    }

    fn pop(&mut self) -> u8 {
        let c = self.staging.get(self.pos).copied().unwrap_or(0);
        if c != 0 {
            self.pos += 1;
        }
        c
    }

    fn drain(&mut self, ctx: &mut Ctx, out: &mut Vec<Out>) {
        loop {
            match self.pop() {
                0 => break,
                b'\\' => {
                    let c = self.pop();
                    if c.is_ascii_digit() || c.is_ascii_uppercase() {
                        self.state = c;
                    } else if self.escape(c, ctx, out) {
                        self.dial(ctx.net);
                    }
                }
                0x06 => {}
                c => out.push(Out::Char(c)),
            }
        }
        self.staging.clear();
        self.pos = 0;
    }

    /// Expands one escape; returns true for `\|`, which the caller handles.
    fn escape(&mut self, c: u8, ctx: &mut Ctx, out: &mut Vec<Out>) -> bool {
        let text = |out: &mut Vec<Out>, s: &str| out.extend(s.bytes().map(Out::Char));
        let partner = self.partner.map(|p| &ctx.net.subscribers[p]);
        match c {
            b'*' => text(out, &self.ship.answerback),
            b'%' => text(out, &self.ship.name),
            b':' => text(out, &call_sign_word(&self.ship.answerback)),
            b'!' => text(out, &format!("{:2.1}", ctx.now.seconds.wrapping_sub(self.connected_at) as f32 / 60.0)),
            b'(' => text(out, &format!("A{}43", ctx.net.next_ref(ctx.random))),
            b')' => text(out, &format!("{:05}", ctx.net.next_ref(ctx.random))),
            b'[' => text(out, &format!("{}.{}.{}", ctx.now.day, ctx.now.month, i32::from(ctx.now.year) - 1900)),
            b']' => text(out, &format!("{}:{:02}", ctx.now.hour, ctx.now.minute)),
            b'<' => text(out, partner.map_or("NO SUBSCRIBER", |p| &p.number)),
            b'>' => text(out, partner.map_or("NO SUBSCRIBER", |p| &p.answerback)),
            b'~' => match self.partner.take() {
                Some(p) => ctx.net.subscribers[p].busy = false,
                None => text(out, "NO SUBSCRIBER"),
            },
            b'-' => out.push(Out::Disconnect),
            b'=' => out.push(Out::Pause),
            b'&' => out.push(Out::Mom),
            b'{' => out.push(Out::StartTimer),
            b'}' => out.push(Out::ElapsedMinutes),
            b'|' => return true,
            _ => {} // the original logs "Unknown command char"
        }
        false
    }

    /// `\|NONE|BUSY|`: connect to the subscriber numbered by the command argument.
    fn dial(&mut self, net: &mut Network) {
        let rest = &self.staging[self.pos..];
        let Some(bar1) = rest.iter().position(|&c| c == b'|') else { return };
        let Some(bar2) = rest[bar1 + 1..].iter().position(|&c| c == b'|').map(|p| bar1 + 1 + p) else { return };
        let (bar1, bar2) = (self.pos + bar1, self.pos + bar2);
        match net.subscribers.iter().position(|s| s.number.as_bytes() == self.arg.as_slice()) {
            None => self.staging.truncate(bar1),
            Some(i) if net.subscribers[i].busy => {
                self.staging[self.pos..=bar1].fill(0x06);
                self.staging.truncate(bar2);
            }
            Some(i) => {
                net.subscribers[i].busy = true;
                self.staging[self.pos..=bar2].fill(0x06);
                // The original never connects to the first subscriber (index 0) although it
                // marks it busy; its connection then reads "NO SUBSCRIBER".
                self.partner = (i > 0).then_some(i);
            }
        }
    }
}

/// `\:`: skip leading digits and spaces, then take letters up to the first non-letter.
fn call_sign_word(answerback: &str) -> String {
    let rest = answerback.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ');
    rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

/// Renders output for display: events become `<MOM>`, `<PAUSE>`, `<CLEAR>`, `<TIMER>`,
/// and the elapsed time `<MIN>`.
pub fn transcript(out: &[Out]) -> String {
    out.iter()
        .map(|o| match o {
            Out::Char(c) => (*c as char).to_string(),
            Out::Disconnect => "<CLEAR>".into(),
            Out::Pause => "<PAUSE>".into(),
            Out::Mom => "<MOM>".into(),
            Out::StartTimer => "<TIMER>".into(),
            Out::ElapsedMinutes => "<MIN>".into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATION: &str = r#"+ 4711 HARBOUR XX
 \*
 \0
# 4000 4100
Q 4711
\0 GA+?
'BRK+' \-
'TEST+' MOM \&
 QUICK FOX \( \)
"DIAL" MOM \& \|NC|OCC|
 \<
 \>
 \5
'HELP+' MOM \&
 LIST
*
 \0
\5
 CONNECTED \[ \]
'BYE+' \~ \0
"#;

    fn subs() -> Vec<Subscriber> {
        let s = |n: &str, num: &str, ab: &str| Subscriber { name: n.into(), number: num.into(), answerback: ab.into(), busy: false };
        vec![s("First", "100+", "100 FIRST X"), s("Office", "200+", "200 OFFICE X"), s("Agent", "300+", "300 AGENT X")]
    }

    fn now(seconds: u32) -> SimTime {
        SimTime { seconds, year: 1997, month: 2, day: 14, hour: 9, minute: 5 }
    }

    fn run(f: impl FnOnce(&mut Session, &mut Ctx) -> String) -> String {
        let st = CoastStation::parse(STATION.as_bytes());
        let mut net = Network::new(subs());
        let mut rnd = |_n: u16| 1;
        let mut ctx = Ctx { now: now(0), net: &mut net, random: &mut rnd };
        let ship = Ship { name: "SEA STAR".into(), answerback: "1234567 SEASTAR X".into() };
        let (mut s, out) = Session::connect(&st, ship, window_len(std::slice::from_ref(&st)), &mut ctx);
        transcript(&out) + &f(&mut s, &mut ctx)
    }

    #[test]
    fn connect_sends_identification_and_prompt() {
        assert_eq!(run(|_, _| String::new()), " 4711 HARBOUR XX\n 1234567 SEASTAR X\n \n GA+?");
    }

    #[test]
    fn exact_command_runs_when_typed() {
        let out = run(|s, c| transcript(&s.type_str("xxtest+", c)));
        // counter starts at 143, random(5) = 1 each time -> +2
        assert!(out.ends_with("\n MOM <MOM>\n QUICK FOX A14543 00147\n GA+?"), "{out:?}");
    }

    #[test]
    fn prefix_command_and_wildcard() {
        let out = run(|s, c| transcript(&s.type_str("  HELP+", c)) + "|" + &transcript(&s.type_str("WHAT+", c)));
        // HELP+ is exact; WHAT+ falls to the `*` entry, which has no text of its own
        assert!(out.ends_with("\n MOM <MOM>\n LIST\n GA+?|\n\n \n GA+?"), "{out:?}");
    }

    #[test]
    fn dialling_subscribers() {
        let out = run(|s, c| {
            let a = transcript(&s.type_str("DIAL999+", c));
            let b = transcript(&s.type_str("DIAL200+", c));
            let busy = c.net.subscribers[1].busy;
            let d = transcript(&s.type_str("BYE+", c));
            format!("{a}|{b}|{busy}|{d}|{}", c.net.subscribers[1].busy)
        });
        let parts: Vec<&str> = out.splitn(2, " GA+?").nth(1).unwrap().split('|').collect();
        assert_eq!(parts, vec![
            "\n MOM <MOM> NC GA+?",                                   // unknown number
            "\n MOM <MOM> \n 200+\n 200 OFFICE X\n \n CONNECTED 14.2.97 9:05", // connected, state 5
            "true",
            "\n  \n GA+?", // " \~ \0": release, back to state 0

            "false",
        ]);
    }

    #[test]
    fn busy_and_first_subscriber_quirk() {
        let out = run(|s, c| {
            c.net.subscribers[2].busy = true;
            let busy = transcript(&s.type_str("DIAL300+", c));
            let first = transcript(&s.type_str("DIAL100+", c));
            format!("{busy}|{first}|{}", c.net.subscribers[0].busy)
        });
        let tail = out.splitn(2, " GA+?").nth(1).unwrap();
        assert_eq!(tail, "\n MOM <MOM> OCC GA+?|\n MOM <MOM> \n NO SUBSCRIBER\n NO SUBSCRIBER\n \n CONNECTED 14.2.97 9:05|true");
    }

    #[test]
    fn disconnect_event() {
        let out = run(|s, c| transcript(&s.type_str("BRK+", c)));
        assert!(out.ends_with(" GA+?\n <CLEAR>\n GA+?"), "{out:?}");
    }

    #[test]
    fn call_sign_word_rules() {
        assert_eq!(call_sign_word("1234567 SEASTAR X"), "SEASTAR");
        assert_eq!(call_sign_word("9VG SERADIO"), "VG");
        assert_eq!(call_sign_word(""), "");
    }
}

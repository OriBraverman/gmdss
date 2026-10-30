//! Radiotelex (SITOR ARQ) message terminal: call a coast station, type commands, read replies
//! as they arrive over the link.

use crate::clock::SimClock;
use crate::Ship;
use eframe::egui::{self, Color32, FontId, Key, RichText};
use gmdss::coast_station::CoastStation;
use gmdss::subscriber::Subscriber;
use gmdss::telex::{self, Ctx, Network, Out, Session, SimTime};
use std::collections::VecDeque;

/// One ARQ cycle carries a block of 3 characters every 450 ms.
const ARQ_CYCLE: f64 = 0.45;
const SCREEN: Color32 = Color32::from_rgb(12, 14, 12);
const INK: Color32 = Color32::from_rgb(200, 210, 200);

enum Link {
    Idle,
    Calling { station: usize, connect_at: f64 },
    Connected { station: usize, session: Session<'static> },
}

pub struct TelexTerminal {
    net: Network,
    rng: u32,
    selected: usize,
    link: Link,
    lines: Vec<String>,
    queue: VecDeque<Out>,
    next_block: f64,
    timer_start: u32,
    /// Simulation time of the current frame.
    time: SimTime,
}

impl TelexTerminal {
    pub fn new(subscribers: Vec<Subscriber>) -> TelexTerminal {
        TelexTerminal {
            net: Network::new(subscribers),
            rng: 0x2545_f491,
            selected: 0,
            link: Link::Idle,
            lines: vec![String::new()],
            queue: VecDeque::new(),
            next_block: 0.0,
            timer_start: 0,
            time: SimClock::new().now(),
        }
    }

    fn print(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '\n' => self.lines.push(String::new()),
                '\r' => {}
                c => self.lines.last_mut().unwrap().push(c),
            }
        }
    }

    fn print_line(&mut self, s: &str) {
        if !self.lines.last().unwrap().is_empty() {
            self.print("\n");
        }
        self.print(s);
        self.print("\n");
    }

    pub fn show(&mut self, ui: &mut egui::Ui, stations: &'static [CoastStation], ship: &Ship, clock: &SimClock) {
        let now = clock.secs();
        self.time = clock.now();
        self.step(now, stations, ship);
        self.keyboard(ui, now, stations, ship);
        self.controls(ui, now, stations);
        ui.add_space(8.0);
        self.screen(ui, ship);
        if !matches!(self.link, Link::Idle) || !self.queue.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Connects when the call has rung long enough and sends due ARQ blocks.
    fn step(&mut self, now: f64, stations: &'static [CoastStation], ship: &Ship) {
        if let Link::Calling { station, connect_at } = self.link {
            if now >= connect_at {
                let mut rng = self.rng;
                let mut rnd = |n: u16| random(&mut rng, n);
                let mut ctx = Ctx { now: self.time, net: &mut self.net, random: &mut rnd };
                let me = telex::Ship { name: ship.name.clone(), answerback: ship.answerback.clone() };
                let (session, out) = Session::connect(&stations[station], me, telex::window_len(stations), &mut ctx);
                self.rng = rng;
                self.queue.extend(out);
                self.link = Link::Connected { station, session };
                self.next_block = now;
            }
        }
        while now >= self.next_block && !self.queue.is_empty() {
            let mut sent = 0;
            self.next_block = now + ARQ_CYCLE;
            while sent < 3 {
                let Some(o) = self.queue.pop_front() else { break };
                match o {
                    Out::Char(c) => {
                        self.print(&(c as char).to_string());
                        sent += 1;
                    }
                    Out::Mom => {
                        self.next_block = now + 1.1 + f64::from(random(&mut self.rng, 6)) * 0.055;
                        break;
                    }
                    Out::Pause => {
                        self.next_block = now + 20.0;
                        break;
                    }
                    Out::StartTimer => self.timer_start = self.time.seconds,
                    Out::ElapsedMinutes => {
                        let minutes = self.time.seconds.saturating_sub(self.timer_start) as f32 / 60.0;
                        for c in format!("{minutes:2.1}").bytes().rev() {
                            self.queue.push_front(Out::Char(c));
                        }
                    }
                    Out::Disconnect => {
                        self.hang_up("*** END OF CONNECTION");
                        break;
                    }
                }
            }
        }
    }

    fn hang_up(&mut self, why: &str) {
        if let Link::Connected { session, .. } = &mut self.link {
            session.hang_up(&mut self.net);
        }
        self.link = Link::Idle;
        self.queue.clear();
        let t = self.time;
        self.print_line(&format!("{why} {:02}:{:02}", t.hour, t.minute));
    }

    fn call(&mut self, now: f64, stations: &'static [CoastStation]) {
        let st = &stations[self.selected];
        let t = self.time;
        let ch = st.channels[0];
        // The ship receives on the station's transmit frequency and vice versa.
        self.print_line(&format!(
            "*** {}/{}-{:02} {:02}:{:02}, RX: {:.1} kHz, TX: {:.1} kHz, ARQ to {}",
            t.day, t.month, t.year % 100, t.hour, t.minute, ch.transmit_khz, ch.receive_khz, st.number
        ));
        self.link = Link::Calling { station: self.selected, connect_at: now + 2.0 + f64::from(random(&mut self.rng, 3)) };
    }

    /// Sends text from the keyboard to the station, echoing it locally.
    fn send(&mut self, text: &str) {
        let Link::Connected { session, .. } = &mut self.link else { return };
        let mut rng = self.rng;
        let mut rnd = |n: u16| random(&mut rng, n);
        let mut ctx = Ctx { now: self.time, net: &mut self.net, random: &mut rnd };
        let out = session.type_str(text, &mut ctx);
        let closed = session.is_closed();
        self.rng = rng;
        self.queue.extend(out);
        self.print(text);
        if closed {
            self.hang_up("*** CONNECTION CLEARED");
        }
    }

    fn keyboard(&mut self, ui: &mut egui::Ui, now: f64, stations: &'static [CoastStation], ship: &Ship) {
        let events = ui.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Text(t) => {
                    // Telex (ITA2) has capitals, digits and a few signs only.
                    let t: String = t.to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric() || " -?:().,'=/+".contains(*c)).collect();
                    if !t.is_empty() {
                        self.send(&t);
                    }
                }
                egui::Event::Key { key, pressed: true, .. } => self.function_key(key, now, stations, ship),
                _ => {}
            }
        }
    }

    fn function_key(&mut self, key: Key, now: f64, stations: &'static [CoastStation], ship: &Ship) {
        let t = self.time;
        match key {
            Key::Enter => self.send("\r\n"),
            Key::F1 if matches!(self.link, Link::Idle) && !stations.is_empty() => self.call(now, stations),
            Key::F3 => self.send(&format!("{:02}:{:02}", t.hour, t.minute)),
            Key::F4 => self.send(&format!("{:02}.{:02}.{:02}", t.day, t.month, t.year % 100)),
            Key::F6 => {
                if let Link::Connected { station, .. } = self.link {
                    let wru = format!("\n{}\n", stations[station].answerback);
                    self.queue.extend(wru.bytes().map(Out::Char));
                }
            }
            Key::F7 => self.send(&format!("DE {}", ship.answerback)),
            Key::F8 => self.send("+"),
            Key::F9 if !matches!(self.link, Link::Idle) => self.hang_up("*** BREAK"),
            _ => {}
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui, now: f64, stations: &'static [CoastStation]) {
        ui.horizontal(|ui| {
            let idle = matches!(self.link, Link::Idle);
            ui.label(RichText::new("Coast station").color(Color32::LIGHT_GRAY));
            ui.add_enabled_ui(idle, |ui| {
                egui::ComboBox::from_id_salt("station")
                    .width(260.0)
                    .selected_text(stations.get(self.selected).map_or(String::new(), label))
                    .show_ui(ui, |ui| {
                        for (i, st) in stations.iter().enumerate() {
                            ui.selectable_value(&mut self.selected, i, label(st));
                        }
                    });
            });
            if crate::panel::hook(&ui.add_enabled(idle && !stations.is_empty(), egui::Button::new("Call  (F1)")), "HF_Tlx", "Call") && idle && !stations.is_empty() {
                self.call(now, stations);
            }
            if crate::panel::hook(&ui.add_enabled(!idle, egui::Button::new("Break  (F9)")), "HF_Tlx", "Break") && !idle {
                self.hang_up("*** BREAK");
            }
        });
    }

    fn screen(&mut self, ui: &mut egui::Ui, ship: &Ship) {
        let font = FontId::monospace(14.0);
        let t = self.time;
        let status = match &self.link {
            Link::Idle => "Standby - choose a station and press F1 to call".to_string(),
            Link::Calling { .. } => "Calling ...".to_string(),
            Link::Connected { .. } if !self.queue.is_empty() => "Reception in progress".to_string(),
            Link::Connected { session, .. } => format!("Link established - type a command and end it with +   (state {})", session.state() as char),
        };
        // Bezel around the CRT.
        egui::Frame::new().fill(Color32::from_rgb(150, 152, 145)).corner_radius(10).inner_margin(18).show(ui, |ui| {
            egui::Frame::new().fill(SCREEN).corner_radius(4).inner_margin(10).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let bar = |ui: &mut egui::Ui, text: String| {
                    egui::Frame::new().fill(INK).inner_margin(2).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(text).font(font.clone()).color(SCREEN));
                    });
                };
                bar(ui, format!("RADIOTLX   {:^32}   {:02}.{:02}.{:02} {:02}:{:02}", ship.name, t.day, t.month, t.year % 100, t.hour, t.minute));
                ui.label(RichText::new(format!("{:^78}", "TELEX (ARQ) TERMINAL")).font(font.clone()).color(INK));
                let height = (ui.available_height() - 70.0).max(120.0);
                egui::ScrollArea::vertical().max_height(height).min_scrolled_height(height).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    let mut text = self.lines.join("\n");
                    if !matches!(self.link, Link::Idle) {
                        text.push('_');
                    }
                    ui.label(RichText::new(text).font(font.clone()).color(INK));
                });
                bar(ui, " F1 Call  F3 Time  F4 Date  F6 WRU  F7 DE  F8 Over  F9 Break".into());
                ui.label(RichText::new(status).font(font.clone()).color(INK));
            });
        });
    }
}

/// xorshift32; `0..n`.
fn random(state: &mut u32, n: u16) -> u16 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state % u32::from(n.max(1))) as u16
}

fn label(st: &CoastStation) -> String {
    format!("{} ({})", st.name, st.number)
}

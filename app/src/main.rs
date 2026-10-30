//! GMDSS training station: the operator's console with one device shown at a time.
//!
//! Usage: `gmdss-app [DATA_DIR] [--instructor PORT | --student HOST:PORT] [--lessons DIR]`. The
//! coast station (`*.CS`) and subscriber (`*.EU`) files are read from DATA_DIR, default
//! `../_gmdss/data` or `_gmdss/data`. With `--instructor` this console teaches a class; `--student`
//! joins one. `--lessons` adds the lesson scripts (`*.txt`, index `SCE.TUD*`) of a folder.

mod books_page;
mod chart_page;
mod clock;
mod dsc_panel;
mod hf_page;
mod inmarsat_page;
mod net_link;
mod options_page;
mod panel;
mod sound;
#[cfg(test)]
mod snapshot;
mod svg_art;
mod telex_terminal;
mod tutor_page;
mod units_page;
mod vhf_page;

use eframe::egui::{self, Color32, RichText};
use gmdss::coast_station::CoastStation;
use gmdss::subscriber::{parse_subscribers, Subscriber};
use std::path::{Path, PathBuf};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let dir = args.iter().enumerate().find(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--"))).map(|(_, a)| PathBuf::from(a)).or_else(|| {
        ["../_gmdss/data", "_gmdss/data", "../../_gmdss/data"].iter().map(PathBuf::from).find(|p| p.is_dir())
    });
    // Extra lessons in the lesson language, e.g. the user's own copies of the original's.
    let lessons = flag("--lessons");
    let link = match (flag("--instructor"), flag("--student")) {
        (Some(port), _) => Some(port.parse().map_err(|_| std::io::Error::other("bad port")).and_then(net_link::Link::instructor)),
        (_, Some(addr)) => Some(net_link::Link::student(&addr)),
        _ => None,
    };
    let link = match link {
        Some(Err(e)) => {
            eprintln!("Network: {e} - running stand-alone");
            None
        }
        l => l.map(Result::unwrap),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1180.0, 760.0]).with_title("GMDSS Training Station"),
        ..Default::default()
    };
    eframe::run_native("gmdss", options, Box::new(|cc| {
        cc.egui_ctx.set_theme(egui::Theme::Dark); // ignore the system light theme
        let mut app = App::new(dir, link);
        app.tutor.load_examinees();
        if let Some(d) = &lessons {
            app.tutor.load_dir(Path::new(d));
            app.tutor.record_dir = PathBuf::from(d);
        }
        Ok(Box::new(app))
    }))
}

#[derive(Clone)]
pub struct Ship {
    pub name: String,
    pub call_sign: String,
    pub mmsi: String,
    pub telex: String,
    pub answerback: String,
    /// Position in minutes of arc, north/east positive.
    pub lat_min: i32,
    pub lon_min: i32,
}

fn default_ship() -> Ship {
    Ship {
        name: "SEA STAR".into(),
        call_sign: "4XAB".into(),
        mmsi: "428123000".into(),
        telex: "1234567".into(),
        answerback: "1234567 SEASTAR X".into(),
        lat_min: 32 * 60 + 55,
        lon_min: 34 * 60 + 40,
    }
}

/// Original data files, loaded once.
pub struct Data {
    pub stations: &'static [CoastStation],
    pub subscribers: Vec<Subscriber>,
    pub ntx: Vec<gmdss::navtex::Message>,
    pub ces: gmdss::ces_list::CesList,
    pub egc: Vec<gmdss::egc::Broadcast>,
    pub dir: PathBuf,
}

fn load(dir: &Path) -> std::io::Result<Data> {
    let mut stations = vec![];
    let mut subscribers = vec![];
    let mut ces = gmdss::ces_list::CesList::default();
    let mut egc = vec![];
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?.map(|e| e.map(|e| e.path())).collect::<Result<_, _>>()?;
    paths.sort();
    for p in paths {
        let ext = p.extension().map(|e| e.to_ascii_lowercase());
        match ext.as_ref().and_then(|e| e.to_str()) {
            Some("cs") => {
                let st = CoastStation::parse(&std::fs::read(&p)?);
                if st.is_valid() {
                    stations.push(st);
                }
            }
            Some("eu") => {
                parse_subscribers(&std::fs::read(&p)?, &mut subscribers);
            }
            Some("ces") => ces = gmdss::ces_list::CesList::parse(&std::fs::read(&p)?),
            Some("ctl") if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("INM_EGC.CTL")) => {
                if let Ok(records) = gmdss::egc::parse_control(&std::fs::read(&p)?) {
                    egc = records.into_iter().filter_map(|r| match r {
                        gmdss::egc::ControlRecord::Broadcast(b) => Some(b),
                        _ => None,
                    }).collect();
                }
            }
            _ => {}
        }
    }
    // ponytail: leaked once at start so sessions can borrow stations for the app's lifetime.
    Ok(Data { stations: Box::leak(stations.into_boxed_slice()), subscribers, ntx: units_page::load_ntx(dir), ces, egc, dir: dir.to_path_buf() })
}

#[derive(Clone, Copy, PartialEq)]
enum Device {
    VhfDsc,
    MfHfDsc,
    Radiotelex,
    InmarsatA,
    InmarsatC,
    Navtex,
    Epirb,
    Sart,
    Portable,
    R501,
    Power,
    Printer,
}

impl Device {
    const ALL: [Device; 12] = [Self::VhfDsc, Self::MfHfDsc, Self::Radiotelex, Self::InmarsatA, Self::InmarsatC, Self::Navtex, Self::Epirb, Self::Sart, Self::Portable, Self::R501, Self::Power, Self::Printer];

    fn label(self) -> &'static str {
        match self {
            Self::VhfDsc => "VHF & DSC",
            Self::MfHfDsc => "MF/HF & DSC",
            Self::Radiotelex => "Radiotelex",
            Self::InmarsatA => "Inmarsat-A",
            Self::InmarsatC => "Inmarsat-C",
            Self::Navtex => "NAVTEX",
            Self::Epirb => "EPIRB",
            Self::Sart => "SART",
            Self::Portable => "VHF portable",
            Self::R501 => "2182 kHz watch receiver",
            Self::Power => "Battery charger",
            Self::Printer => "Printer",
        }
    }

    /// The manuals of the device's units (help files), as their Help buttons open them on the original.
    fn manuals(self) -> &'static [&'static str] {
        match self {
            Self::VhfDsc => &["vhf.hlp", "vhf_dsc.hlp"],
            Self::MfHfDsc => &["hf.hlp", "hf_dsc.hlp"],
            Self::Radiotelex => &["hf_tlx.hlp", "telex.hlp"],
            Self::InmarsatA => &["inma.hlp"],
            Self::InmarsatC => &["capsat.hlp", "capsat_s.hlp"],
            Self::Navtex => &["navtex.hlp"],
            Self::Epirb => &["cospas.hlp"],
            Self::Sart => &["sart.hlp"],
            Self::Portable => &["sp3110.hlp"],
            Self::R501 => &["r501.hlp"],
            Self::Power => &["elec.hlp"],
            Self::Printer => &[],
        }
    }

    /// What lessons call the device and the units on it (the first is the device itself).
    fn lesson_names(self) -> &'static [&'static str] {
        match self {
            Self::VhfDsc => &["VHF", "VHF_DSC"],
            Self::MfHfDsc => &["HF", "HF_DSC"],
            // The original's radiotelex station: DSC / telex unit RM2151, telex modem, terminal.
            Self::Radiotelex => &["HF_Tlx", "Msg_Tlx", "Tlx_Mon"],
            Self::InmarsatA => &["Inmarsat_A"],
            Self::InmarsatC => &["Inm", "InmMsg", "Inm_Mon"],
            Self::Navtex => &["Navtex"],
            Self::Epirb => &["EPIRB"],
            Self::Sart => &["SART"],
            Self::Portable => &["VHF_Portable"],
            Self::R501 => &["R_501"],
            Self::Power => &["Charger"],
            Self::Printer => &["Printer"],
        }
    }
}

/// State shared by the installations: the stations around the ship and the radio log.
pub struct Shared {
    pub world: gmdss::world::World,
    pub log: Vec<String>,
    /// Own ship position (degrees) and UTC hour, for radio ranges.
    pub ship_pos: (f64, f64),
    pub hour: u8,
    /// The console printer's paper.
    pub printer: String,
}

impl Shared {
    pub fn note(&mut self, clock: &clock::SimClock, text: String) {
        let t = clock.now();
        self.log.push(format!("{:02}:{:02}  {text}", t.hour, t.minute));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Units,
    Chart,
    Tutor,
    Books,
    Options,
    Intercom,
}

struct App {
    link: Option<net_link::Link>,
    /// What we last told the instructor about our ship.
    hello: Option<gmdss::net::Msg>,
    sound: Option<sound::Sound>,
    sound_on: bool,
    sart_flash: bool,
    ship: Ship,
    tab: Tab,
    books: books_page::Books,
    chart: chart_page::Chart,
    tutor: tutor_page::Tutor,
    clock: clock::SimClock,
    shared: Shared,
    hf: hf_page::HfStation,
    units: units_page::Units,
    inmarsat: inmarsat_page::Inmarsat,
    subs_a: Vec<Subscriber>,
    data: Result<Data, String>,
    device: Device,
    terminal: telex_terminal::TelexTerminal,
    vhf: vhf_page::VhfStation,
}

impl App {
    fn new(dir: Option<PathBuf>, link: Option<net_link::Link>) -> App {
        let data = match dir {
            Some(d) => load(&d).map_err(|e| format!("Cannot read {}: {e}", d.display())),
            None => Err("Original data folder not found. Start with: gmdss-app <path to _gmdss/data>".into()),
        };
        let subscribers = data.as_ref().map(|d| d.subscribers.clone()).unwrap_or_default();
        let ntx = data.as_ref().map(|d| d.ntx.clone()).unwrap_or_default();
        let help_dir = data.as_ref().ok().and_then(|d| d.dir.parent().map(|p| p.join("help")));
        // The original propagation tables, when present (else the own model in `propagation`).
        if let Some(e) = data.as_ref().ok().and_then(|d| std::fs::read(d.dir.join("ETHER.DAT")).ok()).and_then(|b| gmdss::propagation::Ether::parse(&String::from_utf8_lossy(&b))) {
            gmdss::propagation::use_ether(e);
        }
        let ship = default_ship();
        App {
            link,
            hello: None,
            sound: if cfg!(test) { None } else { sound::Sound::new() },
            sound_on: true,
            sart_flash: false,
            tab: Tab::Units,
            books: books_page::Books::new(help_dir),
            tutor: tutor_page::Tutor::new(),
            chart: chart_page::Chart::new(
                data.as_ref().ok().and_then(|d| std::fs::read(d.dir.join("../chart/COAST_RS.DAT")).ok()).and_then(|b| gmdss::coast_rs::parse(&b)).unwrap_or_default(),
                &ship,
            ),
            clock: clock::SimClock::new(),
            shared: Shared {
                world: gmdss::world::World::new(gmdss::world::CoastDsc { name: "HAIFA RADIO".into(), mmsi: "004281000".into(), pos: (32.82, 35.0) }),
                log: vec![],
                ship_pos: (0.0, 0.0),
                hour: 0,
                printer: String::new(),
            },
            hf: hf_page::HfStation::new(&ship),
            units: units_page::Units::new(ntx),
            inmarsat: inmarsat_page::Inmarsat::new(&ship, data.as_ref().map(|d| d.dir.clone()).unwrap_or_default()),
            subs_a: subscribers.clone(),
            data, device: Device::VhfDsc, terminal: telex_terminal::TelexTerminal::new(subscribers), vhf: vhf_page::VhfStation::new(&ship),
            ship,
        }
    }
}

const CONSOLE: Color32 = Color32::from_rgb(52, 72, 74);
const PANEL: Color32 = Color32::from_rgb(32, 44, 46);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

impl App {
    /// An exercise event (from the Intercom page or the instructor).
    fn exercise(&mut self, id: &str) {
        use gmdss::dsc::{Call, Nature, Position};
        let (clock, shared) = (&self.clock, &mut self.shared);
        let now = clock.secs();
        let t = clock.now();
        // The ship in distress is about 10 nm north of us.
        let (lat, lon) = shared.ship_pos;
        let pos = Some(Position { lat_min: ((lat + 0.17) * 60.0) as i32, lon_min: (lon * 60.0) as i32 });
        match id {
            "vhf_distress" => {
                shared.world.inject(Call::distress("211234560", Nature::Fire, pos, Some((t.hour, t.minute))), now + 2.0);
                shared.note(clock, "Exercise: VHF DSC distress alert from 211234560 sent".into());
            }
            "mf_distress" => {
                let mut c = Call::distress("636091234", Nature::Sinking, pos, Some((t.hour, t.minute)));
                c.freq_khz = Some(2187.5);
                shared.world.inject(c, now + 2.0);
                shared.note(clock, "Exercise: MF DSC distress alert from 636091234 sent".into());
            }
            "two_tone" => {
                self.units.r501.alarm_signal();
                shared.note(clock, "Exercise: two-tone alarm on 2182 kHz".into());
            }
            "sart_radar" => {
                self.units.sart.radar_pulse();
                shared.note(clock, "Exercise: radar pulse at the SART".into());
            }
            "navtex_sar" => {
                let m = gmdss::navtex::Message { b1: 'H', b2: 'D', number: 0, text: "SAR EXERCISE\nPERSON OVERBOARD REPORTED 5 NM NORTH OF YOUR POSITION.\nKEEP SHARP LOOKOUT AND REPORT TO MRCC.".into() };
                if self.units.navtex.receive(&m) {
                    shared.note(clock, "Exercise: NAVTEX SAR message printed".into());
                }
            }
            "shore_telex" => {
                let now_c = gmdss::inmarsat_c::Now { secs: now, hour: t.hour, minute: t.minute, day: t.day, month: t.month, year: t.year };
                let les = self.inmarsat.term.les;
                self.inmarsat.term.receive("SHORE.TLX", "FM OWNERS\nPLEASE CONFIRM ETA NEXT PORT\nBRGDS\n", &now_c, les);
                shared.note(clock, "Exercise: shore telex delivered to Inmarsat-C".into());
            }
            _ => shared.note(clock, format!("Unknown exercise `{id}`")),
        }
    }

    /// The classroom network: tell the instructor who we are, pass on what we transmit, take in
    /// the other ships' calls (if heard here) and the instructor's exercises.
    fn network(&mut self) {
        use gmdss::net::Msg;
        let Some(link) = &mut self.link else {
            self.shared.world.sent.clear();
            return;
        };
        let pos = (self.ship.lat_min, self.ship.lon_min);
        if !link.instructor {
            let hello = Msg::Hello { name: self.ship.name.clone(), mmsi: self.ship.mmsi.clone(), lat_min: pos.0, lon_min: pos.1 };
            if self.hello.as_ref() != Some(&hello) {
                link.send(&hello, None);
                self.hello = Some(hello);
            }
        }
        for call in std::mem::take(&mut self.shared.world.sent) {
            link.send(&Msg::Dsc { lat_min: pos.0, lon_min: pos.1, call }, None);
        }
        let mut exercises = vec![];
        for (from, msg) in link.poll() {
            let who = link.students.iter().find(|s| s.id == from).map_or("?".to_string(), |s| format!("{} ({})", s.name, s.mmsi));
            match msg {
                Msg::Hello { .. } if link.instructor => self.shared.note(&self.clock, format!("Network: student {who} on line")),
                Msg::Dsc { lat_min, lon_min, call } if link.instructor => {
                    self.shared.note(&self.clock, format!("Network: {who} sent {}", dsc_panel::describe(&call)));
                    link.send(&Msg::Dsc { lat_min, lon_min, call }, Some(from));
                }
                Msg::Dsc { lat_min, lon_min, call } => {
                    let from_pos = (f64::from(lat_min) / 60.0, f64::from(lon_min) / 60.0);
                    let khz = call.freq_khz.unwrap_or(156_525.0);
                    if call.from != self.ship.mmsi && gmdss::propagation::heard(from_pos, self.shared.ship_pos, khz, self.shared.hour) {
                        self.shared.world.inject(call, self.clock.secs() + 1.0);
                    }
                }
                Msg::Exercise(id) if !link.instructor => exercises.push(id),
                Msg::Note(t) if link.instructor => self.shared.note(&self.clock, format!("Network: {who}: {t}")),
                _ => {}
            }
        }
        for id in exercises {
            self.exercise(&id);
        }
    }

    /// Sounds the alarm that matters most now, and the SART's beep on each flash.
    fn alarms(&mut self) {
        use sound::Tone;
        let u = &self.units;
        let tone = if !self.sound_on {
            None
        } else if self.vhf.dsc_alarm() || self.hf.dsc_alarm() {
            Some(Tone::Dsc)
        } else if self.hf.two_tone() || (u.r501.on() && u.r501.alarm_open) {
            Some(Tone::TwoTone)
        } else if (u.navtex.on && u.navtex.alarm) || self.inmarsat.arming() {
            Some(Tone::Beeps)
        } else {
            None
        };
        let flash = u.sart.flash();
        if let Some(s) = &mut self.sound {
            s.set(tone);
            if flash && !self.sart_flash && self.sound_on {
                s.chirp();
            }
        }
        self.sart_flash = flash;
    }

    /// One frame of the whole console.
    fn frame(&mut self, ui: &mut egui::Ui) {
        self.clock.tick(ui.input(|i| i.time));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250)); // keep the clock running
        self.shared.ship_pos = (f64::from(self.ship.lat_min) / 60.0, f64::from(self.ship.lon_min) / 60.0);
        self.shared.hour = self.clock.now().hour;
        self.vhf.sync_ship(&self.ship);
        self.hf.sync_ship(&self.ship);
        self.inmarsat.ses.answerback = self.ship.answerback.clone();
        self.vhf.tick(&self.clock, &mut self.shared);
        self.hf.tick(&self.clock, &mut self.shared);
        if let Ok(data) = &self.data {
            self.inmarsat.tick(&self.clock, &self.ship, &data.ces, &mut self.subs_a, &data.egc, &mut self.shared);
        }
        self.tutor.before(ui.ctx(), self.clock.secs());
        let printed = self.inmarsat.printout();
        self.shared.printer.push_str(&printed);
        self.alarms();
        self.network();
        let load = self.vhf.load_a() + self.hf.load_a() + self.inmarsat.load_a();
        self.units.tick(&self.clock, &mut self.shared, load);
        egui::Panel::top("tabs").frame(egui::Frame::new().fill(PANEL).inner_margin(6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, tab) in [("Units View", Tab::Units), ("Chart", Tab::Chart), ("Tutor", Tab::Tutor), ("Books", Tab::Books), ("Options", Tab::Options), ("Intercom", Tab::Intercom)] {
                    let resp = ui.add(egui::Button::new(label).selected(tab == self.tab).min_size(egui::vec2(110.0, 26.0)));
                    // The original's lessons go back to the console with `Icon`.
                    if resp.clicked() || (tab == Tab::Units && panel::hook(&resp, "Console", "Icon")) {
                        self.tab = tab;
                    }
                }
            });
        });
        if let Some((file, topic)) = self.tutor.help_request.take() {
            if self.books.open_topic(&file, &topic) {
                self.tab = Tab::Books;
            }
        }
        if self.tutor.run.is_some() {
            egui::Panel::top("lesson").frame(egui::Frame::new()).show(ui, |ui| self.tutor.strip(ui, self.clock.secs()));
        }
        egui::Panel::right("units").exact_size(220.0).frame(egui::Frame::new().fill(PANEL).inner_margin(8)).show(ui, |ui| {
            clock::show(ui, &mut self.clock, &self.ship);
            ui.checkbox(&mut self.sound_on, RichText::new("Sound (alarms)").small());
            ui.add_space(12.0);
            ui.label(RichText::new("EQUIPMENT").small().color(Color32::GRAY));
            for d in Device::ALL {
                let resp = ui.add(egui::Button::new(d.label()).selected(self.device == d).min_size(egui::vec2(200.0, 24.0)));
                if panel::device(ui, resp.rect, d.lesson_names(), resp.clicked()) {
                    self.device = d;
                    self.tab = Tab::Units;
                }
            }
            if !self.device.manuals().is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("MANUALS").small().color(Color32::GRAY));
                for file in self.device.manuals() {
                    if ui.add(egui::Button::new(books_page::title_of(file.trim_end_matches(".hlp"))).min_size(egui::vec2(200.0, 22.0))).clicked() && self.books.open_file(file) {
                        self.tab = Tab::Books;
                    }
                }
            }
        });
        egui::Panel::bottom("log").default_size(110.0).resizable(true).frame(egui::Frame::new().fill(PANEL).inner_margin(6)).show(ui, |ui| {
            ui.label(RichText::new("RADIO LOG").small().color(Color32::GRAY));
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for l in &self.shared.log {
                    ui.label(RichText::new(l).monospace().size(11.0).color(Color32::from_rgb(200, 210, 200)));
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(CONSOLE).inner_margin(16)).show(ui, |ui| egui::ScrollArea::vertical().show(ui, |ui| match &self.data {
            Err(e) => {
                ui.colored_label(Color32::LIGHT_RED, e);
            }
            Ok(_) if self.tab == Tab::Books => {
                self.books.show(ui);
                if std::mem::take(&mut self.books.chart_request) {
                    self.tab = Tab::Chart;
                }
            }
            Ok(_) if self.tab == Tab::Chart => {
                self.chart.show(ui, &self.ship);
                if let Some((file, topic)) = self.chart.help_request.take() {
                    if self.books.open_topic(&file, &topic) {
                        self.tab = Tab::Books;
                    }
                }
            }
            Ok(_) if self.tab == Tab::Tutor => self.tutor.show(ui, self.clock.secs()),
            Ok(_) if self.tab == Tab::Options => options_page::show_options(ui, &mut self.ship),
            Ok(_) if self.tab == Tab::Intercom => {
                if let Some(id) = options_page::show_intercom(ui, self.link.as_ref()) {
                    if let Some(l) = self.link.as_ref().filter(|l| l.instructor) {
                        l.send(&gmdss::net::Msg::Exercise(id.into()), None);
                    }
                    self.exercise(id);
                }
            }
            Ok(data) => match self.device {
                Device::Radiotelex => {
                    self.hf.show_rm2151(ui, &self.clock, &mut self.shared);
                    ui.add_space(6.0);
                    self.terminal.show(ui, data.stations, &self.ship, &self.clock);
                }
                Device::VhfDsc => self.vhf.show(ui, &self.clock, &mut self.shared),
                Device::MfHfDsc => self.hf.show(ui, &self.clock, &mut self.shared),
                Device::InmarsatA => self.inmarsat.show_a(ui, &self.clock, &self.ship, &data.ces, &mut self.subs_a),
                Device::InmarsatC => self.inmarsat.show_c(ui, &self.clock),
                Device::Navtex => self.units.show_navtex(ui),
                Device::R501 => self.units.show_r501(ui, &self.clock),
                Device::Sart => self.units.show_sart(ui),
                Device::Epirb => self.units.show_epirb(ui),
                Device::Portable => self.units.show_portable(ui),
                Device::Power => self.units.show_charger(ui),
                Device::Printer => {
                    if crate::panel::hook(&ui.button("Tear off the paper"), "Printer", "TEAR_OFF") {
                        self.shared.printer.clear();
                    }
                    egui::Frame::new().fill(Color32::from_rgb(245, 243, 230)).inner_margin(12).show(ui, |ui| {
                        ui.set_width(ui.available_width().min(680.0));
                        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, true]).max_height(560.0).show(ui, |ui| {
                            let text = if self.shared.printer.is_empty() { "(no paper printed yet - Inmarsat-A PRINT ON, Inmarsat-C messages)" } else { &self.shared.printer };
                            ui.label(RichText::new(text).monospace().size(12.5).color(Color32::from_rgb(20, 20, 40)));
                        });
                    });
                }
            },
        }));
        self.tutor.after(ui.ctx(), self.clock.secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A class on this machine: one student's distress alert reaches the other student's radio
    /// and the instructor's log; the instructor's exercise reaches both students.
    #[test]
    fn classroom_consoles() {
        let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../_gmdss/data"));
        if !dir.is_dir() {
            return;
        }
        let teacher_link = net_link::Link::instructor(0).unwrap();
        let addr = format!("127.0.0.1:{}", teacher_link.local_port().unwrap());
        let mut teacher = App::new(Some(dir.clone()), Some(teacher_link));
        let mut a = App::new(Some(dir.clone()), Some(net_link::Link::student(&addr).unwrap()));
        let mut b = App::new(Some(dir), Some(net_link::Link::student(&addr).unwrap()));
        a.ship.mmsi = "211000001".into();
        b.ship.mmsi = "211000002".into();
        b.ship.lat_min += 5; // 5 nm apart: in VHF range
        let rounds = |apps: &mut [&mut App]| {
            for _ in 0..50 {
                for app in apps.iter_mut() {
                    app.shared.ship_pos = (f64::from(app.ship.lat_min) / 60.0, f64::from(app.ship.lon_min) / 60.0);
                    app.network();
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        rounds(&mut [&mut a, &mut b, &mut teacher]);
        assert_eq!(teacher.link.as_ref().unwrap().students.len(), 2);
        let call = gmdss::dsc::Call::distress("211000001", gmdss::dsc::Nature::Fire, None, None);
        a.shared.world.sent.push(call.clone());
        rounds(&mut [&mut a, &mut teacher, &mut b]);
        assert!(teacher.shared.log.iter().any(|l| l.contains("DISTRESS ALERT from 211000001")), "{:?}", teacher.shared.log);
        assert_eq!(b.shared.world.tick(f64::MAX), vec![call]);
        teacher.link.as_ref().unwrap().send(&gmdss::net::Msg::Exercise("vhf_distress".into()), None);
        rounds(&mut [&mut a, &mut b]);
        for s in [&mut a, &mut b] {
            assert!(s.shared.world.tick(f64::MAX).iter().any(|c| c.from == "211234560"));
        }
    }

    /// Every lesson demonstrated on the whole console, on a fast clock: it must finish, and every
    /// control it presses must be on screen.
    #[test]
    fn every_lesson_demonstrates() {
        let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../_gmdss/data"));
        if !dir.is_dir() {
            return;
        }
        for i in 0..tutor_page::LESSONS.len() {
            let mut app = App::new(Some(dir.clone()), None);
            let ctx = egui::Context::default();
            app.tutor.start(&ctx, i, gmdss::script::Mode::Demo, 0.0);
            app.clock.speed = 10.0;
            let mut t = 0.0;
            let mut finished = false;
            for _ in 0..40_000 {
                t += 0.05;
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1180.0, 760.0))), time: Some(t), ..Default::default() };
                let mut out = ctx.run_ui(input, |ui| app.frame(ui));
                out.textures_delta.clear();
                if app.tutor.run.as_ref().is_some_and(|r| r.finished()) {
                    finished = true;
                    break;
                }
            }
            let title = gmdss::script::parse(tutor_page::LESSONS[i]).unwrap().title;
            assert!(finished, "lesson {title:?} did not finish");
            assert!(app.tutor.missing.is_empty(), "lesson {title:?}: controls not on screen {:?}", app.tutor.missing);
            // What each lesson leaves behind, where it can be seen.
            let logged = |s: &str| app.shared.log.iter().any(|l| l.contains(s));
            let ok = match i {
                0 | 5 => logged("distress acknowledgement"),
                4 => logged("acknowledgement from 004281000"),
                6 => app.units.r501.muted,
                7 => app.units.sart.in_bracket && app.units.sart.state == gmdss::sart::State::Off,
                8 => !app.units.epirb.switch_on && app.units.epirb.in_bracket,
                9 => app.units.charger.switch == gmdss::charger::Switch::Auto,
                10 => app.inmarsat.term.distress_active,
                12 => {
                    let v = &app.units.portable;
                    v.on && v.channel == 6 && !v.high_power && v.locked
                }
                _ => true,
            };
            assert!(ok, "lesson {title:?} did not reach its result; log: {:?}", app.shared.log);
        }
    }

    /// Local diagnostic: the original's lessons (decrypted on this machine into `re/tue_plain`,
    /// never part of the project) demonstrated on this console; prints what each one could not find.
    /// `cargo test -p gmdss-app original_lessons -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn original_lessons() {
        let data = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../_gmdss/data"));
        let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../re/tue_plain"));
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".TUE.txt")).collect();
        files.sort();
        let (mut clean, mut total) = (0, 0);
        let mut all_missing: std::collections::BTreeMap<String, usize> = Default::default();
        for f in files {
            let text = String::from_utf8_lossy(&std::fs::read(&f).unwrap()).into_owned();
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            let script = match gmdss::script::parse(&text) {
                Ok(s) => s,
                Err(e) => {
                    println!("{name}: does not parse: {e:?}");
                    continue;
                }
            };
            total += 1;
            let mut app = App::new(Some(data.clone()), None);
            let ctx = egui::Context::default();
            let i = app.tutor.push_lesson(script);
            app.tutor.start(&ctx, i, gmdss::script::Mode::Demo, 0.0);
            app.clock.speed = 10.0;
            let mut t = 0.0;
            let mut finished = false;
            for _ in 0..20_000 {
                t += 0.05;
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1180.0, 760.0))), time: Some(t), ..Default::default() };
                let mut out = ctx.run_ui(input, |ui| app.frame(ui));
                out.textures_delta.clear();
                if app.tutor.run.as_ref().is_some_and(|r| r.finished()) {
                    finished = true;
                    break;
                }
            }
            let mut missing = app.tutor.missing.clone();
            missing.sort();
            missing.dedup();
            for m in &missing {
                *all_missing.entry(m.clone()).or_default() += 1;
            }
            if finished && missing.is_empty() {
                clean += 1;
            }
            println!("{name}: {} missing {:?}", if finished { "finished" } else { "NOT finished" }, missing);
        }
        println!("{clean} of {total} parsed lessons ran clean; missing over all: {all_missing:?}");
        let mut app = App::new(None, None);
        let before = app.tutor.lesson_count();
        app.tutor.load_dir(&dir);
        println!("load_dir: {} lessons added", app.tutor.lesson_count() - before);
        snapshot::render("app_tab_tutor_loaded", egui::vec2(1180.0, 1400.0), |ui| app.tutor.show(ui, 0.0));
    }

    /// Received Inmarsat-C messages print by themselves; Inmarsat-A screen text only with PRINT ON.
    #[test]
    fn printer() {
        let mut app = App::new(None, None);
        app.exercise("shore_telex");
        assert!(app.inmarsat.printout().contains("PLEASE CONFIRM ETA"));
        assert_eq!(app.inmarsat.printout(), "");
        app.inmarsat.ses.screen.push_str("NOT PRINTED
");
        assert_eq!(app.inmarsat.printout(), "");
    }

    /// The whole console, every device and every tab, at the window's size (needs the original
    /// data folder; skipped without it).
    #[test]
    fn whole_app_snapshots() {
        let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../_gmdss/data"));
        if !dir.is_dir() {
            return;
        }
        let mut app = App::new(Some(dir), None);
        let size = egui::vec2(1180.0, 760.0);
        for d in Device::ALL {
            app.device = d;
            app.tab = Tab::Units;
            let name = format!("app_{}", d.lesson_names()[0].to_lowercase());
            snapshot::render(&name, size, |ui| app.frame(ui));
        }
        for (name, tab) in [("chart", Tab::Chart), ("tutor", Tab::Tutor), ("books", Tab::Books), ("options", Tab::Options), ("intercom", Tab::Intercom)] {
            app.tab = tab;
            snapshot::render(&format!("app_tab_{name}"), size, |ui| app.frame(ui));
        }
    }
}

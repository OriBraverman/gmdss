//! Parses the original program's data files when they are present next to the crate
//! (`../_gmdss/data`). They are proprietary and not shipped, so these tests skip without them.

use gmdss::ces_list::{CesList, OceanRegion};
use gmdss::coast_station::CoastStation;
use gmdss::egc::{self, ControlRecord};
use gmdss::subscriber::parse_subscribers;
use gmdss::telex::{self, Ctx, Network, Session, Ship, SimTime};
use std::path::{Path, PathBuf};

fn data_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../_gmdss/data");
    if dir.is_dir() { Some(dir) } else { eprintln!("skipped: {} not found", dir.display()); None }
}

/// Case-insensitive lookup, like DOS.
fn read(dir: &Path, name: &str) -> Vec<u8> {
    let path = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("{name} not found"));
    std::fs::read(path).unwrap()
}

#[test]
fn all_coast_stations_parse() {
    let Some(dir) = data_dir() else { return };
    let mut count = 0;
    for path in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()) {
        if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("cs")) {
            continue;
        }
        let st = CoastStation::parse(&std::fs::read(&path).unwrap());
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(st.is_valid(), "{name}: not a usable station");
        assert!(st.errors.is_empty(), "{name}: unclosed commands on lines {:?}", st.errors);
        let cmds = st.blocks.iter().flat_map(|b| b.exact.iter().chain(&b.prefix)).filter(|e| e.key.is_some()).count();
        eprintln!("{name:14} {:12} {:22} {:6} ch={:2} blocks={:2} commands={cmds}",
                  st.name, st.answerback, st.number, st.channels.len(), st.blocks.len());
        count += 1;
    }
    assert!(count > 0, "no .CS files in {}", dir.display());
}

#[test]
fn ces_list_parses() {
    let Some(dir) = data_dir() else { return };
    let list = CesList::parse(&read(&dir, "INMA.CES"));
    // Row counts per section in the shipped file; every row is accepted.
    let counts: Vec<usize> = OceanRegion::ALL.iter().map(|&r| list.region(r).len()).collect();
    assert_eq!(counts, vec![12, 5, 7, 13]);
    for r in OceanRegion::ALL {
        for c in list.region(r) {
            eprintln!("{:20} {} {:15} {:13} {:26} {}", r.display_name(), c.id, c.name, c.country, c.header_format, c.answerback);
        }
    }
}

#[test]
fn egc_schedule_and_messages() {
    let Some(dir) = data_dir() else { return };
    let records = egc::parse_control(&read(&dir, "INM_EGC.CTL")).unwrap();
    assert_eq!(records.len(), 40); // 39 `EGC` + 1 `Distress_EGC`
    let mut unknown = std::collections::BTreeSet::new();
    for rec in &records {
        let file = match rec {
            ControlRecord::Distress { file, .. } => file,
            ControlRecord::Broadcast(b) => &b.file,
        };
        let text = gmdss::dos::latin1(&read(&dir, file));
        // Fill every placeholder we know about and collect the ones left over.
        let known = ["m_les", "m_ref_no", "m_ncs_date", "m_ncs_time", "m_proc", "d_mes_egc_title",
                     "d_ncs_egc_title", "d_mes_number", "d_distress_nature", "d_lat", "d_lon",
                     "d_course", "d_speed", "d_utc", "d_position_status"];
        let out = egc::render(&text, |n| known.contains(&n).then(|| "<x>".to_string()));
        for w in out.split('\\').skip(1).step_by(2) {
            unknown.insert(format!("{file}: \\{w}\\"));
        }
        if let ControlRecord::Broadcast(b) = rec {
            eprintln!("{file:12} LES {:03} p={} svc={:2} addr={:11} {:02}:{:02} n={}", b.les, b.priority, b.service, b.address, b.time.0, b.time.1, b.number);
        }
    }
    // Only the misspelled placeholder in EGC.018 stays unfilled.
    assert_eq!(unknown.into_iter().collect::<Vec<_>>(), vec![r"EGC.018: \m_reg_no\".to_string()]);
}

fn load_stations(dir: &Path) -> Vec<(String, CoastStation)> {
    let mut v: Vec<(String, CoastStation)> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cs")))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), CoastStation::parse(&std::fs::read(&p).unwrap())))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn ship() -> Ship {
    Ship { name: "SEA STAR".into(), answerback: "1234567 SEASTAR X".into() }
}

#[test]
fn telex_conversation_with_bern() {
    let Some(dir) = data_dir() else { return };
    let stations = load_stations(&dir);
    let all: Vec<CoastStation> = stations.iter().map(|s| s.1.clone()).collect();
    let mut subs = vec![];
    assert_eq!(parse_subscribers(&read(&dir, "ABTEL.EU"), &mut subs), Vec::<usize>::new());
    let number = subs[1].number.clone();
    let mut net = Network::new(subs);
    let mut rnd = |n: u16| n / 2;
    let mut ctx = Ctx { now: SimTime { seconds: 0, year: 1997, month: 2, day: 14, hour: 11, minute: 5 }, net: &mut net, random: &mut rnd };
    let bern = &stations.iter().find(|s| s.0 == "BERN.CS").unwrap().1;
    let (mut s, out) = Session::connect(bern, ship(), telex::window_len(&all), &mut ctx);
    let mut log = format!("<connect>{}", telex::transcript(&out));
    // Dialling a subscriber switches to state 7 (talking to them); `$` asks for their answerback,
    // KKKK ends the call with a summary and returns to state 0, where BRK+ clears the line.
    for typed in ["HELP+", "TEST+", &format!("DIRTLX0{number}"), "$", "KKKK", "BRK+"] {
        ctx.now.seconds += 90;
        log += &format!("\n<ship: {typed}>{}", telex::transcript(&s.type_str(typed, &mut ctx)));
        if typed.starts_with("DIRTLX") {
            assert!(ctx.net.subscribers[1].busy);
        }
    }
    eprintln!("{log}");
    assert!(log.contains("HEC  HELP-FILE"));
    assert!(log.contains("THE QUICK BROWN FOX"));
    assert!(log.contains(&ctx.net.subscribers[1].answerback), "dialled subscriber's answerback is shown");
    assert!(log.contains("DURATION: <MIN> MIN"));
    assert!(!ctx.net.subscribers[1].busy, "KKKK released the subscriber");
    assert!(log.ends_with("<CLEAR>\n\n GA+?")); // BERN.CS has a blank line after `'BRK+' \-`
}

/// Every command of every station, each in a fresh session: nothing may panic, and every
/// exact command must produce some output.
#[test]
fn telex_every_command_of_every_station() {
    let Some(dir) = data_dir() else { return };
    let stations = load_stations(&dir);
    let all: Vec<CoastStation> = stations.iter().map(|s| s.1.clone()).collect();
    let window = telex::window_len(&all);
    let mut subs = vec![];
    parse_subscribers(&read(&dir, "ABTEL.EU"), &mut subs);
    let mut runs = 0;
    for (name, st) in &stations {
        for block in &st.blocks {
            for (entries, exact) in [(&block.exact, true), (&block.prefix, false)] {
                for key in entries.iter().filter_map(|e| e.key.as_ref()) {
                    let mut net = Network::new(subs.clone());
                    let mut rnd = |_n: u16| 0;
                    let mut ctx = Ctx { now: SimTime { seconds: 600, year: 1997, month: 2, day: 14, hour: 11, minute: 5 }, net: &mut net, random: &mut rnd };
                    let (mut s, _) = Session::connect(st, ship(), window, &mut ctx);
                    // reach the block's state first: type nothing if it is the current one
                    if s.state() != block.kind {
                        continue;
                    }
                    let typed = if exact { key.clone() } else { format!("{key}X+") };
                    let out = s.type_str(&typed, &mut ctx);
                    assert!(!out.is_empty(), "{name}: `{typed}` printed nothing");
                    runs += 1;
                }
            }
        }
    }
    eprintln!("{runs} commands run");
    assert!(runs > 300);
}

#[test]
fn coast_radio_station_positions() {
    let Some(dir) = data_dir() else { return };
    let s = gmdss::coast_rs::parse(&std::fs::read(dir.join("../chart/COAST_RS.DAT")).unwrap()).unwrap();
    assert_eq!(s.len(), 198);
    let haifa = s.iter().find(|x| x.name == "HAIFA").unwrap();
    assert!((haifa.lat_min / 60.0 - 32.8).abs() < 0.5 && (haifa.lon_min / 60.0 - 35.0).abs() < 0.5, "{haifa:?}");
}

/// Every help file: links point at topics, links and pictures sit inside the text, every picture
/// decodes, and the topics the lessons name exist.
#[test]
fn all_help_files_parse() {
    let Some(dir) = data_dir() else { return };
    let help = dir.join("../help");
    let (mut topics, mut links, mut pictures) = (0, 0, 0);
    for path in std::fs::read_dir(&help).unwrap().map(|e| e.unwrap().path()) {
        let h = gmdss::help::parse(&std::fs::read(&path).unwrap()).unwrap_or_else(|| panic!("{}", path.display()));
        for t in &h.topics {
            let len = t.text.chars().count();
            for l in &t.links {
                assert!(l.target < h.topics.len() && l.start + l.len <= len, "{}: {:?} in {:?}", path.display(), l, t.title);
                let shown: String = t.text.chars().skip(l.start).take(l.len).collect();
                assert!(!shown.trim().is_empty() && !shown.contains('\n'), "{}: link on {shown:?} in {:?}", path.display(), t.title);
            }
            for &(at, p) in &t.pictures {
                assert!(at <= len && p < h.pictures.len(), "{}: picture {p} in {:?}", path.display(), t.title);
            }
            links += t.links.len();
        }
        for p in &h.pictures {
            assert!(p.width > 0 && p.pixels.len() == p.width * p.height, "{}: picture", path.display());
        }
        topics += h.topics.len();
        pictures += h.pictures.len();
    }
    assert!(topics > 5000 && links > 8000 && pictures > 700, "{topics} {links} {pictures}");
    for e in gmdss::script::parse_index(include_str!("../lessons/index.txt")) {
        let Some((file, topic)) = e.help else { continue };
        let h = gmdss::help::parse(&read(&help, &file)).unwrap();
        assert!(h.named(&topic).is_some(), "{:?}: no topic {topic} in {file}", e.title);
    }
}

/// The original propagation tables: VHF about 30 nm, MF farther at night, 8 MHz across oceans.
#[test]
fn ether_tables() {
    let Some(dir) = data_dir() else { return };
    let e = gmdss::propagation::Ether::parse(&String::from_utf8_lossy(&read(&dir, "ETHER.DAT"))).unwrap();
    assert_eq!(e.bands.len(), 9);
    // Every row's first point lies under the band's spreading curve: absorption, not gain.
    let rows: Vec<_> = e.bands.iter().flat_map(|b| &b.rows).collect();
    assert_eq!(rows.len(), 124);
    assert!(rows.iter().all(|r| r.k > 0.0 && r.k.is_finite()), "{rows:?}");
    let heard = |khz: f64, hour: f64, nm: f64| e.level(khz, hour, nm, e.ref_watts(khz).unwrap()).unwrap() > 0.0;
    assert!(heard(156_525.0, 12.0, 25.0) && !heard(156_525.0, 12.0, 35.0));
    // With the DSC receiver's noise the VHF call fades out a little before the table's 30 nm.
    let vhf_noise = gmdss::propagation::noise(gmdss::propagation::dsc_bandwidth_hz(156_525.0));
    let level = |nm: f64| e.level(156_525.0, 12.0, nm, 25.0).unwrap();
    assert!(gmdss::propagation::decoded(&[level(29.5)], vhf_noise).is_some());
    assert!(gmdss::propagation::decoded(&[level(29.9)], vhf_noise).is_none());
    assert!(heard(2187.5, 4.0, 400.0) && !heard(2187.5, 12.0, 400.0));
    assert!(heard(8414.5, 3.0, 4000.0) && !heard(8414.5, 15.0, 4000.0));
}

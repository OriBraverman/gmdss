# GMDSS training station

An open-source GMDSS radio-operator trainer in Rust: the equipment of a ship's radio console
(VHF and MF/HF radiotelephones with DSC, radiotelex, Inmarsat-A and -C, NAVTEX, EPIRB, SART,
portable VHF, 2182 kHz watch receiver, battery charger), a chart, lessons with exam mode, and
exercise events.

It re-implements the behaviour of a 1996 DOS trainer (Transas "SAILOR GMDSS Training Station")
from its documentation, its data formats and a study of how the program behaves. No code, text,
pictures or data of the original are part of this repository.

## Run

```
cargo run --release -p gmdss-app -- <path to the original data folder>
```

The app needs coast station scripts (`*.CS`), telex subscribers (`*.EU`), Inmarsat earth
stations (`INMA.CES`) and the EGC schedule (`INM_EGC.CTL` + `EGC.nnn`). These come from an
original installation and are not distributed here; without them the app starts with a message.
Used when present: `ETHER.DAT` (radio propagation tables), the `help` folder next to `data`
(Books) and `chart/COAST_RS.DAT` (station positions on the chart).
Lessons (`lessons/*.txt`) and the demo NAVTEX broadcasts are part of this project.
More lessons in the same script language (one `*.txt` per lesson, optional index `SCE.TUD.txt` in
the `lessons/index.txt` format) are added with `--lessons <folder>`, for example your own copies of
the original program's lessons as plain text; they are listed under their index groups.

## Layout

| Path | What |
|---|---|
| `src/` (crate `gmdss`) | Simulation core, no UI: file formats, units, DSC, telex, tutor engine |
| `app/` (crate `gmdss-app`) | eframe/egui desktop application |
| `app/assets/panels/` | Every unit as SVG (own drawings of the static parts; element ids for the controls, lamps and read-outs the app draws). Edit in Inkscape; `cargo test` renders all of them to `target/snapshots/` |
| `app/assets/fonts/` | DejaVu Sans (Bitstream Vera licence, see `LICENSE_DEJAVU.txt`) |
| `app/assets/chart/land.txt` | Land for the chart from [Natural Earth](https://www.naturalearthdata.com/) 1:50m (public domain), made by `tools/land_asset.py` from `ne_50m_land.geojson` |
| `lessons/` | Lesson scripts and `index.txt` (format in `src/script.rs`) |
| `tests/original_data.rs` | Checks against the original data files when they are present |

Core modules: `coast_station` `telex` `subscriber` (HF radiotelex), `vhf` `dsc` (VHF/MF/HF DSC),
`hf` (MF/HF SSB), `navtex`, `epirb`, `sart`, `portable`, `r501`, `charger`, `inmarsat_a`,
`inmarsat_c` `egc` `ces_list`, `coast_rs` (station positions), `propagation`, `world`
(coast station that answers), `script` (lesson player: demonstration, practice, exam), `help`
(original help files on the Books shelf).

## Status

Faithful to the original where its logic could be studied (telex scripts, DSC texts and timing,
channel tables, NAVTEX menus, data formats, the lesson script language and its demonstration /
exam modes). The lessons themselves are our own, written in that script structure. Own designs:
Inmarsat-C terminal internals, chart drawing (the original chart is a proprietary vector chart),
radio propagation when `ETHER.DAT` is missing (with it, the original signal-level model and
receiver noise are used), the classroom network protocol. The Books read the original help files
(links, pictures, English and Russian text) at run time.

## Classroom

One console teaches, the others join it over the network (TCP):

```
cargo run --release -p gmdss-app -- <data folder> --instructor 5000
cargo run --release -p gmdss-app -- <data folder> --student 192.168.1.10:5000
```

The instructor sees the class on the Intercom page and sends exercise events to every student;
DSC calls a student sends reach the other students' radios when they are within range.

Sound: alarms are generated tones (no sound files); on Linux the audio backend needs the ALSA
development package (`libasound2-dev`) to build.

## Test

```
cargo test --workspace
```

## License

MIT (see `LICENSE`) for the code, artwork and lessons of this project. Bundled third-party data:
DejaVu fonts (see `app/assets/fonts/LICENSE_DEJAVU.txt`) and Natural Earth land (public domain).
The original program and its data are not part of this repository and are not covered by it.

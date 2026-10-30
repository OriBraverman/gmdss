//! Help files ("Transas Marine Help File Version 3.11", `*.HLP`) shown on the Books shelf.
//!
//! ```text
//! header text, ^Z, u8, u16 n
//! n x {u32 offset, u8 kind}       topics; kind & 3: 1 text with links, 2 plain text, 3 picture page
//! u16 p, p x u32 offset           pictures
//! u16 len, len bytes              topic names, NUL terminated, one per topic ("" = none)
//! topic kind 1: u16 len, u16 next, u16 prev, u8 links, u8 pictures, u16, u8 title length, text,
//!               links {u16 line start, u8 line, u8 column, u16 topic, u8 len},
//!               pictures {u16 line start, u8 line, u8 column, u16 picture}  (column: chars into the line)
//! topic kind 2: u16 len, u16 lines, text
//! topic kind 3: u16 len, u8 pictures, text, pictures
//! picture:      u16 stored, u16 size, data; stored < size: bit runs (`unpack`) and a u16 check
//!   1 plane:    u8 1, u8 header (1: 6 bytes, 2: 10), .., [5] bytes per row; rows of bits (1 = ink)
//!   4 planes:   u8 4, u16 w-1, u16 h-1; per row 4 slices, planes 3, 2, 1, 0 (colour index bits)
//! ```
//! Text is code page 866 (the manuals are English or Russian); positions count bytes, one per char.

/// The program's VGA palette (6-bit DAC values scaled to 8 bits), which the pictures index.
pub const PALETTE: [[u8; 3]; 16] = {
    const DAC: [[u8; 3]; 16] = [
        [0x00, 0x00, 0x00], [0x1c, 0x1c, 0x1c], [0x16, 0x1f, 0x18], [0x19, 0x26, 0x1c],
        [0x21, 0x2a, 0x20], [0x29, 0x31, 0x28], [0x38, 0x3c, 0x38], [0x38, 0x38, 0x38],
        [0x2c, 0x2c, 0x2c], [0x3a, 0x3a, 0x3c], [0x2c, 0x2e, 0x18], [0x3f, 0x10, 0x08],
        [0x00, 0x3a, 0x2c], [0x3f, 0x3a, 0x00], [0x37, 0x00, 0x00], [0x3f, 0x3f, 0x3f],
    ];
    let mut p = [[0; 3]; 16];
    let mut i = 0;
    while i < 48 {
        p[i / 3][i % 3] = (DAC[i / 3][i % 3] as u16 * 255 / 63) as u8;
        i += 1;
    }
    p
};

/// Pixels are palette indexes; `CLEAR` is the page showing through.
pub const CLEAR: u8 = 0xff;

pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// A link: `len` chars of the text from `start` lead to topic `target`.
#[derive(Debug, PartialEq)]
pub struct Link {
    pub start: usize,
    pub len: usize,
    pub target: usize,
}

pub struct Topic {
    /// The name lessons refer to (`help file topic`); often empty.
    pub name: String,
    pub title: String,
    pub text: String,
    pub links: Vec<Link>,
    /// (text position, picture index).
    pub pictures: Vec<(usize, usize)>,
}

pub struct HelpFile {
    pub topics: Vec<Topic>,
    pub pictures: Vec<Picture>,
}

impl HelpFile {
    pub fn named(&self, name: &str) -> Option<usize> {
        self.topics.iter().position(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// The topics that have Russian text, in the translation format: `@@ n` (topic number) on a
    /// line, then the text with its links as `[[target|text]]` and pictures as `{{picture}}`.
    pub fn russian(&self) -> String {
        let mut s = String::new();
        for (n, t) in self.topics.iter().enumerate().filter(|(_, t)| t.text.chars().any(|c| ('\u{410}'..='\u{44f}').contains(&c))) {
            s += &format!("@@ {n}\n{}\n", marked(t));
        }
        s
    }

    /// Replaces topics by their translations (the `russian` format); returns how many.
    pub fn translate(&mut self, file: &str) -> usize {
        let mut done = 0;
        for part in file.split("\n@@ ").map(|p| p.strip_prefix("@@ ").unwrap_or(p)) {
            let Some((n, body)) = part.split_once('\n') else { continue };
            let Some(t) = n.trim().parse::<usize>().ok().and_then(|n| self.topics.get_mut(n)) else { continue };
            (t.text, t.links, t.pictures) = unmarked(body.trim_end_matches(['\r', '\n']));
            t.title = t.text.lines().map(str::trim).find(|l| l.chars().count() > 2).unwrap_or(&t.name).to_string();
            done += 1;
        }
        done
    }
}

/// A topic's text with its links as `[[target|text]]` and its pictures as `{{picture}}`.
fn marked(t: &Topic) -> String {
    let chars: Vec<char> = t.text.chars().collect();
    let mut s = String::new();
    let mut close = None;
    for i in 0..=chars.len() {
        if close == Some(i) {
            s += "]]";
            close = None;
        }
        for (_, p) in t.pictures.iter().filter(|(at, _)| *at == i) {
            s += &format!("{{{{{p}}}}}");
        }
        if let Some(l) = t.links.iter().find(|l| l.start == i && l.len > 0).filter(|_| close.is_none()) {
            s += &format!("[[{}|", l.target);
            close = Some(i + l.len);
        }
        if let Some(c) = chars.get(i) {
            s.push(*c);
        }
    }
    s
}

/// The text, links and pictures of a `marked` text.
fn unmarked(m: &str) -> (String, Vec<Link>, Vec<(usize, usize)>) {
    let (mut text, mut links, mut pictures) = (String::new(), vec![], vec![]);
    let (mut n, mut open, mut rest) = (0, None, m);
    while let Some(c) = rest.chars().next() {
        if let Some((target, after)) = rest.strip_prefix("[[").and_then(|r| r.split_once('|')).and_then(|(t, a)| Some((t.parse().ok()?, a))) {
            open = Some((n, target));
            rest = after;
        } else if let Some((after, (start, target))) = rest.strip_prefix("]]").zip(open) {
            links.push(Link { start, len: n - start, target });
            open = None;
            rest = after;
        } else if let Some((k, after)) = rest.strip_prefix("{{").and_then(|r| r.split_once("}}")).and_then(|(k, a)| Some((k.parse().ok()?, a))) {
            pictures.push((n, k));
            rest = after;
        } else {
            text.push(c);
            n += 1;
            rest = &rest[c.len_utf8()..];
        }
    }
    (text, links, pictures)
}

const CP866_HIGH: &str = "АБВГДЕЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдежзийклмноп░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀рстуфхцчшщъыьэюяЁёЄєЇїЎў°∙·√№¤■\u{a0}";

/// One char per byte, so text positions stay byte positions; control bytes become spaces.
fn text(b: &[u8]) -> String {
    let high: Vec<char> = CP866_HIGH.chars().collect();
    b.iter()
        .map(|&c| match c {
            b'\n' | b'\t' => c as char,
            0..32 => ' ',
            32..128 => c as char,
            _ => high[usize::from(c - 128)],
        })
        .collect()
}

fn u16_at(b: &[u8], i: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?])))
}

fn u32_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}

/// Alternating runs of 0 and 1 bits, one byte each; a run of 255 goes on in the next byte.
fn unpack(runs: &[u8], size: usize) -> Vec<u8> {
    let mut out = vec![0u8; size];
    let (mut bit, mut one) = (0, false);
    for &n in runs {
        let end = (bit + usize::from(n)).min(size * 8);
        if one {
            for b in bit..end {
                out[b / 8] |= 0x80 >> (b % 8);
            }
        }
        bit += usize::from(n);
        if n != 255 {
            one = !one;
        }
    }
    out
}

fn picture(b: &[u8], at: usize) -> Option<Picture> {
    let (stored, size) = (u16_at(b, at)?, u16_at(b, at + 2)?);
    let data = b.get(at + 4..at + 4 + stored)?;
    let d = if stored == size { data.to_vec() } else { unpack(&data[..stored.checked_sub(2)?], size) };
    let bit = |row: &[u8], x: usize| row[x / 8] >> (7 - x % 8) & 1;
    match *d.first()? {
        1 => {
            let head = if *d.get(1)? == 2 { 10 } else { 6 };
            let bpr = usize::from(*d.get(5)?).max(1);
            let rows: Vec<&[u8]> = d.get(head..)?.chunks_exact(bpr).collect();
            let pixels = rows.iter().flat_map(|r| (0..bpr * 8).map(|x| if bit(r, x) == 1 { 0 } else { CLEAR })).collect();
            Some(Picture { width: bpr * 8, height: rows.len(), pixels })
        }
        4 => {
            let (w, h) = (u16_at(&d, 1)? + 1, u16_at(&d, 3)? + 1);
            let bpr = w.div_ceil(8);
            let mut pixels = Vec::with_capacity(w * h);
            for y in 0..h {
                let row = d.get(5 + y * 4 * bpr..5 + (y + 1) * 4 * bpr)?;
                pixels.extend((0..w).map(|x| (0..4).map(|k| bit(&row[k * bpr..], x) << (3 - k)).sum::<u8>()));
            }
            Some(Picture { width: w, height: h, pixels })
        }
        _ => None,
    }
}

fn topic(b: &[u8], at: usize, kind: u8, name: String) -> Option<Topic> {
    let t = b.get(at..at + u16_at(b, at)?)?;
    let (head, links, pics, title_len) = match kind & 3 {
        1 => (11, usize::from(t[6]), usize::from(t[7]), usize::from(t[10])),
        3 => (3, 0, usize::from(*t.get(2)?), 0),
        _ => (4, 0, 0, 0),
    };
    let end = t.len().checked_sub(7 * links + 6 * pics).filter(|&e| e >= head)?;
    let text = text(&t[head..end]);
    let word = |i: usize| usize::from(t[i]) | usize::from(t[i + 1]) << 8;
    let at = |r: usize| word(r) + usize::from(t[r + 3]);
    let pictures = (0..pics).map(|i| end + 7 * links + 6 * i).map(|r| (at(r), word(r + 4))).collect();
    let links = (0..links).map(|i| end + 7 * i).map(|r| Link { start: at(r), len: usize::from(t[r + 6]), target: word(r + 4) }).collect();
    let title = text.chars().take(title_len).collect::<String>().trim().to_string();
    let title = if title.is_empty() { text.lines().map(str::trim).find(|l| l.chars().count() > 2).unwrap_or(&name).to_string() } else { title };
    Some(Topic { name, title, text, links, pictures })
}

/// The topics (in file order, which links refer to) and pictures of a help file.
pub fn parse(b: &[u8]) -> Option<HelpFile> {
    if !b.starts_with(b"Transas Marine Help File") {
        return None;
    }
    let z = b.iter().position(|&c| c == 0x1a)?;
    let n = u16_at(b, z + 2)?;
    let table: Vec<(usize, u8)> = (0..n).map(|i| Some((u32_at(b, z + 4 + 5 * i)?, *b.get(z + 8 + 5 * i)?))).collect::<Option<_>>()?;
    let p = z + 4 + 5 * n;
    let np = u16_at(b, p)?;
    let pictures = (0..np).map(|i| u32_at(b, p + 2 + 4 * i).and_then(|o| picture(b, o))).map(|p| p.unwrap_or(Picture { width: 0, height: 0, pixels: vec![] })).collect();
    let q = p + 2 + 4 * np;
    let names = b.get(q + 2..q + 2 + u16_at(b, q)?)?;
    let mut names = names.split(|&c| c == 0).map(|s| String::from_utf8_lossy(s).into_owned());
    let topics = table
        .iter()
        .map(|&(o, kind)| {
            let name = names.next().unwrap_or_default();
            topic(b, o, kind, name.clone()).unwrap_or(Topic { title: name.clone(), name, text: String::new(), links: vec![], pictures: vec![] })
        })
        .collect();
    Some(HelpFile { topics, pictures })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Russian topics out in the translation format, a translation back in with its links.
    #[test]
    fn translation() {
        let t = |text: &str, links, pictures| Topic { name: String::new(), title: String::new(), text: text.into(), links, pictures };
        let mut h = HelpFile {
            topics: vec![t("English\n", vec![], vec![]), t("Смотри рисунок\nДалее", vec![Link { start: 15, len: 5, target: 0 }], vec![(6, 3)])],
            pictures: vec![],
        };
        let ru = h.russian();
        assert_eq!(ru, "@@ 1\nСмотри{{3}} рисунок\n[[0|Далее]]\n");
        assert_eq!(unmarked(&ru[5..ru.len() - 1]), (h.topics[1].text.clone(), vec![Link { start: 15, len: 5, target: 0 }], vec![(6, 3)]));
        assert_eq!(h.translate("@@ 1\r\nSee{{3}} the picture\r\n[[0|Next]]\r\n@@ 9\nnone\n"), 1);
        let s = &h.topics[1];
        assert_eq!((s.text.as_str(), s.title.as_str()), ("See the picture\r\nNext", "See the picture"));
        assert_eq!((s.links[0].start, s.links[0].len, s.pictures[0]), (17, 4, (3, 3)));
    }

    #[test]
    fn topics_links_and_pictures() {
        // Two topics: a text topic with a link to the picture page, and the picture page.
        let mut f = b"Transas Marine Help File Version 3.11\r\n\x1a\x00\x02\x00".to_vec();
        let table = f.len();
        f.extend([0; 10]);
        // One packed picture: 1 plane, 8x2, rows 0xF0 0x0F (header 01 01 00 00 02 01).
        let pic = [0x01, 0x01, 0x00, 0x00, 0x02, 0x01, 0xf0, 0x0f];
        let mut runs = vec![];
        let (mut v, mut n) = (0u8, 0u8);
        for byte in pic {
            for k in 0..8 {
                let b = byte >> (7 - k) & 1;
                if b != v {
                    runs.push(n);
                    (v, n) = (b, 0);
                }
                n += 1;
            }
        }
        runs.push(n);
        let pictures = f.len();
        f.extend([1, 0, 0, 0, 0, 0]);
        f.extend([7, 0, b'm', b'a', b'i', b'n', 0, 0, 0]);
        let t0 = f.len();
        let body = b"\t\tC O N T E N T S\nSee the picture\n";
        let len = 11 + body.len() + 7;
        f.extend([len as u8, 0, 1, 0, 0xff, 0xff, 1, 0, 1, 0, 18]);
        f.extend(body);
        f.extend([18, 0, 1, 0, 1, 0, 15]);
        let t1 = f.len();
        f.extend([3 + 2 + 6, 0, 1, b'\n', 0xa0, 1, 0, 1, 0, 0, 0]);
        let pic_at = f.len();
        f.extend([(runs.len() + 2) as u8, 0, pic.len() as u8, 0]);
        f.extend(&runs);
        f.extend([0, 0]);
        f[table..table + 4].copy_from_slice(&(t0 as u32).to_le_bytes());
        f[table + 4] = 0x81;
        f[table + 5..table + 9].copy_from_slice(&(t1 as u32).to_le_bytes());
        f[table + 9] = 3;
        f[pictures + 2..pictures + 6].copy_from_slice(&(pic_at as u32).to_le_bytes());

        let h = parse(&f).unwrap();
        assert_eq!(h.topics.len(), 2);
        let t = &h.topics[0];
        assert_eq!((t.name.as_str(), t.title.as_str()), ("main", "C O N T E N T S"));
        assert_eq!(t.links, [Link { start: 18, len: 15, target: 1 }]);
        assert_eq!(&t.text[18..33], "See the picture");
        assert_eq!(h.named("MAIN"), Some(0));
        assert_eq!(h.topics[1].text, "\nа");
        assert_eq!(h.topics[1].pictures, [(1, 0)]);
        let p = &h.pictures[0];
        assert_eq!((p.width, p.height), (8, 2));
        assert_eq!(p.pixels[..8], [0, 0, 0, 0, CLEAR, CLEAR, CLEAR, CLEAR]);
        assert!(parse(b"nope").is_none());
    }

    #[test]
    fn long_runs_continue() {
        let out = unpack(&[255, 1, 8], 34);
        assert_eq!(out[31], 0);
        assert_eq!(out[32], 0xff);
    }
}

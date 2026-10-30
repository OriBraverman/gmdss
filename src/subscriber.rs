//! Telex subscribers ashore (`*.EU` files) that a ship can reach through a coast station.
//!
//! ```text
//! ; comment
//! [Owners]
//! Number=1234567+
//! Answerback=34567 OWNER X
//! ```
//! Lines are trimmed of spaces. `[name]` starts a subscriber; `Number=` and `Answerback=` fill in
//! the latest one (other keys are accepted and ignored; no spaces around `=`). Items before the
//! first section of a file are errors. At most 40 subscribers are kept over all files.

use crate::dos::latin1;

pub const MAX_SUBSCRIBERS: usize = 40;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Subscriber {
    pub name: String,
    /// What the ship types after the station's dial command, including the trailing `+`.
    pub number: String,
    pub answerback: String,
    /// Connected to a ship at the moment.
    pub busy: bool,
}

/// Adds the subscribers of one file to `list` and returns the line numbers of invalid lines.
pub fn parse_subscribers(bytes: &[u8], list: &mut Vec<Subscriber>) -> Vec<usize> {
    let mut errors = Vec::new();
    let mut current: Option<usize> = None;
    for (i, raw) in crate::dos::fgets_lines(bytes, 100).iter().enumerate() {
        let raw = &raw[..raw.len().saturating_sub(1)]; // fgets + drop last char, as for `.CS`
        let line = trim_spaces(raw);
        if line.is_empty() || line[0] == b';' {
            continue;
        }
        if line[0] == b'[' {
            let Some(name) = line[1..].strip_suffix(b"]") else {
                errors.push(i + 1);
                continue;
            };
            if list.len() < MAX_SUBSCRIBERS {
                list.push(Subscriber { name: latin1(name), ..Default::default() });
                current = Some(list.len() - 1);
            } else {
                current = None;
            }
            continue;
        }
        match (line.iter().position(|&c| c == b'='), current) {
            (Some(eq), Some(cur)) if eq > 0 => {
                let value = latin1(&line[eq + 1..]);
                match &line[..eq] {
                    b"Number" => list[cur].number = value,
                    b"Answerback" => list[cur].answerback = value,
                    _ => {}
                }
            }
            _ => errors.push(i + 1),
        }
    }
    errors
}

fn trim_spaces(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| c != b' ').unwrap_or(s.len());
    let end = s.iter().rposition(|&c| c != b' ').map_or(start, |e| e + 1);
    &s[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_and_items() {
        let mut list = vec![];
        let text = b"Number=1+\n; c\n[ Harbour Office]  \n  Number=123+\nAnswerback=123 HO X\nColour=red\n\
                     Number = 9\n=x\n[Agent\n[Agent]\nNumber=77+\n";
        let errors = parse_subscribers(text, &mut list);
        assert_eq!(errors, vec![1, 8, 9]);
        assert_eq!(list, vec![
            Subscriber { name: " Harbour Office".into(), number: "123+".into(), answerback: "123 HO X".into(), busy: false },
            Subscriber { name: "Agent".into(), number: "77+".into(), answerback: "".into(), busy: false },
        ]);
    }

    #[test]
    fn limit_over_files() {
        let mut list = vec![Subscriber::default(); MAX_SUBSCRIBERS];
        let errors = parse_subscribers(b"[Late]\nNumber=1+\n", &mut list);
        assert_eq!((list.len(), errors), (MAX_SUBSCRIBERS, vec![2]));
    }
}

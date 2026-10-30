//! Coast station files (`*.CS`): an HF radiotelex coast station's identity, channels and
//! command scripts.
//!
//! Line-oriented DOS text. The first character of each line decides what it is:
//!
//! | line          | meaning                                                          |
//! |---------------|------------------------------------------------------------------|
//! | `;...`        | comment (anywhere, also inside script blocks)                    |
//! | `N name`      | station name                                                     |
//! | `A text`      | answerback, upper-cased, max 20 chars                            |
//! | `Q text`      | station number, upper-cased, max 10 chars                        |
//! | `# tx rx`     | channel: two frequencies in kHz                                  |
//! | `+text`       | opens script block `+`                                           |
//! | `\Xtext`      | opens script block `X` (upper-cased); ignored if `X` is a space  |
//! | anything else | script line, added to the most recent block                      |
//!
//! Script lines: `'KEY' text` adds an exact-match command, `"KEY" text` a prefix-match command,
//! `*text` a prefix command with key `*`. Other lines continue the previous command of the same
//! kind. A block starts in prefix mode with no key. An unclosed quote is reported in
//! [`CoastStation::errors`] and the following continuation lines are dropped until the next command.
//!
//! Header letters are checked on every line, so script text starting with `N`, `A`, `Q`, `#` or `;`
//! is taken as a header line. The original behaves the same: the shipped SYDNEY.CS ends up named
//! "ORTH 18S AND EAST 118E" because a weather report line reads "NORTH 18S AND EAST 118E".

use crate::dos::{fgets_lines, latin1};

/// A parsed coast station file.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CoastStation {
    pub name: String,
    pub answerback: String,
    pub number: String,
    pub channels: Vec<Channel>,
    pub blocks: Vec<Block>,
    /// Line numbers (1-based) of commands with an unclosed `'` or `"`.
    pub errors: Vec<usize>,
}

/// One channel line `# a b`, in the order written (the shipped files label them
/// "Transmits" and "Receives" from the station's point of view).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channel {
    pub transmit_khz: f32,
    pub receive_khz: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// `+`, or the upper-cased character after `\` (`0`, `1`, ...).
    pub kind: u8,
    /// Lines of `'KEY'` commands and their continuations, in file order.
    pub exact: Vec<Entry>,
    /// Lines of `"KEY"` / `*` commands, the block's own text, and their continuations.
    pub prefix: Vec<Entry>,
}

/// One script line. `key` is `Some` on the line that starts a command, `None` on continuations.
/// Empty keys and empty texts are stored as `None`, as the original does.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub line: usize,
    pub key: Option<String>,
    pub text: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Dropped,
    Exact,
    Prefix,
}

impl CoastStation {
    /// The original skips stations without a number or without any channel.
    pub fn is_valid(&self) -> bool {
        !self.number.is_empty() && !self.channels.is_empty()
    }

    /// Parses the raw bytes of a `.CS` file.
    pub fn parse(bytes: &[u8]) -> CoastStation {
        let mut st = CoastStation::default();
        let mut mode = Mode::Prefix; // mode of the most recent block
        for (i, line) in dos_lines(bytes).iter().enumerate() {
            let line_no = i + 1;
            match line.first() {
                Some(b';') => {}
                Some(b'N') => st.name = latin1(skip_one_space(&line[1..])),
                Some(b'A') => st.answerback = upper(skip_one_space(&line[1..]), 20),
                Some(b'Q') => st.number = upper(skip_one_space(&line[1..]), 10),
                Some(b'#') => {
                    if let Some(ch) = parse_channel(&line[1..]) {
                        st.channels.push(ch);
                    }
                }
                Some(b'+') => st.open_block(line, line_no, &mut mode),
                Some(b'\\') => st.open_block(&line[1..], line_no, &mut mode),
                _ => st.script_line(line, line_no, &mut mode),
            }
        }
        st
    }

    fn open_block(&mut self, rest: &[u8], line_no: usize, mode: &mut Mode) {
        let Some(&kind) = rest.first() else { return };
        if kind == b' ' {
            return;
        }
        self.blocks.push(Block { kind: kind.to_ascii_uppercase(), exact: vec![], prefix: vec![] });
        *mode = Mode::Prefix;
        self.script_line(&rest[1..], line_no, mode);
    }

    fn script_line(&mut self, text: &[u8], line: usize, mode: &mut Mode) {
        let Some(block) = self.blocks.last_mut() else { return };
        let (new_mode, key, text) = match text.first() {
            Some(&q @ (b'"' | b'\'')) => match text[1..].iter().position(|&c| c == q) {
                Some(end) => {
                    let m = if q == b'"' { Mode::Prefix } else { Mode::Exact };
                    (m, non_empty(&text[1..1 + end]), &text[2 + end..])
                }
                None => {
                    self.errors.push(line);
                    *mode = Mode::Dropped;
                    return;
                }
            },
            Some(b'*') => (Mode::Prefix, Some("*".to_string()), &text[1..]),
            _ => (*mode, None, text),
        };
        *mode = new_mode;
        let entry = Entry { line, key, text: non_empty(text) };
        match new_mode {
            Mode::Exact => block.exact.push(entry),
            Mode::Prefix => block.prefix.push(entry),
            Mode::Dropped => {}
        }
    }
}

/// Lines as the original reads them: `fgets(buf, 100, f)`, then the last character dropped.
/// That character is the newline, except for a final line without one or a line longer than
/// 98 chars, which really lose a char.
fn dos_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut lines = fgets_lines(bytes, 100);
    for line in &mut lines {
        line.pop();
    }
    lines
}

fn skip_one_space(s: &[u8]) -> &[u8] {
    s.strip_prefix(b" ").unwrap_or(s)
}

fn upper(s: &[u8], max: usize) -> String {
    latin1(&s[..s.len().min(max)]).to_ascii_uppercase()
}

fn non_empty(s: &[u8]) -> Option<String> {
    (!s.is_empty()).then(|| latin1(s))
}

/// `sscanf(s, "%e %e")`: two floats, each after optional whitespace, trailing text ignored.
fn parse_channel(s: &[u8]) -> Option<Channel> {
    let (transmit_khz, rest) = c_float(s)?;
    let (receive_khz, _) = c_float(rest)?;
    Some(Channel { transmit_khz, receive_khz })
}

/// Longest `[+-]digits[.digits][e[+-]digits]` prefix after whitespace, like `%e`.
fn c_float(s: &[u8]) -> Option<(f32, &[u8])> {
    let start = s.iter().position(|c| !c.is_ascii_whitespace())?;
    let s = &s[start..];
    let digits = |from: usize| from + s[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut end = usize::from(matches!(s.first(), Some(b'+' | b'-')));
    let int_end = digits(end);
    let mut seen_digit = int_end > end;
    end = int_end;
    if s.get(end) == Some(&b'.') {
        let frac_end = digits(end + 1);
        seen_digit |= frac_end > end + 1;
        end = frac_end;
    }
    if !seen_digit {
        return None;
    }
    if matches!(s.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(s.get(end + 1), Some(b'+' | b'-')));
        let exp_end = digits(end + 1 + sign);
        if exp_end > end + 1 + sign {
            end = exp_end;
        }
    }
    let value = std::str::from_utf8(&s[..end]).ok()?.parse().ok()?;
    Some((value, &s[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A made-up station exercising every line type (CRLF, then ^Z and junk after it).
    const SAMPLE: &str = r#"; comment
+ 1234 TESTRAD XX
 \*
#  2000.5   2100
# 4000 bad
A 1234 testrad xx with a very long tail
N TEST HARBOUR
Q 123456789012
\0 GO AHEAD+?
'PING+' PONG
 second line
; comments are skipped inside blocks too
"DIAL" CALLING \<
*fallback
'BROKEN
 dropped
''
\ ignored, block not opened
 still in block 0
\1
"#;

    fn sample() -> Vec<u8> {
        let mut b = SAMPLE.replace('\n', "\r\n").into_bytes();
        b.extend_from_slice(b"\x1ajunk after ctrl-z\r\n");
        b
    }

    fn e(line: usize, key: Option<&str>, text: Option<&str>) -> Entry {
        Entry { line, key: key.map(Into::into), text: text.map(Into::into) }
    }

    #[test]
    fn parses_header() {
        let st = CoastStation::parse(&sample());
        assert_eq!(st.name, "TEST HARBOUR");
        assert_eq!(st.answerback, "1234 TESTRAD XX WITH");
        assert_eq!(st.number, "1234567890");
        assert_eq!(st.channels, vec![Channel { transmit_khz: 2000.5, receive_khz: 2100.0 }]);
        assert!(st.is_valid());
        assert_eq!(st.errors, vec![15]);
    }

    #[test]
    fn parses_blocks() {
        let st = CoastStation::parse(&sample());
        assert_eq!(st.blocks.len(), 3);
        let plus = &st.blocks[0];
        assert_eq!(plus.kind, b'+');
        assert_eq!(plus.prefix, vec![e(2, None, Some(" 1234 TESTRAD XX")), e(3, None, Some(" \\*"))]);

        let b0 = &st.blocks[1];
        assert_eq!(b0.kind, b'0');
        assert_eq!(b0.exact, vec![
            e(10, Some("PING+"), Some(" PONG")),
            e(11, None, Some(" second line")),
            e(17, None, None), // '' : empty key and text both become None
            e(19, None, Some(" still in block 0")),
        ]);
        assert_eq!(b0.prefix, vec![
            e(9, None, Some(" GO AHEAD+?")),
            e(13, Some("DIAL"), Some(" CALLING \\<")),
            e(14, Some("*"), Some("fallback")),
        ]);

        assert_eq!(st.blocks[2], Block { kind: b'1', exact: vec![], prefix: vec![e(20, None, None)] });
    }

    #[test]
    fn dos_line_quirks() {
        // final line without newline loses a char
        assert_eq!(dos_lines(b"ab\r\ncd\nlast"), vec![b"ab".to_vec(), b"cd".to_vec(), b"las".to_vec()]);
        let lines = dos_lines(&[b'x'; 120]);
        assert_eq!((lines[0].len(), lines[1].len()), (98, 20)); // fgets(100) splits, drops one each
    }

    #[test]
    fn c_float_prefixes() {
        assert_eq!(c_float(b"  12.5e1x"), Some((125.0, &b"x"[..])));
        assert_eq!(c_float(b"-.5 1"), Some((-0.5, &b" 1"[..])));
        assert_eq!(c_float(b"7e"), Some((7.0, &b"e"[..])));
        assert_eq!(c_float(b"bad"), None);
    }
}

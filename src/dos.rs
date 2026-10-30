//! Reading files the way the original DOS program did.

/// `fopen(.., "rt")` + repeated `fgets(buf, size, f)`: DOS text mode turns CRLF into LF and stops
/// at `^Z`. Each item is what one `fgets` call returns: at most `size - 1` bytes, ending at and
/// including `\n` when there is one. Long lines therefore come back in several pieces.
pub fn fgets_lines(bytes: &[u8], size: usize) -> Vec<Vec<u8>> {
    let end = bytes.iter().position(|&c| c == 0x1a).unwrap_or(bytes.len());
    let mut text = bytes[..end].iter().peekable();
    let mut lines = Vec::new();
    while text.peek().is_some() {
        let mut line = Vec::new();
        while line.len() < size - 1 {
            let Some(&c) = text.next() else { break };
            if c == b'\r' && text.peek() == Some(&&b'\n') {
                continue;
            }
            line.push(c);
            if c == b'\n' {
                break;
            }
        }
        lines.push(line);
    }
    lines
}

/// DOS bytes as a `String`, one char per byte (no code page translation).
pub fn latin1(s: &[u8]) -> String {
    s.iter().map(|&c| c as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_mode_fgets() {
        let lines = fgets_lines(b"ab\r\ncd\nlast\x1ajunk", 100);
        assert_eq!(lines, vec![b"ab\n".to_vec(), b"cd\n".to_vec(), b"last".to_vec()]);
        let lines = fgets_lines(b"abcdef\n", 4);
        assert_eq!(lines, vec![b"abc".to_vec(), b"def".to_vec(), b"\n".to_vec()]);
    }
}

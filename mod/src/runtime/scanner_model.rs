/// Mutable state for a `java.util.Scanner` backed by a VFS file.
#[derive(Clone, Debug)]
pub(crate) struct ScannerState {
    pub text: String,
    pub pos: usize,
    pub closed: bool,
    pub unknown: bool,
}

/// Returns the next line and byte position after its terminator.
/// This follows Java Scanner's line pattern: `.*(\\r\\n|[\\n\\r\\u{2028}\\u{2029}\\u{0085}])|.+$`.
pub(crate) fn next_line(text: &str, pos: usize) -> Option<(String, usize)> {
    if pos >= text.len() {
        return None;
    }
    let mut i = pos;
    while i < text.len() {
        let rest = &text[i..];
        let ch = rest.chars().next().unwrap();
        if matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}' | '\u{0085}') {
            let end = i;
            let next = if ch == '\r' && text[i + 1..].starts_with('\n') {
                i + 2
            } else {
                i + ch.len_utf8()
            };
            return Some((text[pos..end].to_owned(), next));
        }
        i += ch.len_utf8();
    }
    Some((text[pos..].to_owned(), text.len()))
}

#[cfg(test)]
mod tests {
    use super::next_line;
    #[test]
    fn crlf() {
        assert_eq!(next_line("a\r\nb", 0), Some(("a".into(), 3)));
    }
    #[test]
    fn lone_cr() {
        assert_eq!(next_line("a\rb", 0), Some(("a".into(), 2)));
    }
    #[test]
    fn unicode_separator() {
        assert_eq!(next_line("a\u{2028}b", 0), Some(("a".into(), 4)));
    }
    #[test]
    fn no_trailing_newline() {
        assert_eq!(next_line("a", 0), Some(("a".into(), 1)));
    }
    #[test]
    fn empty() {
        assert_eq!(next_line("", 0), None);
    }
}

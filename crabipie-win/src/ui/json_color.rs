//! JSON tokenizer for syntax coloring (VS Code Dark palette, as in the iced app).

use super::geom::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Key,
    Str,
    Num,
    Lit,
    Punct,
    Plain,
}

pub fn color(t: Tok) -> Color {
    match t {
        Tok::Key => Color::hex(0x9cdcfe),
        Tok::Str => Color::hex(0xce9178),
        Tok::Num => Color::hex(0xb5cea8),
        Tok::Lit => Color::hex(0x569cd6),
        Tok::Punct => Color::hex(0xd4d4d4),
        Tok::Plain => Color::hex(0xd4d4d4),
    }
}

/// Byte ranges (start, end, kind) of the non-whitespace tokens in one line.
pub fn tokenize(line: &str) -> Vec<(usize, usize, Tok)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' => i += 1,
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let end = i.min(b.len());
                let mut j = end;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                let key = j < b.len() && b[j] == b':';
                out.push((start, end, if key { Tok::Key } else { Tok::Str }));
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                out.push((i, i + 1, Tok::Punct));
                i += 1;
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while i < b.len() && matches!(b[i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    i += 1;
                }
                out.push((start, i, Tok::Num));
            }
            _ => {
                let start = i;
                while i < b.len() && !matches!(b[i], b' ' | b'\t' | b',' | b'}' | b']' | b':') {
                    i += 1;
                }
                if i == start {
                    i += 1; // always advance, even on odd bytes
                }
                // slicing on char boundaries: advance to the next boundary
                while i < b.len() && !line.is_char_boundary(i) {
                    i += 1;
                }
                let word = &line[start..i];
                let kind = if matches!(word, "true" | "false" | "null") { Tok::Lit } else { Tok::Plain };
                out.push((start, i, kind));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_keys_strings_numbers_and_literals() {
        let l = r#"  "id": 12, "ok": true, "t": "a\"b" "#;
        let kinds: Vec<Tok> = tokenize(l).iter().map(|t| t.2).collect();
        assert_eq!(
            kinds,
            vec![Tok::Key, Tok::Punct, Tok::Num, Tok::Punct, Tok::Key, Tok::Punct, Tok::Lit, Tok::Punct, Tok::Key, Tok::Punct, Tok::Str]
        );
        assert!(tokenize("\"unterminated").len() == 1);
        let _ = tokenize("héllo 日本"); // must not split a char
    }
}

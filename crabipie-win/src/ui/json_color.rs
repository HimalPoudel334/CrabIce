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

/// Palettes: key, string, number, true/false/null, punctuation, plain text.
/// The first three match the iced app (its built-in themes all share "Custom Dark").
const THEMES: [(&str, [u32; 6]); 5] = [
    ("VS Code Dark", [0x9cdcfe, 0xce9178, 0xb5cea8, 0x569cd6, 0xd4d4d4, 0xd4d4d4]),
    ("Custom Dark", [0x66c2f0, 0xbadd87, 0xe0ba87, 0xdb78a6, 0xcccccc, 0xffffff]),
    ("Solarized Dark", [0x829400, 0x29a199, 0xcc4a17, 0xd43582, 0x94a1a1, 0x94a1a1]),
    ("Monokai", [0x66d9ef, 0xe6db74, 0xae81ff, 0xf92672, 0xf8f8f2, 0xf8f8f2]),
    ("One Dark", [0xe06c75, 0x98c379, 0xd19a66, 0x56b6c2, 0xabb2bf, 0xabb2bf]),
];

static THEME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn theme_names() -> Vec<String> {
    THEMES.iter().map(|(n, _)| n.to_string()).collect()
}

pub fn theme_index() -> usize {
    THEME.load(std::sync::atomic::Ordering::Relaxed).min(THEMES.len() - 1)
}

pub fn set_theme(i: usize) {
    THEME.store(i.min(THEMES.len() - 1), std::sync::atomic::Ordering::Relaxed);
}

/// Maps a saved theme name (ours, or one the iced app wrote) to a palette index.
pub fn index_of(name: &str) -> usize {
    let n = name.to_lowercase();
    if let Some(i) = THEMES.iter().position(|(t, _)| t.to_lowercase() == n) {
        return i;
    }
    if n.contains("solarized") {
        2
    } else if n.contains("base16") || n.contains("github") || n.contains("custom") {
        1 // iced maps every other built-in to the Custom Dark colors
    } else {
        0
    }
}

pub fn color(t: Tok) -> Color {
    let p = &THEMES[theme_index()].1;
    Color::hex(match t {
        Tok::Key => p[0],
        Tok::Str => p[1],
        Tok::Num => p[2],
        Tok::Lit => p[3],
        Tok::Punct => p[4],
        Tok::Plain => p[5],
    })
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

    #[test]
    fn saved_theme_names_map_to_palettes() {
        assert_eq!(index_of("VS Code Dark"), 0);
        assert_eq!(index_of("solarized dark"), 2);
        assert_eq!(index_of("Solarized (dark)"), 2);
        assert_eq!(index_of("base16-ocean.dark"), 1);
        assert_eq!(index_of(""), 0);
        assert_eq!(theme_names().len(), 5);
    }
}

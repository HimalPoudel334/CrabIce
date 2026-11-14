//! Order-preserving JSON pretty-printer (serde_json's Value would sort keys).

const MAX_LEN: usize = 64 * 1024 * 1024;

/// Returns the pretty-printed text, or None if `s` isn't a JSON object/array
/// (or is too large to format without stalling the UI).
pub fn pretty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.len() > MAX_LEN || !(t.starts_with('{') || t.starts_with('[')) {
        return None;
    }
    serde_json::from_str::<serde::de::IgnoredAny>(t).ok()?; // validate only, builds no tree

    // JSON's structural characters are ASCII, so working on bytes keeps UTF-8 intact.
    let b = t.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len() * 2);
    let (mut indent, mut in_str, mut esc, mut i) = (0usize, false, false, 0usize);
    let nl = |out: &mut Vec<u8>, n: usize| {
        out.push(b'\n');
        for _ in 0..n {
            out.extend_from_slice(b"  ");
        }
    };
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_str = true;
                out.push(c);
            }
            b'{' | b'[' => {
                let close = if c == b'{' { b'}' } else { b']' };
                let mut j = i + 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && b[j] == close {
                    out.push(c);
                    out.push(close);
                    i = j + 1;
                    continue;
                }
                out.push(c);
                indent += 1;
                nl(&mut out, indent);
            }
            b'}' | b']' => {
                indent = indent.saturating_sub(1);
                nl(&mut out, indent);
                out.push(c);
            }
            b',' => {
                out.push(c);
                nl(&mut out, indent);
            }
            b':' => out.extend_from_slice(b": "),
            c if c.is_ascii_whitespace() => {}
            _ => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_keeps_key_order() {
        let p = pretty(r#"{"z":1,"a":[1,2,{"k":"v, {x}"}],"e":{},"n":[]}"#).unwrap();
        assert_eq!(
            p,
            "{\n  \"z\": 1,\n  \"a\": [\n    1,\n    2,\n    {\n      \"k\": \"v, {x}\"\n    }\n  ],\n  \"e\": {},\n  \"n\": []\n}"
        );
    }

    #[test]
    fn escaped_quotes_inside_strings_are_safe() {
        let p = pretty(r#"{"a":"he said \"hi\", ok"}"#).unwrap();
        assert!(p.contains(r#""he said \"hi\", ok""#));
    }

    #[test]
    fn rejects_non_json_and_invalid_json() {
        assert!(pretty("<html>").is_none());
        assert!(pretty("{\"a\":").is_none());
        assert!(pretty("42").is_none());
    }

    #[test]
    fn formats_big_bodies_that_used_to_be_skipped() {
        let item = r#"{"code":"AD22093","prodname":" KNEE BRACE","qty":1969.0,"ok":false}"#;
        let body = format!("[{}]", vec![item; 90_000].join(","));
        assert!(body.len() > 4 * 1024 * 1024);
        let p = pretty(&body).expect("formatted");
        assert_eq!(p.lines().count(), 90_000 * 6 + 2);
    }
}

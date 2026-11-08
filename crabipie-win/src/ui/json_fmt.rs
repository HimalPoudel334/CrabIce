//! Order-preserving JSON pretty-printer (serde_json's Value would sort keys).

const MAX_LEN: usize = 4 * 1024 * 1024;

/// Returns the pretty-printed text, or None if `s` isn't a JSON object/array
/// (or is too large to format without stalling the UI).
pub fn pretty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.len() > MAX_LEN || !(t.starts_with('{') || t.starts_with('[')) {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(t).ok()?; // validate only

    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len() * 2);
    let (mut indent, mut in_str, mut esc, mut i) = (0usize, false, false, 0usize);
    let nl = |out: &mut String, n: usize| {
        out.push('\n');
        for _ in 0..n {
            out.push_str("  ");
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && chars[j] == close {
                    out.push(c);
                    out.push(close);
                    i = j + 1;
                    continue;
                }
                out.push(c);
                indent += 1;
                nl(&mut out, indent);
            }
            '}' | ']' => {
                indent = indent.saturating_sub(1);
                nl(&mut out, indent);
                out.push(c);
            }
            ',' => {
                out.push(c);
                nl(&mut out, indent);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            _ => out.push(c),
        }
        i += 1;
    }
    Some(out)
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
}

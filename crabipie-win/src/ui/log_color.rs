//! Colors for WebSocket log lines: `HH:MM:SS.mmm <glyph> text`, where the
//! glyph says what happened (→ sent, ← received, ! error, others = status).
//! Lines without a timestamp are continuations of a pretty-printed JSON body.

use super::geom::Color;
use super::json_color;
use super::theme::Theme;

fn has_stamp(b: &[u8]) -> bool {
    b.len() >= 13
        && b[2] == b':'
        && b[5] == b':'
        && b[8] == b'.'
        && b[12] == b' '
        && [0usize, 1, 3, 4, 6, 7, 9, 10, 11].iter().all(|&i| b[i].is_ascii_digit())
}

/// Byte ranges (start, end, color) to draw for one log line.
pub fn tokens(line: &str, th: &Theme) -> Vec<(usize, usize, Color)> {
    let mut out = Vec::new();
    if !has_stamp(line.as_bytes()) {
        for (a, b, t) in json_color::tokenize(line) {
            out.push((a, b, json_color::color(t)));
        }
        return out;
    }
    out.push((0, 12, th.text_dim));
    let rest = &line[13..];
    let Some(g) = rest.chars().next() else { return out };
    let gl = g.len_utf8();
    let col = match g {
        '→' => th.accent,
        '←' => th.success,
        '!' => th.danger,
        _ => th.text_dim,
    };
    out.push((13, 13 + gl, col));
    let body = &line[13 + gl..];
    let trimmed = body.trim_start();
    let start = 13 + gl + (body.len() - trimmed.len());
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        for (a, b, t) in json_color::tokenize(&line[start..]) {
            out.push((start + a, start + b, json_color::color(t)));
        }
    } else if !trimmed.is_empty() {
        out.push((start, line.len(), if g == '!' { th.danger } else { th.text }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_glyphs_and_json_bodies_get_their_own_colors() {
        let th = Theme::default();
        let t = tokens("12:00:01.123 ← {\"a\": 1}", &th);
        assert_eq!(t[0], (0, 12, th.text_dim));
        assert_eq!(t[1].2, th.success);
        assert!(t.len() > 3, "the JSON body is tokenized");
        let c = tokens("  \"k\": 2", &th); // continuation line
        assert!(!c.is_empty());
        assert_eq!(tokens("12:00:01.123 ! boom", &th).last().unwrap().2, th.danger);
    }
}

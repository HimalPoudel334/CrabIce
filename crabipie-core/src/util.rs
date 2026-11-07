//! Small pure helpers: header/form raw-text conversion, query-string
//! syncing, find-in-text, duration formatting, GraphQL query building.

use crate::*;

// ── headers / form data <-> raw text ────────────────────────────────────

pub fn headers_to_raw(headers: &[RequestHeaders]) -> String {
    headers
        .iter()
        .filter_map(|h| {
            if h.key.is_empty() && h.value.is_empty() {
                None
            } else {
                let mut line = format!("{}: {}", h.key, h.value);
                if !h.enabled {
                    line = format!("# {}", line);
                }
                Some(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn raw_to_headers(raw: &str) -> Vec<RequestHeaders> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (enabled, line) = if let Some(rest) = line.strip_prefix('#') {
                (false, rest.trim())
            } else {
                (true, line)
            };
            let (key, value) = line.split_once(':')?;
            Some(RequestHeaders {
                enabled,
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect()
}

pub fn form_data_to_raw(form_data: &[FormField]) -> String {
    form_data
        .iter()
        .filter_map(|f| {
            if f.key.is_empty() && f.value.is_empty() || f.field_type == FormFieldType::File {
                None
            } else {
                let mut line = format!("{}: {}", f.key, f.value);
                if !f.enabled {
                    line = format!("# {}", line);
                }
                Some(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn raw_to_form_data(raw: &str) -> Vec<FormField> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (enabled, content) = if let Some(rest) = line.strip_prefix('#') {
                (false, rest)
            } else {
                (true, line)
            };
            let (key, value) = content.split_once(':')?;
            Some(FormField {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
                enabled,
                field_type: FormFieldType::Text,
                files: vec![],
            })
        })
        .collect()
}

// ── query params <-> URL ────────────────────────────────────────────────

/// "?a=1&b=2" built from enabled, non-empty params (empty string if none).
pub fn build_query_string(params: &[QueryParam]) -> String {
    let parts: Vec<String> = params
        .iter()
        .filter(|p| p.enabled && !p.key.is_empty())
        .map(|p| format!("{}={}", urlencoding::encode(&p.key), urlencoding::encode(&p.value)))
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// Rewrites the URL's query string from the params table.
pub fn rebuild_url(current_url: &str, params: &[QueryParam]) -> String {
    let base = current_url.split('?').next().unwrap_or(current_url);
    let mut query = String::new();
    for p in params {
        if p.enabled && !p.key.is_empty() {
            if !query.is_empty() {
                query.push('&');
            }
            query.push_str(&encode_query_component(&p.key));
            query.push('=');
            query.push_str(&encode_query_component(&p.value));
        }
    }
    if query.is_empty() {
        base.to_string()
    } else {
        format!("{}?{}", base, query)
    }
}

/// Rebuilds the params table from whatever follows `?` in the URL.
pub fn parse_url_query(url: &str) -> Vec<QueryParam> {
    let mut out = Vec::new();
    if let Some(q_index) = url.find('?') {
        for pair in url[q_index + 1..].split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut parts = pair.splitn(2, '=');
            out.push(QueryParam {
                key: parts.next().unwrap_or("").to_string(),
                value: parts.next().unwrap_or("").to_string(),
                enabled: true,
            });
        }
    }
    out
}

// ── find in text ────────────────────────────────────────────────────────

/// (line index, char offset within line) for a byte position.
pub fn position_to_line_col(text: &str, byte_pos: usize) -> (usize, usize) {
    let mut line_idx = 0;
    let mut line_start_byte = 0;
    for line in text.split_inclusive('\n') {
        let line_end_byte = line_start_byte + line.len();
        if byte_pos >= line_start_byte && byte_pos < line_end_byte {
            let byte_offset_in_line = byte_pos - line_start_byte;
            let char_offset = line[..byte_offset_in_line].chars().count();
            return (line_idx, char_offset);
        }
        line_start_byte = line_end_byte;
        line_idx += 1;
    }
    (0, 0)
}

/// Byte offsets of every match of `pattern` in `text`.
pub fn find_matches(text: &str, pattern: &str, case_sensitive: bool, whole_word: bool) -> Vec<usize> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let (hay, needle) = if case_sensitive {
        (text.to_string(), pattern.to_string())
    } else {
        (text.to_lowercase(), pattern.to_lowercase())
    };

    let mut matches = Vec::new();
    let mut start = 0;
    while start <= hay.len() {
        // Step to a char boundary so slicing never panics on multibyte text.
        while start < hay.len() && !hay.is_char_boundary(start) {
            start += 1;
        }
        let Some(pos) = hay[start..].find(&needle) else {
            break;
        };
        let abs_pos = start + pos;
        if whole_word {
            let before = abs_pos == 0
                || !text[..abs_pos].chars().last().map_or(false, |c| c.is_alphanumeric());
            let after_pos = abs_pos + pattern.len();
            let after = after_pos >= text.len()
                || !text
                    .get(after_pos..)
                    .and_then(|s| s.chars().next())
                    .map_or(false, |c| c.is_alphanumeric());
            if before && after {
                matches.push(abs_pos);
            }
        } else {
            matches.push(abs_pos);
        }
        start = abs_pos + 1;
    }
    matches
}

// ── misc ────────────────────────────────────────────────────────────────

pub fn format_duration(dur: std::time::Duration) -> String {
    let secs = dur.as_secs_f64();
    if secs < 1.0 {
        format!("{:.1}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.2}s", secs)
    } else if secs < 3600.0 {
        format!("{}m {}s", secs as u64 / 60, secs as u64 % 60)
    } else {
        format!("{}h {}m", secs as u64 / 3600, (secs as u64 % 3600) / 60)
    }
}

/// Order in which to prefetch/render tabs: nearest to `active` first.
pub fn proximity_order(active: usize, len: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..len).collect();
    indices.sort_by_key(|&i| i.abs_diff(active));
    indices
}

/// Builds the `query { ... }` text from the schema browser's selection.
pub fn build_graphql_query(
    schema: Option<&GraphqlSchema>,
    selected_paths: &std::collections::HashSet<String>,
) -> String {
    let mut buf = String::from("query {\n");
    if let Some(schema) = schema {
        let root_type_name = &schema.query_type.name;
        if let Some(root_type) = schema.types.iter().find(|t| t.name.as_deref() == Some("Query")) {
            buf.push_str(&generate_selection_set(
                &schema.types,
                root_type,
                selected_paths,
                1,
                root_type_name,
            ));
        }
    }
    buf.push('}');
    buf
}


pub static VAR_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"\{\{([\w$:+-]+)\}\}").unwrap());

/// Built-in dynamic variables, evaluated fresh every time they're resolved
pub enum DynamicVar {
    Uuid,
    /// Offset in whole days.
    Date(i64),
    /// Offset in seconds.
    DateTime(i64),
    /// Offset in seconds.
    Timestamp(i64),
    /// Inclusive (lo, hi) range.
    RandomInt(i64, i64),
}

pub fn parse_dynamic_var(name: &str) -> Option<DynamicVar> {
    if !name.starts_with('$') {
        return None;
    }

    let (base, offset) = match name[1..].find(['+', '-']) {
        Some(idx) => (&name[..idx + 1], Some(&name[idx + 1..])),
        None => (name, None),
    };

    match base {
        "$uuid" | "$guid" | "$randomUUID" => Some(DynamicVar::Uuid),
        "$date" => Some(DynamicVar::Date(parse_offset_secs(offset) / 86_400)),
        "$datetime" | "$isoTimestamp" => Some(DynamicVar::DateTime(parse_offset_secs(offset))),
        "$timestamp" => Some(DynamicVar::Timestamp(parse_offset_secs(offset))),
        _ if name.starts_with("$randomInt") => Some(parse_random_int(name)),
        _ => None,
    }
}

pub fn parse_offset_secs(offset: Option<&str>) -> i64 {
    let Some(offset) = offset else { return 0 };
    if offset.len() < 2 {
        return 0;
    }
    let (sign, rest) = offset.split_at(1);
    let sign: i64 = if sign == "-" { -1 } else { 1 };
    let (num, unit) = rest.split_at(rest.len() - 1);
    let Ok(n) = num.parse::<i64>() else {
        return 0;
    };
    let secs = match unit {
        "d" => n * 86_400,
        "h" => n * 3_600,
        "m" => n * 60,
        "s" => n,
        _ => return 0,
    };
    sign * secs
}

/// Parses "$randomInt" (defaults to 0..=1000) or "$randomInt:lo:hi".
pub fn parse_random_int(name: &str) -> DynamicVar {
    match name.split(':').collect::<Vec<_>>().as_slice() {
        [_, lo, hi] => {
            let lo: i64 = lo.parse().unwrap_or(0);
            let hi: i64 = hi.parse().unwrap_or(1000);
            if lo <= hi {
                DynamicVar::RandomInt(lo, hi)
            } else {
                DynamicVar::RandomInt(hi, lo)
            }
        }
        _ => DynamicVar::RandomInt(0, 1000),
    }
}

pub fn resolve_dynamic_var(name: &str) -> Option<String> {
    let var = parse_dynamic_var(name)?;
    Some(match var {
        DynamicVar::Uuid => uuid::Uuid::new_v4().to_string(),
        DynamicVar::Date(offset_days) => (chrono::Utc::now() + chrono::Duration::days(offset_days))
            .format("%Y-%m-%d")
            .to_string(),
        DynamicVar::DateTime(offset_secs) => (chrono::Utc::now()
            + chrono::Duration::seconds(offset_secs))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string(),
        DynamicVar::Timestamp(offset_secs) => (chrono::Utc::now()
            + chrono::Duration::seconds(offset_secs))
        .timestamp()
        .to_string(),
        DynamicVar::RandomInt(lo, hi) => {
            use rand::Rng;
            rand::thread_rng().gen_range(lo..=hi).to_string()
        }
    })
}

pub fn encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        out.push_str(&urlencoding::encode(&rest[..start]));
        match rest[start..].find("}}") {
            Some(rel_end) => {
                let end = start + rel_end + 2;
                out.push_str(&rest[start..end]);
                rest = &rest[end..];
            }
            None => {
                // Unterminated "{{" — treat the remainder as literal text.
                out.push_str(&urlencoding::encode(&rest[start..]));
                rest = "";
            }
        }
    }
    out.push_str(&urlencoding::encode(rest));
    out
}

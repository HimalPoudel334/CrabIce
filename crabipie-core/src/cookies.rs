use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub enabled: bool,
    pub domain: String,
    pub path: String,
    /// Absolute expiry (RFC 2822 when written by us; several HTTP date formats are read).
    pub expires: Option<String>,
    #[serde(default)]
    pub secure: bool,
    /// No `Domain` attribute: only sent to exactly the host that set it.
    #[serde(default)]
    pub host_only: bool,
}

impl CookieEntry {
    /// A session cookie for `domain`, sent to that host on every path.
    pub fn new(name: &str, value: &str, domain: &str) -> Self {
        Self { name: name.into(), value: value.into(), enabled: true, domain: domain.into(), path: "/".into(), expires: None, secure: false, host_only: true }
    }
}

pub fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.to_string()))
}

/// Parses the date formats servers use for `Expires` (RFC 2822/1123, the dashed
/// RFC 850 variants, asctime) and RFC 3339.
pub fn parse_expires(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc2822(s) {
        return Some(d.with_timezone(&Utc));
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    let t = s.trim_end_matches("GMT").trim_end_matches("UTC").trim();
    for f in ["%a, %d-%b-%Y %H:%M:%S", "%a, %d %b %Y %H:%M:%S", "%A, %d-%b-%y %H:%M:%S", "%a %b %e %H:%M:%S %Y"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(t, f) {
            return Some(n.and_utc());
        }
    }
    None
}

pub fn is_expired(c: &CookieEntry, now: DateTime<Utc>) -> bool {
    c.expires.as_deref().and_then(parse_expires).map_or(false, |t| t <= now)
}

/// RFC 6265 default-path: the request path up to (not including) its last '/'.
fn default_path(request_path: &str) -> String {
    match request_path.rfind('/') {
        Some(i) if i > 0 && request_path.starts_with('/') => request_path[..i].to_string(),
        _ => "/".to_string(),
    }
}

fn domain_match(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    host.parse::<std::net::IpAddr>().is_err() && host.len() > domain.len() && host.ends_with(domain) && host[..host.len() - domain.len()].ends_with('.')
}

fn path_match(request_path: &str, cookie_path: &str) -> bool {
    let cp = if cookie_path.is_empty() { "/" } else { cookie_path };
    request_path == cp || (request_path.starts_with(cp) && (cp.ends_with('/') || request_path[cp.len()..].starts_with('/')))
}

/// Parses one `Set-Cookie` header received from `url`. Returns the jar key (the
/// domain it is stored under), the cookie, and whether it is already expired
/// (Max-Age <= 0 or a past Expires: that means "delete"). None if it is invalid
/// or sets a Domain the request host doesn't belong to.
pub fn parse_set_cookie_at(raw: &str, url: &str, now: DateTime<Utc>) -> Option<(String, CookieEntry, bool)> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let mut parts = raw.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let (mut path, mut domain, mut expires, mut max_age, mut secure) = (None, None, None, None, false);
    for part in parts {
        let (k, v) = part.split_once('=').map_or((part.trim(), ""), |(k, v)| (k.trim(), v.trim()));
        match k.to_ascii_lowercase().as_str() {
            "path" if v.starts_with('/') => path = Some(v.to_string()),
            "domain" => {
                let d = v.trim_start_matches('.').to_lowercase();
                if !d.is_empty() {
                    domain = Some(d);
                }
            }
            "expires" => expires = parse_expires(v),
            "max-age" => max_age = v.parse::<i64>().ok(),
            "secure" => secure = true,
            _ => {}
        }
    }
    let (key, host_only) = match domain {
        Some(d) if domain_match(&host, &d) => (d, false),
        Some(_) => return None,
        None => (host, true),
    };
    // Max-Age wins over Expires
    let expiry = match max_age {
        Some(n) if n <= 0 => Some(now - chrono::Duration::seconds(1)),
        Some(n) => Some(now + chrono::Duration::seconds(n)),
        None => expires,
    };
    let entry = CookieEntry {
        name: name.to_string(),
        value: value.trim().to_string(),
        enabled: true,
        domain: key.clone(),
        path: path.unwrap_or_else(|| default_path(u.path())),
        expires: expiry.map(|t| t.to_rfc2822()),
        secure,
        host_only,
    };
    let expired = expiry.map_or(false, |t| t <= now);
    Some((key, entry, expired))
}

/// Drops expired cookies (and domains left empty).
pub fn purge_expired(jar: &mut HashMap<String, Vec<CookieEntry>>, now: DateTime<Utc>) {
    for list in jar.values_mut() {
        list.retain(|c| !is_expired(c, now));
    }
    jar.retain(|_, list| !list.is_empty());
}

/// The `Cookie` header value for a request to `url`: domain, path, Secure and
/// expiry rules applied, longer paths first. None when nothing matches.
pub fn cookie_header(jar: &HashMap<String, Vec<CookieEntry>>, url: &str, now: DateTime<Utc>) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let path = u.path();
    let secure_ok = u.scheme() == "https" || matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    let mut hits: Vec<&CookieEntry> = Vec::new();
    for (key, list) in jar {
        let key = key.to_lowercase();
        for c in list {
            if !c.enabled || c.name.is_empty() || is_expired(c, now) || (c.secure && !secure_ok) || !path_match(path, &c.path) {
                continue;
            }
            let domain_ok = if c.host_only { host == key } else { domain_match(&host, &key) };
            if domain_ok {
                hits.push(c);
            }
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by(|a, b| b.path.len().cmp(&a.path.len()).then_with(|| a.name.cmp(&b.name)));
    Some(hits.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
}

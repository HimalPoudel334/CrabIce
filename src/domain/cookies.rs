//! Cookie jar entries and `Set-Cookie` parsing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CookieEntry {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) enabled: bool,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) expires: Option<String>,
}

pub(crate) fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

pub(crate) fn parse_set_cookie(raw: &str) -> Option<CookieEntry> {
    let mut parts = raw.split(';');
    let main = parts.next()?;
    let (name, value) = main.split_once('=')?;

    let mut path = "/".to_string();
    let mut domain = String::new();
    let mut expires = None;

    for part in parts {
        let p = part.trim();
        if let Some(v) = p.strip_prefix("path=") {
            path = v.to_string();
        } else if let Some(v) = p.strip_prefix("domain=") {
            domain = v.to_string();
        } else if let Some(v) = p.strip_prefix("expires=") {
            expires = Some(v.to_string());
        }
    }

    Some(CookieEntry {
        name: name.trim().to_string(),
        value: value.trim().to_string(),
        enabled: true,
        domain,
        path,
        expires,
    })
}

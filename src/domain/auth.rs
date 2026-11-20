//! Auth configuration and the automatic login / token-refresh logic.

use serde::{Deserialize, Serialize};

use crate::domain::environment::Environment;

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum AuthType {
    None,
    Basic,
    Bearer,
    ApiKey,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "No Auth"),
            AuthType::Basic => write!(f, "Basic Auth"),
            AuthType::Bearer => write!(f, "Bearer Token"),
            AuthType::ApiKey => write!(f, "Api Key"),
        }
    }
}

impl AuthType {
    pub(crate) const ALL: [AuthType; 4] = [
        AuthType::None,
        AuthType::Basic,
        AuthType::Bearer,
        AuthType::ApiKey,
    ];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ApiKeyPosition {
    Header,
    QueryParams,
}

impl std::fmt::Display for ApiKeyPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiKeyPosition::Header => write!(f, "Header"),
            ApiKeyPosition::QueryParams => write!(f, "Query Params"),
        }
    }
}

impl ApiKeyPosition {
    pub(crate) const ALL: [ApiKeyPosition; 2] = [ApiKeyPosition::Header, ApiKeyPosition::QueryParams];
}

// ─────────────────────────────────────────────────────────────────────────
// Auth auto-refresh (login/refresh-token automation)
// ─────────────────────────────────────────────────────────────────────────

/// How an environment renews its access token automatically.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum AuthRefreshMode {
    /// No auto-refresh configured; bearer/basic fields behave exactly as before.
    #[default]
    None,
    /// No dedicated refresh endpoint exists — "refreshing" means calling the
    /// login endpoint again with the stored username/password.
    ReLogin,
    /// A proper OAuth2-style flow: login returns an access token *and* a
    /// refresh token; renewal calls a separate refresh endpoint with the
    /// refresh token instead of resending the password.
    RefreshToken,
}

impl std::fmt::Display for AuthRefreshMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthRefreshMode::None => write!(f, "Off"),
            AuthRefreshMode::ReLogin => write!(f, "Re-login (username/password)"),
            AuthRefreshMode::RefreshToken => write!(f, "Refresh token"),
        }
    }
}

impl AuthRefreshMode {
    pub(crate) const ALL: [AuthRefreshMode; 3] = [
        AuthRefreshMode::None,
        AuthRefreshMode::ReLogin,
        AuthRefreshMode::RefreshToken,
    ];
}

/// HTTP method used for the login / refresh request.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum AuthHttpMethod {
    #[default]
    Post,
    Put,
    Patch,
    /// Only valid with form-urlencoded; fields are sent as the query string.
    Get,
}

impl AuthHttpMethod {
    pub(crate) const ALL: [AuthHttpMethod; 4] = [
        AuthHttpMethod::Post,
        AuthHttpMethod::Put,
        AuthHttpMethod::Patch,
        AuthHttpMethod::Get,
    ];

    pub(crate) fn to_reqwest(self) -> reqwest::Method {
        match self {
            AuthHttpMethod::Post => reqwest::Method::POST,
            AuthHttpMethod::Put => reqwest::Method::PUT,
            AuthHttpMethod::Patch => reqwest::Method::PATCH,
            AuthHttpMethod::Get => reqwest::Method::GET,
        }
    }
}

impl std::fmt::Display for AuthHttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthHttpMethod::Post => write!(f, "POST"),
            AuthHttpMethod::Put => write!(f, "PUT"),
            AuthHttpMethod::Patch => write!(f, "PATCH"),
            AuthHttpMethod::Get => write!(f, "GET"),
        }
    }
}

/// Body encoding for the login / refresh request.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum AuthBodyFormat {
    #[default]
    Json,
    FormUrlEncoded,
}

impl AuthBodyFormat {
    pub(crate) const ALL: [AuthBodyFormat; 2] = [AuthBodyFormat::Json, AuthBodyFormat::FormUrlEncoded];
}

impl std::fmt::Display for AuthBodyFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthBodyFormat::Json => write!(f, "JSON"),
            AuthBodyFormat::FormUrlEncoded => write!(f, "x-www-form-urlencoded"),
        }
    }
}

/// Which of the two auth requests a message/helper refers to.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AuthRequestKind {
    Login,
    Refresh,
}

pub(crate) fn default_auth_template(kind: AuthRequestKind, format: AuthBodyFormat) -> &'static str {
    match (kind, format) {
        (AuthRequestKind::Login, AuthBodyFormat::Json) => {
            r#"{"username": "{{username}}", "password": "{{password}}"}"#
        }
        (AuthRequestKind::Login, AuthBodyFormat::FormUrlEncoded) => {
            "grant_type=password&username={{username}}&password={{password}}"
        }
        (AuthRequestKind::Refresh, AuthBodyFormat::Json) => {
            r#"{"refresh_token": "{{refresh_token}}"}"#
        }
        (AuthRequestKind::Refresh, AuthBodyFormat::FormUrlEncoded) => {
            "grant_type=refresh_token&refresh_token={{refresh_token}}"
        }
    }
}

/// Where in the login/refresh response body a value should be read from,
/// expressed as a dotted/bracketed JSON path, e.g. "data.tokens[0].access_token".
pub(crate) type FieldPath = String;

/// Per-environment configuration for automatic bearer-token renewal. Stored
/// on `Environment` because credentials and endpoints are environment-scoped
/// (dev/staging/prod each have their own login endpoint and creds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AuthRefreshConfig {
    pub(crate) mode: AuthRefreshMode,
    pub(crate) auto_refresh_enabled: bool,

    /// Endpoint used for the initial login (both modes) and for ReLogin renewal.
    pub(crate) login_endpoint: String,
    /// Endpoint used to renew via refresh token (RefreshToken mode only). If
    /// empty, falls back to `login_endpoint`.
    pub(crate) refresh_endpoint: String,

    /// Env var names the resolved tokens get written into. Reference these
    /// with `{{access_token_var}}` in the tab's Bearer Token field.
    pub(crate) access_token_var: String,
    pub(crate) refresh_token_var: String,

    /// Raw JSON request bodies, sent through the same {{var}} substitution
    /// used everywhere else (URL, headers). This is what actually makes the
    /// request shape generalize: instead of a fixed set of named fields
    /// (which breaks the moment an API wants an extra field, a different
    /// key name, or both tokens sent back together), the user just writes
    /// the exact JSON their endpoint expects and references whichever env
    /// vars they want by name.
    /// Used for the initial login (and ReLogin-mode renewal).
    pub(crate) login_body_template: String,
    /// Used for RefreshToken-mode renewal when a refresh token is already
    /// stored. Falls back to `login_body_template` when none is stored yet.
    pub(crate) refresh_body_template: String,

    /// HTTP method + body encoding for the login / refresh requests. OAuth2
    /// token endpoints want `application/x-www-form-urlencoded`, not JSON.
    /// `#[serde(default)]` keeps previously saved environments loading (POST + JSON).
    #[serde(default)]
    pub(crate) login_method: AuthHttpMethod,
    #[serde(default)]
    pub(crate) login_body_format: AuthBodyFormat,
    #[serde(default)]
    pub(crate) refresh_method: AuthHttpMethod,
    #[serde(default)]
    pub(crate) refresh_body_format: AuthBodyFormat,

    /// JSON paths into the login/refresh response body.
    pub(crate) access_token_path: FieldPath,
    pub(crate) refresh_token_path: FieldPath,
    /// Optional path to a seconds-until-expiry field (e.g. "expires_in").
    /// Leave empty to fall back to `default_ttl_secs`.
    pub(crate) expiry_path: FieldPath,
    /// Used when the API doesn't report an expiry at all.
    pub(crate) default_ttl_secs: i64,
}

impl Default for AuthRefreshConfig {
    fn default() -> Self {
        Self {
            mode: AuthRefreshMode::None,
            auto_refresh_enabled: false,
            login_endpoint: String::new(),
            refresh_endpoint: String::new(),
            access_token_var: "access_token".to_string(),
            refresh_token_var: "refresh_token".to_string(),
            login_body_template: r#"{"username": "{{username}}", "password": "{{password}}"}"#
                .to_string(),
            refresh_body_template: r#"{"refresh_token": "{{refresh_token}}"}"#.to_string(),
            login_method: AuthHttpMethod::default(),
            login_body_format: AuthBodyFormat::default(),
            refresh_method: AuthHttpMethod::default(),
            refresh_body_format: AuthBodyFormat::default(),
            access_token_path: String::new(),
            refresh_token_path: String::new(),
            expiry_path: String::new(),
            default_ttl_secs: 3600,
        }
    }
}

/// Which field of `AuthRefreshConfig` a `Message::EnvAuthFieldChanged` edits.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum EnvAuthField {
    LoginEndpoint,
    RefreshEndpoint,
    LoginBodyTemplate,
    RefreshBodyTemplate,
    AccessTokenVar,
    RefreshTokenVar,
    AccessTokenPath,
    RefreshTokenPath,
    ExpiryPath,
    DefaultTtlSecs,
}

/// Result of a successful login/refresh call, ready to be written into the
/// owning environment's variables.
#[derive(Debug, Clone)]
pub(crate) struct AuthRefreshOutcome {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    /// Unix millis when the access token should be considered stale.
    pub(crate) expires_at: Option<i64>,
}

/// A candidate field found while auto-scanning a login response, offered to
/// the user as a one-click "use this as X" suggestion during setup.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldSuggestion {
    pub(crate) path: String,
    pub(crate) kind: SuggestionKind,
    pub(crate) sample_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SuggestionKind {
    AccessToken,
    RefreshToken,
    Expiry,
    LooksLikeJwt,
}

impl std::fmt::Display for SuggestionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SuggestionKind::AccessToken => write!(f, "access token"),
            SuggestionKind::RefreshToken => write!(f, "refresh token"),
            SuggestionKind::Expiry => write!(f, "expiry"),
            SuggestionKind::LooksLikeJwt => write!(f, "looks like a JWT"),
        }
    }
}

/// Resolves a dotted/bracketed path (e.g. "data.tokens[0].access_token")
/// against a `serde_json::Value`. Returns `None` if any segment is missing
/// or is the wrong shape (e.g. indexing into an object).
pub(crate) fn resolve_json_path(root: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    if path.trim().is_empty() {
        return None;
    }
    enum Seg {
        Key(String),
        Index(usize),
    }
    fn split_path(path: &str) -> Vec<Seg> {
        let mut segments = Vec::new();
        for dot_part in path.split('.') {
            if dot_part.is_empty() {
                continue;
            }
            let mut rest = dot_part;
            if let Some(bracket_pos) = rest.find('[') {
                let key = &rest[..bracket_pos];
                if !key.is_empty() {
                    segments.push(Seg::Key(key.to_string()));
                }
                rest = &rest[bracket_pos..];
                while let Some(close) = rest.find(']') {
                    if let Ok(idx) = rest[1..close].parse::<usize>() {
                        segments.push(Seg::Index(idx));
                    }
                    rest = &rest[close + 1..];
                    if !rest.starts_with('[') {
                        break;
                    }
                }
            } else {
                segments.push(Seg::Key(rest.to_string()));
            }
        }
        segments
    }

    let mut current = root;
    for seg in split_path(path) {
        current = match seg {
            Seg::Key(k) => current.get(&k)?,
            Seg::Index(i) => current.get(i)?,
        };
    }
    Some(current.clone())
}

/// Recursively scans a login/refresh response and flags fields that look
/// like an access token, refresh token, or expiry, purely by key-name and
/// shape heuristics. Used only to speed up initial setup (`Test Login`
/// button) — the resolved path the user confirms is what actually gets
/// stored and used at runtime, never these heuristics themselves.
pub(crate) fn suggest_token_fields(root: &serde_json::Value) -> Vec<FieldSuggestion> {
    fn looks_like_jwt(s: &str) -> bool {
        let parts: Vec<&str> = s.split('.').collect();
        parts.len() == 3
            && parts.iter().all(|p| {
                !p.is_empty()
                    && p.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            })
    }

    fn classify(key: &str, value: &serde_json::Value, path: &str, out: &mut Vec<FieldSuggestion>) {
        let lk = key.to_lowercase();
        let is_str = value.is_string();
        const ACCESS: &[&str] = &[
            "token",
            "access_token",
            "accesstoken",
            "jwt",
            "authtoken",
            "bearer",
        ];
        const REFRESH: &[&str] = &["refresh_token", "refreshtoken"];
        const EXPIRY: &[&str] = &[
            "expires_in",
            "expiresin",
            "expiry",
            "expires_at",
            "expiresat",
            "exp",
            "ttl",
        ];

        if is_str && REFRESH.contains(&lk.as_str()) {
            out.push(FieldSuggestion {
                path: path.to_string(),
                kind: SuggestionKind::RefreshToken,
                sample_value: value.as_str().unwrap_or_default().to_string(),
            });
        } else if is_str && ACCESS.contains(&lk.as_str()) {
            out.push(FieldSuggestion {
                path: path.to_string(),
                kind: SuggestionKind::AccessToken,
                sample_value: value.as_str().unwrap_or_default().to_string(),
            });
        } else if (value.is_number() || is_str) && EXPIRY.contains(&lk.as_str()) {
            out.push(FieldSuggestion {
                path: path.to_string(),
                kind: SuggestionKind::Expiry,
                sample_value: value.to_string(),
            });
        } else if is_str && looks_like_jwt(value.as_str().unwrap_or_default()) {
            out.push(FieldSuggestion {
                path: path.to_string(),
                kind: SuggestionKind::LooksLikeJwt,
                sample_value: value.as_str().unwrap_or_default().to_string(),
            });
        }
    }

    fn walk(value: &serde_json::Value, path: String, out: &mut Vec<FieldSuggestion>) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let child = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    classify(k, v, &child, out);
                    walk(v, child, out);
                }
            }
            serde_json::Value::Array(arr) => {
                for (i, v) in arr.iter().enumerate() {
                    walk(v, format!("{path}[{i}]"), out);
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    walk(root, String::new(), &mut out);
    out
}

/// Calls the configured login (or refresh) endpoint and extracts the
/// access/refresh tokens and expiry from the JSON response.
///
/// - `ReLogin`: sends the login request (method/body format from config) to `login_endpoint`.
/// - `RefreshToken`: sends the refresh request to `refresh_endpoint` if a
///   refresh token is already stored, otherwise falls back to a full
///   username/password login (first run, or the refresh token itself died).
/// Resolves {{var}} substitution against the environment, then parses the
/// result as JSON. This is what makes the request body shape general: the
/// user writes exactly the JSON their endpoint expects, referencing
/// whichever env vars they want by name — no fixed set of named fields to
/// outgrow.
pub(crate) fn build_body_from_template(
    env: &Environment,
    template: &str,
) -> Result<serde_json::Value, String> {
    if template.trim().is_empty() {
        return Err("Request body template is empty".to_string());
    }
    let resolved = env.resolve_own_vars(template);
    serde_json::from_str(&resolved).map_err(|e| {
        format!(
            "Request body template isn't valid JSON after substituting variables: {e}\n\nResolved to: {resolved}"
        )
    })
}

/// Parses a form template (`k=v&k2=v2`, or one pair per line) into pairs, then
/// resolves `{{var}}` in each key/value *separately*. Substituting first and
/// splitting afterwards would break on values containing `&` or `=`; reqwest
/// percent-encodes the pairs when sending.
pub(crate) fn build_form_pairs(env: &Environment, template: &str) -> Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for raw in template.split(|c| c == '&' || c == '\n') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (k, v) = raw.split_once('=').unwrap_or((raw, ""));
        pairs.push((
            env.resolve_own_vars(k.trim()),
            env.resolve_own_vars(v.trim()),
        ));
    }
    if pairs.is_empty() {
        Err("Request body template is empty".to_string())
    } else {
        Ok(pairs)
    }
}

/// Builds the login/refresh request with the configured method and body encoding.
pub(crate) fn build_auth_request(
    client: &reqwest::Client,
    env: &Environment,
    endpoint: &str,
    method: AuthHttpMethod,
    format: AuthBodyFormat,
    template: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let rb = client.request(method.to_reqwest(), env.resolve_own_vars(endpoint));
    match format {
        AuthBodyFormat::Json => {
            if method == AuthHttpMethod::Get {
                return Err(
                    "GET can't carry a JSON body; use form-urlencoded (sent as query string) or POST"
                        .to_string(),
                );
            }
            Ok(rb.json(&build_body_from_template(env, template)?))
        }
        AuthBodyFormat::FormUrlEncoded => {
            let pairs = build_form_pairs(env, template)?;
            Ok(if method == AuthHttpMethod::Get {
                rb.query(&pairs)
            } else {
                rb.form(&pairs)
            })
        }
    }
}

pub(crate) async fn perform_auth_refresh(
    client: &reqwest::Client,
    env: &Environment,
) -> Result<AuthRefreshOutcome, String> {
    let cfg = env
        .auth_config
        .as_ref()
        .ok_or_else(|| "Auto-refresh is not configured for this environment".to_string())?;

    if cfg.mode == AuthRefreshMode::None {
        return Err("Auto-refresh is turned off for this environment".to_string());
    }

    let get_var = |name: &str| -> String {
        if name.is_empty() {
            return String::new();
        }
        env.get_var(name).unwrap_or_default()
    };

    let (endpoint, method, format, template) = match cfg.mode {
        AuthRefreshMode::None => unreachable!(),
        AuthRefreshMode::ReLogin => {
            if cfg.login_endpoint.trim().is_empty() {
                return Err("No login endpoint configured".to_string());
            }
            (
                cfg.login_endpoint.clone(),
                cfg.login_method,
                cfg.login_body_format,
                cfg.login_body_template.clone(),
            )
        }
        AuthRefreshMode::RefreshToken => {
            let refresh_token = get_var(&cfg.refresh_token_var);
            if refresh_token.is_empty() {
                // No refresh token yet (first run, or it expired/was revoked) —
                // fall back to a full login instead of failing outright.
                if cfg.login_endpoint.trim().is_empty() {
                    return Err(
                        "No refresh token stored yet and no login endpoint configured".to_string(),
                    );
                }
                (
                    cfg.login_endpoint.clone(),
                    cfg.login_method,
                    cfg.login_body_format,
                    cfg.login_body_template.clone(),
                )
            } else {
                let endpoint = if cfg.refresh_endpoint.trim().is_empty() {
                    cfg.login_endpoint.clone()
                } else {
                    cfg.refresh_endpoint.clone()
                };
                if endpoint.trim().is_empty() {
                    return Err("No refresh endpoint configured".to_string());
                }
                (
                    endpoint,
                    cfg.refresh_method,
                    cfg.refresh_body_format,
                    cfg.refresh_body_template.clone(),
                )
            }
        }
    };

    let resp = build_auth_request(client, env, &endpoint, method, format, &template)?
        .send()
        .await
        .map_err(|e| format!("Auth refresh request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Auth refresh endpoint returned {status}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Auth refresh response wasn't valid JSON: {e}"))?;

    let access_token = resolve_json_path(&json, &cfg.access_token_path)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| {
            format!(
                "Couldn't find an access token at path '{}' in the response",
                cfg.access_token_path
            )
        })?;

    let refresh_token =
        if cfg.mode == AuthRefreshMode::RefreshToken && !cfg.refresh_token_path.is_empty() {
            resolve_json_path(&json, &cfg.refresh_token_path)
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        } else {
            None
        };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let expires_at = if !cfg.expiry_path.is_empty() {
        resolve_json_path(&json, &cfg.expiry_path)
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            })
            .map(|secs_from_now| now_ms + secs_from_now * 1000)
    } else if cfg.default_ttl_secs > 0 {
        Some(now_ms + cfg.default_ttl_secs * 1000)
    } else {
        None
    };

    Ok(AuthRefreshOutcome {
        access_token,
        refresh_token,
        expires_at,
    })
}

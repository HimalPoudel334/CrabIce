//! Turning a tab's saved configuration into a ready-to-send reqwest request,
//! plus the pure decision logic around auth auto-refresh and cookies.

use crate::*;
use std::collections::HashMap;

/// Substitutes `{{var}}` against the given environment. Dynamic variables
/// (`{{$random...}}`, timestamps, ...) win over environment variables.
pub fn resolve_vars(input: &str, env: Option<&Environment>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    replace_vars(input, |name| {
        resolve_dynamic_var(name).or_else(|| {
            env.and_then(|e| e.variables.iter().find(|v| v.enabled && v.key == name))
                .map(|v| v.value.clone())
        })
    })
}

/// Builds the request for one tab. Returns the builder and the final URL
/// (scheme defaulted to https, api-key query param appended if configured).
pub fn build_request(
    tab: &SavedState,
    env: Option<&Environment>,
    cookie_jar: &HashMap<String, Vec<CookieEntry>>,
) -> (reqwest::RequestBuilder, String) {
    let rv = |s: &str| resolve_vars(s, env);

    let mut url = rv(&tab.url);
    if !matches!(url.split_once("://"), Some(("http" | "https", _))) {
        url = format!("https://{}", url);
    }

    // ── headers ──────────────────────────
    let mut header_map: reqwest::header::HeaderMap = tab
        .headers
        .iter()
        .filter(|h| h.enabled)
        .filter_map(|h| {
            let key = rv(&h.key);
            let value = rv(&h.value);
            let name = reqwest::header::HeaderName::from_bytes(key.trim().as_bytes()).ok()?;
            let value = reqwest::header::HeaderValue::from_str(value.trim()).ok()?;
            Some((name, value))
        })
        .collect();

    // ── auth ──────────────────────────────
    // Auth fields go through variable resolution too, so a Bearer Token of
    // "{{access_token}}" picks up whatever auto-refresh last wrote.
    match tab.auth_type {
        AuthType::Basic => {
            let username = rv(&tab.username);
            let password = rv(&tab.password);
            if !username.is_empty() || !password.is_empty() {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    credentials,
                );
                if let Ok(hv) = reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded)) {
                    header_map.insert(reqwest::header::AUTHORIZATION, hv);
                }
            }
        }
        AuthType::Bearer => {
            let bearer_token = rv(&tab.bearer_token);
            if !bearer_token.is_empty() {
                if let Ok(hv) =
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", bearer_token))
                {
                    header_map.insert(reqwest::header::AUTHORIZATION, hv);
                }
            }
        }
        AuthType::ApiKey => {
            let api_key = rv(&tab.api_key);
            let api_key_name = rv(&tab.api_key_name);
            if !api_key.is_empty() && !api_key_name.is_empty() {
                if tab.api_key_position == ApiKeyPosition::Header {
                    if let (Ok(hn), Ok(hv)) = (
                        reqwest::header::HeaderName::try_from(&api_key_name),
                        reqwest::header::HeaderValue::from_str(&api_key),
                    ) {
                        header_map.insert(hn, hv);
                    }
                } else if let Ok(mut parsed) = url::Url::parse(&url) {
                    parsed.query_pairs_mut().append_pair(&api_key_name, &api_key);
                    url = parsed.to_string();
                }
            }
        }
        AuthType::None => {}
    }

    // ── cookie jar ───────────────────────
    if let Some(domain) = extract_domain(&url) {
        if let Some(cookies) = cookie_jar.get(&domain) {
            let cookie_str = cookies
                .iter()
                .filter(|c| c.enabled && !c.name.is_empty())
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; ");
            if !cookie_str.is_empty() {
                if let Ok(hv) = reqwest::header::HeaderValue::from_str(&cookie_str) {
                    header_map.insert(reqwest::header::COOKIE, hv);
                }
            }
        }
    }

    // ── body ─────────────────────────────
    let client = &*HTTP_CLIENT;
    if tab.request_type == RequestType::GraphQL {
        let variables: serde_json::Value = serde_json::from_str(&tab.graphql_variables)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        let body = serde_json::json!({
            "query": tab.graphql_query,
            "variables": variables,
            "operationName": if tab.graphql_operation.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(tab.graphql_operation.clone())
            }
        });

        return (
            client
                .post(&url)
                .body(body.to_string())
                .header("Content-Type", "application/json")
                .headers(header_map),
            url,
        );
    }

    let builder = match tab.method {
        HttpMethod::GET => client.get(&url),
        HttpMethod::DELETE => client.delete(&url),
        HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH => {
            let req = match tab.method {
                HttpMethod::POST => client.post(&url),
                HttpMethod::PUT => client.put(&url),
                HttpMethod::PATCH => client.patch(&url),
                _ => unreachable!(),
            };
            match tab.content_type {
                ContentType::Json => req
                    .body(tab.body.clone())
                    .header("Content-Type", "application/json"),
                ContentType::XWWWFormUrlEncoded => {
                    let params: Vec<_> = tab
                        .form_data
                        .iter()
                        .filter(|f| {
                            f.enabled && !f.key.is_empty() && f.field_type == FormFieldType::Text
                        })
                        .map(|f| (f.key.clone(), f.value.clone()))
                        .collect();
                    req.form(&params)
                }
                ContentType::FormData => {
                    let mut form = reqwest::multipart::Form::new();
                    for field in &tab.form_data {
                        if field.enabled && !field.key.is_empty() {
                            match field.field_type {
                                FormFieldType::Text => {
                                    form = form.text(field.key.clone(), field.value.clone());
                                }
                                FormFieldType::File => {
                                    for fp in &field.files {
                                        if let Ok(fc) = std::fs::read(fp) {
                                            let fname = std::path::Path::new(fp)
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("file")
                                                .to_string();
                                            let part =
                                                reqwest::multipart::Part::bytes(fc).file_name(fname);
                                            form = form.part(field.key.clone(), part);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    req.multipart(form)
                }
            }
        }
        HttpMethod::HEAD => client.head(&url),
        HttpMethod::OPTIONS => client.request(reqwest::Method::OPTIONS, &url),
        HttpMethod::QUERY => {
            let query_verb = reqwest::Method::from_bytes(b"QUERY").unwrap();
            client.request(query_verb, &url)
        }
    };

    (builder.headers(header_map), url)
}

// ───────────────────────── auth auto-refresh decisions ─────────────────────────

fn auto_refresh_cfg(env: Option<&Environment>) -> Option<(&Environment, &AuthRefreshConfig)> {
    let env = env?;
    let cfg = env.auth_config.as_ref()?;
    if !cfg.auto_refresh_enabled || cfg.mode == AuthRefreshMode::None {
        return None;
    }
    Some((env, cfg))
}

/// True when a Bearer tab's environment has auto-refresh on and the stored
/// access token is missing or (within 30s of being) expired. The caller
/// should run `perform_auth_refresh` first, then send.
pub fn needs_preventive_refresh(auth_type: AuthType, env: Option<&Environment>) -> bool {
    if auth_type != AuthType::Bearer {
        return false;
    }
    let Some((env, cfg)) = auto_refresh_cfg(env) else {
        return false;
    };
    let has_token = env
        .get_var(&cfg.access_token_var)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    !(has_token && !env.is_token_expired(30))
}

/// True when a 401 may trigger one refresh + automatic resend. The caller
/// must set its per-send "retry used" flag when it acts on this.
pub fn can_retry_after_auth_failure(
    auth_type: AuthType,
    retry_used: bool,
    env: Option<&Environment>,
) -> bool {
    auth_type == AuthType::Bearer && !retry_used && auto_refresh_cfg(env).is_some()
}

/// Writes a successful login/refresh result back into the environment.
pub fn apply_auth_refresh_outcome(env: &mut Environment, outcome: &AuthRefreshOutcome) {
    if let Some(cfg) = env.auth_config.clone() {
        env.set_var(&cfg.access_token_var, outcome.access_token.clone());
        if let (Some(rt), false) = (&outcome.refresh_token, cfg.refresh_token_var.is_empty()) {
            env.set_var(&cfg.refresh_token_var, rt.clone());
        }
    }
    env.token_expires_at = outcome.expires_at;
    env.auth_last_error = None;
}

/// Message shown in the response pane when a refresh fails.
pub fn auth_refresh_failure_body(err: &str) -> String {
    format!(
        "Token auto-refresh failed: {err}\n\nCheck the environment's Auth Auto-Refresh settings (Environments panel), or paste a token manually and try again."
    )
}

// ───────────────────────── cookies ─────────────────────────

/// Merges `Set-Cookie` headers from a response into the jar, keyed by the
/// request URL's domain. Same-named cookies are replaced.
pub fn merge_set_cookies(
    jar: &mut HashMap<String, Vec<CookieEntry>>,
    url: &str,
    set_cookies: &[String],
) {
    let Some(domain) = extract_domain(url) else {
        return;
    };
    for raw in set_cookies {
        if let Some(cookie) = parse_set_cookie(raw) {
            let entries = jar.entry(domain.clone()).or_default();
            if let Some(existing) = entries.iter_mut().find(|c| c.name == cookie.name) {
                *existing = cookie;
            } else {
                entries.push(cookie);
            }
        }
    }
}

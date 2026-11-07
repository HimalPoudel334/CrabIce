//! Environments and their variables.

use serde::{Deserialize, Serialize};

use crate::domain::auth::AuthRefreshConfig;
use crate::domain::variables::{VAR_RE, resolve_dynamic_var};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Environment {
    pub(crate) id: usize,
    pub(crate) name: String,
    pub(crate) variables: Vec<EnvVar>,

    #[serde(default)]
    pub(crate) auth_config: Option<AuthRefreshConfig>,
    /// Unix millis when the current access token should be considered
    /// stale. `None` means "unknown" (e.g. API gave no expiry and no
    /// default TTL was set) — treated as "not expired" until a 401 proves
    /// otherwise.
    #[serde(default)]
    pub(crate) token_expires_at: Option<i64>,
    /// Last auto-refresh error, surfaced in the environment panel so a
    /// dead refresh token or changed password doesn't fail silently.
    #[serde(default, skip_serializing)]
    pub(crate) auth_last_error: Option<String>,
}

impl Environment {
    pub(crate) fn new(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            variables: vec![EnvVar::new()],
            auth_config: None,
            token_expires_at: None,
            auth_last_error: None,
        }
    }

    pub(crate) fn get_var(&self, key: &str) -> Option<String> {
        self.variables
            .iter()
            .find(|v| v.enabled && v.key == key)
            .map(|v| v.value.clone())
    }

    /// Writes a value into a named variable, creating it (enabled) if it
    /// doesn't exist yet. Used by the auth-refresh flow to write back the
    /// resolved access/refresh tokens.
    pub(crate) fn set_var(&mut self, key: &str, value: String) {
        if key.is_empty() {
            return;
        }
        if let Some(existing) = self.variables.iter_mut().find(|v| v.key == key) {
            existing.value = value;
            existing.enabled = true;
        } else {
            self.variables.push(EnvVar {
                key: key.to_string(),
                value,
                enabled: true,
            });
        }
    }

    /// True if the stored token is known to have expired (or is about to,
    /// within `buffer_secs`). An unknown expiry is treated as not-expired —
    /// the reactive 401 retry is the backstop for that case.
    pub(crate) fn is_token_expired(&self, buffer_secs: i64) -> bool {
        match self.token_expires_at {
            Some(expires_at) => {
                let now_ms = chrono::Utc::now().timestamp_millis();
                now_ms + buffer_secs * 1000 >= expires_at
            }
            None => false,
        }
    }

    /// Substitutes {{var}} against this environment's own variables. Used
    /// for the auth-refresh login/refresh endpoint fields specifically,
    /// since e.g. `{{b}}/auth/login` should reuse the same base-url var as
    /// the rest of the environment rather than needing the full URL typed
    /// out a second time.
    pub(crate) fn resolve_own_vars(&self, input: &str) -> String {
        if !input.contains("{{") {
            return input.to_string();
        }
        VAR_RE
            .replace_all(input, |caps: &regex::Captures| {
                let name = &caps[1];
                resolve_dynamic_var(name)
                    .or_else(|| self.get_var(name))
                    .unwrap_or_else(|| caps[0].to_string())
            })
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct EnvVar {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) enabled: bool,
}

impl EnvVar {
    pub(crate) fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }
}

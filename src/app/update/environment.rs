//! `update` handlers for the **environment** messages.

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::auth::{
    AuthRefreshConfig,
    AuthRequestKind,
    EnvAuthField,
    SuggestionKind,
    build_auth_request,
    default_auth_template,
    suggest_token_fields,
};
use crate::domain::environment::{EnvVar, Environment};
use crate::domain::http::HTTP_CLIENT;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::TokenRefreshed(env_id, result) => {
            app.auth_test_loading_env = None;
            match result {
                Ok(outcome) => {
                    if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                        if let Some(cfg) = env.auth_config.clone() {
                            env.set_var(&cfg.access_token_var, outcome.access_token.clone());
                            if let (Some(rt), false) =
                                (&outcome.refresh_token, cfg.refresh_token_var.is_empty())
                            {
                                env.set_var(&cfg.refresh_token_var, rt.clone());
                            }
                        }
                        env.token_expires_at = outcome.expires_at;
                        env.auth_last_error = None;
                    }

                    let save = app.save_task();
                    if let Some(tab_id) = app.pending_resend_tab.take() {
                        if app.tabs.iter().any(|t| t.id() == tab_id) {
                            return iced::Task::batch([
                                save,
                                iced::Task::done(Message::SendRequestRetry),
                            ]);
                        }
                    }
                    save
                }
                Err(err) => {
                    if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                        env.auth_last_error = Some(err.clone());
                    }
                    app.pending_resend_tab = None;
                    if let Some(tab) = app.current_tab_mut() {
                        tab.loading = false;
                        tab.response_status = "Auth Refresh Failed".to_string();
                        tab.response_body = std::sync::Arc::from(
                            format!(
                                "Token auto-refresh failed: {err}\n\nCheck the environment's Auth Auto-Refresh settings (Environments panel), or paste a token manually and try again."
                            )
                            .as_str(),
                        );
                    }
                    app.save_task()
                }
            }
        }
        Message::EnvironmentAdd => {
            let id = app.next_environment_id;
            app.next_environment_id += 1;
            app.environments
                .push(Environment::new(id, format!("Environment {}", id)));
            app.active_environment_id = Some(id);
            app.save_task()
        }
        Message::EnvironmentSelected(id) => {
            // Selection is per-tab; we also remember it as the default a
            // brand-new tab will start with.
            app.active_environment_id = id;
            if let Some(tab) = app.current_tab_mut() {
                tab.active_environment_id = id;
            }
            app.save_task()
        }
        Message::EnvAuthPanelToggle(env_id) => {
            app.auth_config_open_env = if app.auth_config_open_env == Some(env_id) {
                None
            } else {
                Some(env_id)
            };
            iced::Task::none()
        }
        Message::EnvAuthModeChanged(env_id, mode) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                let cfg = env
                    .auth_config
                    .get_or_insert_with(AuthRefreshConfig::default);
                cfg.mode = mode;
            }
            app.save_task()
        }
        Message::EnvAuthMethodChanged(env_id, kind, method) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                let cfg = env
                    .auth_config
                    .get_or_insert_with(AuthRefreshConfig::default);
                match kind {
                    AuthRequestKind::Login => cfg.login_method = method,
                    AuthRequestKind::Refresh => cfg.refresh_method = method,
                }
            }
            app.save_task()
        }
        Message::EnvAuthBodyFormatChanged(env_id, kind, format) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                let cfg = env
                    .auth_config
                    .get_or_insert_with(AuthRefreshConfig::default);
                // Swap in the matching default template only if the user
                // hasn't customised the current one.
                let (tpl, fmt) = match kind {
                    AuthRequestKind::Login => {
                        (&mut cfg.login_body_template, &mut cfg.login_body_format)
                    }
                    AuthRequestKind::Refresh => {
                        (&mut cfg.refresh_body_template, &mut cfg.refresh_body_format)
                    }
                };
                if tpl.as_str() == default_auth_template(kind, *fmt) {
                    *tpl = default_auth_template(kind, format).to_string();
                }
                *fmt = format;
            }
            app.save_task()
        }
        Message::EnvAuthAutoRefreshToggled(env_id) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                let cfg = env
                    .auth_config
                    .get_or_insert_with(AuthRefreshConfig::default);
                cfg.auto_refresh_enabled = !cfg.auto_refresh_enabled;
            }
            app.save_task()
        }
        Message::EnvAuthFieldChanged(env_id, field, value) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                let cfg = env
                    .auth_config
                    .get_or_insert_with(AuthRefreshConfig::default);
                match field {
                    EnvAuthField::LoginEndpoint => cfg.login_endpoint = value,
                    EnvAuthField::RefreshEndpoint => cfg.refresh_endpoint = value,
                    EnvAuthField::LoginBodyTemplate => cfg.login_body_template = value,
                    EnvAuthField::RefreshBodyTemplate => cfg.refresh_body_template = value,
                    EnvAuthField::AccessTokenVar => cfg.access_token_var = value,
                    EnvAuthField::RefreshTokenVar => cfg.refresh_token_var = value,
                    EnvAuthField::AccessTokenPath => cfg.access_token_path = value,
                    EnvAuthField::RefreshTokenPath => cfg.refresh_token_path = value,
                    EnvAuthField::ExpiryPath => cfg.expiry_path = value,
                    EnvAuthField::DefaultTtlSecs => {
                        cfg.default_ttl_secs = value.trim().parse::<i64>().unwrap_or(0);
                    }
                }
            }
            app.save_task()
        }
        Message::EnvAuthTestLogin(env_id) => {
            let Some(env) = app.environments.iter().find(|e| e.id == env_id) else {
                return iced::Task::none();
            };
            if env.auth_config.is_none() {
                return iced::Task::none();
            }
            app.auth_test_loading_env = Some(env_id);
            app.auth_test_result.remove(&env_id);
            let client = HTTP_CLIENT.clone();
            let env_clone = env.clone();
            iced::Task::perform(
                async move {
                    let cfg = env_clone.auth_config.as_ref().unwrap();
                    let endpoint = cfg.login_endpoint.clone();
                    if endpoint.trim().is_empty() {
                        return Err("No login endpoint set".to_string());
                    }
                    let resp = build_auth_request(
                        &client,
                        &env_clone,
                        &endpoint,
                        cfg.login_method,
                        cfg.login_body_format,
                        &cfg.login_body_template,
                    )?
                    .send()
                    .await
                    .map_err(|e| format!("Request failed: {e}"))?;
                    let status = resp.status();
                    if !status.is_success() {
                        return Err(format!("Login endpoint returned {status}"));
                    }
                    let json: serde_json::Value = resp
                        .json()
                        .await
                        .map_err(|e| format!("Response wasn't valid JSON: {e}"))?;
                    let suggestions = suggest_token_fields(&json);
                    if suggestions.is_empty() {
                        Err("No token-shaped fields found in the response".to_string())
                    } else {
                        Ok(suggestions)
                    }
                },
                move |result| Message::EnvAuthTestLoginResult(env_id, result),
            )
        }
        Message::EnvAuthTestLoginResult(env_id, result) => {
            app.auth_test_loading_env = None;
            app.auth_test_result.insert(env_id, result);
            iced::Task::none()
        }
        Message::EnvAuthApplySuggestion(env_id, kind, path) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                let cfg = env
                    .auth_config
                    .get_or_insert_with(AuthRefreshConfig::default);
                match kind {
                    SuggestionKind::AccessToken | SuggestionKind::LooksLikeJwt => {
                        cfg.access_token_path = path;
                    }
                    SuggestionKind::RefreshToken => cfg.refresh_token_path = path,
                    SuggestionKind::Expiry => cfg.expiry_path = path,
                }
            }
            app.save_task()
        }
        Message::EnvironmentRenamed(id, name) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == id) {
                env.name = name;
            }
            app.save_task()
        }
        Message::EnvironmentDeleted(id) => {
            app.environments.retain(|e| e.id != id);
            if app.active_environment_id == Some(id) {
                app.active_environment_id = None;
            }
            app.save_task()
        }
        Message::EnvVarAdd(env_id) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                env.variables.push(EnvVar::new());
            }
            iced::Task::none()
        }
        Message::EnvVarRemove(env_id, idx) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                if idx < env.variables.len() {
                    env.variables.remove(idx);
                }
            }
            iced::Task::none()
        }
        Message::EnvVarKeyChanged(env_id, idx, key) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                if let Some(v) = env.variables.get_mut(idx) {
                    v.key = key;
                }
            }
            iced::Task::none()
        }
        Message::EnvVarValueChanged(env_id, idx, value) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                if let Some(v) = env.variables.get_mut(idx) {
                    v.value = value;
                }
            }
            iced::Task::none()
        }
        Message::EnvVarToggled(env_id, idx) => {
            if let Some(env) = app.environments.iter_mut().find(|e| e.id == env_id) {
                if let Some(v) = env.variables.get_mut(idx) {
                    v.enabled = !v.enabled;
                }
            }
            iced::Task::none()
        }
        Message::EnvPanelOpen => {
            app.env_panel_open = true;
            iced::Task::none()
        }
        Message::EnvPanelClose => {
            app.env_panel_open = false;
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

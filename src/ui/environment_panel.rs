//! Environment editor panel and per-environment auth-refresh settings.

use iced::{Alignment, Element, Length};
use iced::widget::{
    Column,
    Space,
    button,
    checkbox,
    column,
    container,
    pick_list,
    row,
    rule,
    scrollable,
    space,
    text,
    text_input,
};

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::auth::{
    AuthBodyFormat,
    AuthHttpMethod,
    AuthRefreshConfig,
    AuthRefreshMode,
    AuthRequestKind,
    EnvAuthField,
};
use crate::domain::environment::Environment;
use crate::ui::format::truncate_str;

impl CrabiPie {
    pub(crate) fn render_env_auth_section(&self, env: &Environment) -> Element<'_, Message> {
        let env_id = env.id;
        let is_open = self.auth_config_open_env == Some(env_id);

        let toggle_btn = button(text(if is_open {
            "🔐 Auth Auto-Refresh ▾"
        } else {
            "🔐 Auth Auto-Refresh ▸"
        }))
        .style(button::text)
        .on_press(Message::EnvAuthPanelToggle(env_id));

        if !is_open {
            return column![toggle_btn].into();
        }

        let default_cfg = AuthRefreshConfig::default();
        let cfg = env.auth_config.as_ref().unwrap_or(&default_cfg);

        let mode_picker = pick_list(&AuthRefreshMode::ALL[..], Some(cfg.mode), move |m| {
            Message::EnvAuthModeChanged(env_id, m)
        });
        let enabled_toggle = row![
            checkbox(cfg.auto_refresh_enabled)
                .on_toggle(move |_| Message::EnvAuthAutoRefreshToggled(env_id)),
            text("Enabled"),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        // No explicit return-type annotation here: closures have restrictions
        // on elided lifetimes in return position, so let inference resolve it
        // from how the pushes into `fields_col: Vec<Element<'_, Message>>` are used.
        let field_row = |label: &'static str, value: &str, field: EnvAuthField| {
            let el: Element<'_, Message> = row![
                text(label).width(Length::Fixed(150.0)),
                text_input("", value)
                    .on_input(move |v| Message::EnvAuthFieldChanged(env_id, field, v))
                    .width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into();
            el
        };

        // Method + body-format pickers for the login / refresh request.
        let style_row = |kind: AuthRequestKind, method: AuthHttpMethod, format: AuthBodyFormat| {
            let el: Element<'_, Message> = row![
                text("Request").width(Length::Fixed(150.0)),
                pick_list(&AuthHttpMethod::ALL[..], Some(method), move |m| {
                    Message::EnvAuthMethodChanged(env_id, kind, m)
                }),
                pick_list(&AuthBodyFormat::ALL[..], Some(format), move |f| {
                    Message::EnvAuthBodyFormatChanged(env_id, kind, f)
                }),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into();
            el
        };

        let mut fields_col: Vec<Element<'_, Message>> = vec![field_row(
            "Login endpoint",
            &cfg.login_endpoint,
            EnvAuthField::LoginEndpoint,
        )];

        if cfg.mode == AuthRefreshMode::RefreshToken {
            fields_col.push(field_row(
                "Refresh endpoint",
                &cfg.refresh_endpoint,
                EnvAuthField::RefreshEndpoint,
            ));
        }

        fields_col.push(style_row(
            AuthRequestKind::Login,
            cfg.login_method,
            cfg.login_body_format,
        ));
        fields_col.push(field_row(
            match cfg.login_body_format {
                AuthBodyFormat::Json => "Login body (JSON)",
                AuthBodyFormat::FormUrlEncoded => "Login body (form)",
            },
            &cfg.login_body_template,
            EnvAuthField::LoginBodyTemplate,
        ));
        fields_col.push(field_row(
            "Access token var",
            &cfg.access_token_var,
            EnvAuthField::AccessTokenVar,
        ));

        if cfg.mode == AuthRefreshMode::RefreshToken {
            fields_col.push(field_row(
                "Refresh token var",
                &cfg.refresh_token_var,
                EnvAuthField::RefreshTokenVar,
            ));
            fields_col.push(style_row(
                AuthRequestKind::Refresh,
                cfg.refresh_method,
                cfg.refresh_body_format,
            ));
            fields_col.push(field_row(
                match cfg.refresh_body_format {
                    AuthBodyFormat::Json => "Refresh body (JSON)",
                    AuthBodyFormat::FormUrlEncoded => "Refresh body (form)",
                },
                &cfg.refresh_body_template,
                EnvAuthField::RefreshBodyTemplate,
            ));
        }

        fields_col.push(field_row(
            "Access token path",
            &cfg.access_token_path,
            EnvAuthField::AccessTokenPath,
        ));

        if cfg.mode == AuthRefreshMode::RefreshToken {
            fields_col.push(field_row(
                "Refresh token path",
                &cfg.refresh_token_path,
                EnvAuthField::RefreshTokenPath,
            ));
        }

        fields_col.push(field_row(
            "Expiry path (optional)",
            &cfg.expiry_path,
            EnvAuthField::ExpiryPath,
        ));
        fields_col.push(field_row(
            "Default TTL (secs)",
            &cfg.default_ttl_secs.to_string(),
            EnvAuthField::DefaultTtlSecs,
        ));

        let test_label = if self.auth_test_loading_env == Some(env_id) {
            "Testing…"
        } else {
            "Test Login & Suggest Fields"
        };
        let test_btn = button(text(test_label)).on_press(Message::EnvAuthTestLogin(env_id));

        let suggestions_view: Element<'_, Message> = match self.auth_test_result.get(&env_id) {
            Some(Ok(suggestions)) => {
                let rows: Vec<Element<'_, Message>> = suggestions
                    .iter()
                    .map(|s| {
                        let path = s.path.clone();
                        let kind = s.kind;
                        let label = format!(
                            "Use as {}: {} = {}",
                            s.kind,
                            s.path,
                            truncate_str(&s.sample_value, 28)
                        );
                        button(text(label))
                            .style(button::secondary)
                            .on_press(Message::EnvAuthApplySuggestion(env_id, kind, path))
                            .into()
                    })
                    .collect();
                column![
                    text("Found these candidate fields — click to use:"),
                    Column::with_children(rows).spacing(4),
                ]
                .spacing(6)
                .into()
            }
            Some(Err(e)) => text(format!("Test login failed: {e}")).into(),
            None => Space::new().into(),
        };

        let error_view: Element<'_, Message> = match &env.auth_last_error {
            Some(err) => text(format!("⚠ Last auto-refresh error: {err}")).into(),
            None => Space::new().into(),
        };

        column![
            toggle_btn,
            row![mode_picker, enabled_toggle].spacing(16).align_y(Alignment::Center),
            text("Auto-refresh writes the token(s) into the env vars named below. Reference them in the tab's Bearer Token field as e.g. {{access_token}}.").size(12),
            Column::with_children(fields_col).spacing(6),
            row![test_btn].spacing(8),
            suggestions_view,
            error_view,
        ]
        .spacing(10)
        .into()
    }

    pub(crate) fn render_env_panel(&self) -> Element<'_, Message> {
        if !self.env_panel_open {
            return Space::new().into();
        }

        let content: Element<'_, Message> = if self.environments.is_empty() {
            text("No environments yet. Click + Add to create one.").into()
        } else {
            let sections: Vec<Element<'_, Message>> = self
                .environments
                .iter()
                .map(|env| {
                    let env_id = env.id;

                    let header = row![
                        text_input("Environment name", &env.name)
                            .on_input(move |name| Message::EnvironmentRenamed(env_id, name))
                            .width(Length::FillPortion(2)),
                        space::horizontal(),
                        button(text("➕").shaping(text::Shaping::Advanced))
                            .style(button::text)
                            .on_press(Message::EnvVarAdd(env_id)),
                        button(text("🗑").shaping(text::Shaping::Advanced))
                            .style(button::text)
                            .on_press(Message::EnvironmentDeleted(env_id)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);

                    let var_rows: Vec<Element<'_, Message>> = env
                        .variables
                        .iter()
                        .enumerate()
                        .map(|(idx, var)| {
                            let is_password = var.key.to_ascii_lowercase().contains("password");
                            row![
                                checkbox(var.enabled)
                                    .on_toggle(move |_| Message::EnvVarToggled(env_id, idx)),
                                text_input("key", &var.key)
                                    .on_input(move |k| Message::EnvVarKeyChanged(env_id, idx, k))
                                    .width(Length::FillPortion(2)),
                                text_input("value", &var.value)
                                    .on_input(move |v| Message::EnvVarValueChanged(env_id, idx, v))
                                    .secure(is_password)
                                    .width(Length::FillPortion(3)),
                                button(text("❌").shaping(text::Shaping::Advanced))
                                    .style(button::subtle)
                                    .on_press(Message::EnvVarRemove(env_id, idx)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .into()
                        })
                        .collect();

                    let auth_section = self.render_env_auth_section(env);

                    column![
                        header,
                        Column::with_children(var_rows).spacing(6),
                        auth_section,
                        rule::horizontal(1),
                    ]
                    .spacing(8)
                    .into()
                })
                .collect();

            scrollable(Column::with_children(sections).spacing(20))
                .height(Length::Fixed(320.0))
                .into()
        };

        let modal_body = column![
            row![
                text("Environments"),
                space::horizontal(),
                button("+ Add Environment")
                    .style(button::text)
                    .on_press(Message::EnvironmentAdd),
                button("✕")
                    .on_press(Message::EnvPanelClose)
                    .style(button::text),
            ]
            .align_y(Alignment::Center)
            .spacing(8),
            rule::horizontal(1),
            content,
        ]
        .spacing(12)
        .padding(20);

        let modal = container(modal_body)
            .width(Length::Fixed(640.0))
            .max_height(520)
            .style(container::rounded_box);

        let overlay = container(
            container(modal)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(iced::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            ..container::Style::default()
        });

        iced::widget::stack![
            Space::new().width(Length::Fill).height(Length::Fill),
            overlay,
        ]
        .into()
    }
}

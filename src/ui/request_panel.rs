//! Request section: URL row, body, form-data, query, headers and auth tabs.

use iced::{Alignment, Border, Element, Length, Padding, alignment};
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
    text_editor,
    text_input,
};

use crate::app::CrabiPie;
use crate::app::message::{JsonTarget, Message};
use crate::domain::auth::{ApiKeyPosition, AuthType};
use crate::domain::http::{
    ContentType,
    FormFieldType,
    HttpMethod,
    RAW_FORM_PLACEHOLDER,
    RequestType,
};
use crate::domain::view_state::{FormViewType, HeaderViewType, RequestTab};
use crate::ui::json_highlighter;

impl CrabiPie {
    pub(crate) fn render_request_row(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let req_type = pick_list(
            &RequestType::ALL[..],
            Some(tab.request_type.clone()),
            Message::RequestTypeSelected,
        )
        .width(110)
        .padding(8);

        let method_picker = pick_list(
            &HttpMethod::ALL[..],
            Some(tab.method.clone()),
            Message::MethodSelected,
        )
        .width(100)
        .padding(8);

        let url_input = text_input("https://api.example.com/endpoint", &tab.url)
            .id(tab.url_id.clone())
            .on_input(Message::UrlChanged)
            .size(16)
            .padding(8)
            .width(Length::Fill);

        let send_button = if tab.loading {
            button(
                text("🛑 Cancel")
                    .align_x(alignment::Horizontal::Center)
                    .shaping(text::Shaping::Advanced)
                    .width(Length::Fill),
            )
            .on_press(Message::CancelRequest)
            .padding(8)
            .width(100)
        } else {
            button(
                text("📤 Send")
                    .shaping(text::Shaping::Advanced)
                    .align_x(alignment::Horizontal::Center)
                    .width(Length::Fill),
            )
            .on_press_maybe(if !tab.url.trim().is_empty() {
                Some(Message::SendRequest)
            } else {
                None
            })
            .padding(8)
            .width(100)
        };

        container(row![req_type, method_picker, url_input, send_button].spacing(10))
            .padding(Padding::new(0.0).top(10.0))
            .into()
    }

    pub(crate) fn render_request_section(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let req_tabs: iced_aw::Tabs<Message, RequestTab, iced::Theme, iced::Renderer> =
            iced_aw::Tabs::new(Message::RequestTabSelected)
                .push(
                    RequestTab::Query,
                    iced_aw::TabLabel::Text("Query".into()),
                    container(self.render_query_tab()).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .push(
                    RequestTab::Body,
                    iced_aw::TabLabel::Text("Body".into()),
                    container({
                        match tab.request_type {
                            RequestType::GraphQL => self.render_graphql_tab(),
                            _ => self.render_body_tab(),
                        }
                    })
                    .padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .push(
                    RequestTab::Headers,
                    iced_aw::TabLabel::Text("Headers".into()),
                    container(self.render_headers_tab()).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .push(
                    RequestTab::Auth,
                    iced_aw::TabLabel::Text("Auth".into()),
                    container(self.render_auth_tab()).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .height(Length::Fill)
                .set_active_tab(&tab.active_request_tab)
                .tab_bar_position(iced_aw::TabBarPosition::Top)
                .into();

        let req_title = row![
            text("Request").height(20),
            space::horizontal(),
            button(text("🍪 Add Cookie").shaping(text::Shaping::Advanced))
                .height(20)
                .style(button::text)
                .on_press(Message::CookieJarOpen)
        ];

        container(column![req_title, rule::horizontal(1.0), req_tabs,].spacing(10))
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.palette().primary,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .padding(5.0)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub(crate) fn render_body_tab(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        if !matches!(
            tab.method,
            HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH
        ) {
            return text("Select POST, PUT, or PATCH to edit body.").into();
        }

        let toggle_format_or_prettify_btn =
            button(text(if tab.content_type == ContentType::Json {
                "✨ Prettify"
            } else if tab.form_view_type == FormViewType::Formatted {
                "View raw"
            } else {
                "View Formatted"
            }))
            .style(button::text)
            .on_press(if tab.content_type == ContentType::Json {
                Message::PrettifyJson(JsonTarget::RequestBody)
            } else if tab.form_view_type == FormViewType::Formatted {
                Message::ViewRawForm
            } else {
                Message::ViewFormattedForm
            });

        let type_selector = row![
            text("Type:"),
            pick_list(
                &ContentType::ALL[..],
                Some(tab.content_type.clone()),
                Message::ContentTypeSelected
            ),
            space::horizontal(),
            toggle_format_or_prettify_btn,
        ]
        .height(20)
        .spacing(10)
        .align_y(Alignment::Center);

        let editor_content = match tab.content_type {
            ContentType::Json => scrollable(
                text_editor(&self.request_body_content)
                    .on_action(Message::BodyAction)
                    .highlight_with::<json_highlighter::JsonHighlighter>(
                        self.get_highlighter_settings(),
                        |highlight, _theme| {
                            let color = match highlight {
                                json_highlighter::HighlightType::Syntax(color) => *color,
                                json_highlighter::HighlightType::SearchMatch => {
                                    iced::Color::from_rgb(1.0, 1.0, 0.0)
                                }
                                json_highlighter::HighlightType::CurrentMatch => {
                                    iced::Color::from_rgb(1.0, 0.5, 0.0)
                                }
                            };

                            iced::advanced::text::highlighter::Format {
                                color: Some(color),
                                font: None,
                            }
                        },
                    )
                    .style(Self::get_editor_style),
            )
            .height(Length::Fill)
            .into(),
            ContentType::FormData | ContentType::XWWWFormUrlEncoded => match tab.form_view_type {
                FormViewType::Formatted => self.render_form_data(),
                FormViewType::Raw => scrollable(
                    text_editor(&self.raw_form_content)
                        .placeholder(RAW_FORM_PLACEHOLDER)
                        .on_action(Message::FormRawAction)
                        .style(Self::get_editor_style)
                        .min_height(200.0),
                )
                .into(),
            },
        };

        column![type_selector, editor_content]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    pub(crate) fn render_form_data(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let is_url_encoded = matches!(tab.content_type, ContentType::XWWWFormUrlEncoded);

        let mut fields_col = Column::new().spacing(10);

        for (idx, field) in tab.form_data.iter().enumerate() {
            let effective_type = if is_url_encoded {
                FormFieldType::Text
            } else {
                field.field_type.clone()
            };

            let value_input = text_input("value", &field.value)
                .on_input(move |val| Message::FormFieldValueChanged(idx, val))
                .width(Length::Fill);

            let value_or_file: Element<'_, Message> = if effective_type == FormFieldType::Text {
                row![text("Value:"), value_input,]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .width(Length::FillPortion(2))
                    .into()
            } else {
                let file_count_text: Element<'_, Message> = if !field.files.is_empty() {
                    text(format!("📎{} file(s)", field.files.len()))
                        .shaping(text::Shaping::Advanced)
                        .into()
                } else {
                    Space::new().into()
                };

                row![
                    text("File:"),
                    button(text("📁 Choose").shaping(text::Shaping::Advanced))
                        .on_press(Message::FormFieldFileSelect(idx)),
                    file_count_text,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::FillPortion(2))
                .into()
            };

            let mut field_row = row![
                checkbox(field.enabled).on_toggle(move |_| Message::FormFieldToggled(idx)),
                text("Key:"),
                text_input("key", &field.key)
                    .on_input(move |key| Message::FormFieldKeyChanged(idx, key))
                    .width(Length::FillPortion(1)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill);

            if !is_url_encoded {
                field_row = field_row.push(pick_list(
                    &FormFieldType::ALL[..],
                    Some(field.field_type.clone()),
                    move |ft| Message::FormFieldTypeSelected(idx, ft),
                ));
            }

            field_row = field_row.push(value_or_file).push(
                button(text("❌").shaping(text::Shaping::Advanced))
                    .style(button::subtle)
                    .on_press(Message::FormFieldRemove(idx)),
            );

            fields_col = fields_col.push(field_row);

            if effective_type == FormFieldType::File && !field.files.is_empty() && !is_url_encoded {
                let mut files_col = Column::new().spacing(4);

                for file in &field.files {
                    let filename = std::path::Path::new(file)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(file);

                    files_col = files_col.push(text(format!(" • {filename}")).size(13));
                }

                fields_col = fields_col.push(container(files_col).padding(Padding {
                    left: 20.0,
                    ..Default::default()
                }));
            }
        }

        fields_col = fields_col.push(
            button(text("➕ Add").shaping(text::Shaping::Advanced))
                .on_press(Message::FormFieldAdd)
                .style(button::subtle),
        );

        scrollable(fields_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    pub(crate) fn render_query_tab(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let mut params_col = Column::new().spacing(10);

        for (idx, param) in tab.query_params.iter().enumerate() {
            let checkbox =
                checkbox(param.enabled).on_toggle(move |_| Message::QueryParamToggled(idx));

            let key_input = text_input("key", &param.key)
                .on_input(move |key| Message::QueryParamKeyChanged(idx, key))
                .width(Length::FillPortion(1));

            let value_input = text_input("value", &param.value)
                .on_input(move |val| Message::QueryParamValueChanged(idx, val))
                .width(Length::FillPortion(2));

            let remove_btn = button(text("❌").shaping(text::Shaping::Advanced))
                .style(button::text)
                .on_press(Message::QueryParamRemove(idx));

            let param_row = row![
                checkbox,
                text("Key:"),
                key_input,
                text("Value:"),
                value_input,
                remove_btn,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Length::Fill);

            params_col = params_col.push(param_row);
        }

        params_col = params_col.push(
            button(text("➕ Add").shaping(text::Shaping::Advanced))
                .style(button::subtle)
                .on_press(Message::QueryParamAdd),
        );

        scrollable(params_col)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    pub(crate) fn render_headers_tab(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let picker = row![
            space::horizontal(),
            pick_list(
                &HeaderViewType::ALL[..],
                Some(tab.header_view_type.clone()),
                Message::HeadersViewTypeChanged,
            )
            .padding(Padding::new(0.0).top(0.0))
        ];

        let content = match tab.header_view_type {
            HeaderViewType::Formatted => self.render_formatted_headers(),

            HeaderViewType::Raw => scrollable(
                text_editor(&self.raw_headers_content)
                    .placeholder("Header: Value")
                    .on_action(Message::HeaderRawAction)
                    .style(Self::get_editor_style)
                    .min_height(200.0),
            )
            .height(Length::Fill)
            .into(),
        };

        column![picker, content]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    pub(crate) fn render_formatted_headers(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let mut headers_col = Column::new().spacing(10);

        for (idx, header) in tab.headers.iter().enumerate() {
            let checkbox = checkbox(header.enabled).on_toggle(move |_| Message::HeaderToggled(idx));

            let key_input = text_input("key", &header.key)
                .on_input(move |key| Message::HeaderKeyChanged(idx, key))
                .width(Length::FillPortion(1));

            let value_input = text_input("value", &header.value)
                .on_input(move |val| Message::HeaderValueChanged(idx, val))
                .width(Length::FillPortion(2));

            let remove_btn = button(text("❌").shaping(text::Shaping::Advanced))
                .style(button::subtle)
                .on_press(Message::HeaderRemove(idx));

            headers_col = headers_col.push(
                row![
                    checkbox,
                    text("Key:"),
                    key_input,
                    text("Value:"),
                    value_input,
                    remove_btn,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            );
        }

        headers_col = headers_col.push(
            button(text("➕ Add").shaping(text::Shaping::Advanced))
                .style(button::subtle)
                .on_press(Message::HeaderAdd),
        );

        scrollable(headers_col).height(Length::Fill).into()
    }

    pub(crate) fn render_auth_tab(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let type_selector = row![
            text("Type:"),
            pick_list(
                &AuthType::ALL[..],
                Some(tab.auth_type.clone()),
                Message::AuthTypeSelected
            )
            .width(150),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let auth_form: Element<'_, Message> = match tab.auth_type {
            AuthType::None => Space::new().into(),

            AuthType::Bearer => row![
                text("Token:"),
                text_input("", &tab.bearer_token)
                    .on_input(Message::BearerTokenChanged)
                    .width(Length::Fill)
                    .padding(8)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),

            AuthType::Basic => row![
                column![text("Username:"), text("Password:"),]
                    .spacing(10)
                    .align_x(Alignment::End),
                column![
                    text_input("Username", &tab.username)
                        .on_input(Message::UsernameChanged)
                        .width(Length::Fill)
                        .padding(8),
                    text_input("Password", &tab.password)
                        .on_input(Message::PasswordChanged)
                        .secure(true) // hides the password
                        .width(Length::Fill)
                        .padding(8),
                ]
                .spacing(10)
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),

            AuthType::ApiKey => row![
                column![text("Key:"), text("Value:"), text("Add to:"),]
                    .align_x(Alignment::Center)
                    .spacing(10),
                column![
                    text_input("Key", &tab.api_key_name)
                        .on_input(Message::ApiKeyNameChanged)
                        .width(Length::Fill),
                    text_input("Value", &tab.api_key)
                        .on_input(Message::ApiKeyChanged)
                        .width(Length::Fill),
                    pick_list(
                        &ApiKeyPosition::ALL[..],
                        Some(tab.api_key_position),
                        Message::ApiKeyPositionChanged
                    )
                ]
                .spacing(10),
            ]
            .align_y(Alignment::Center)
            .spacing(10)
            .into(),
        };

        column![type_selector, auth_form].spacing(10).into()
    }
}

use iced::theme;
#[allow(unused)]
use iced::{
    Alignment, Border, Element, Event, Length, Padding, alignment, highlighter,
    widget::{
        Column, Space, button, column, container, horizontal_space, pick_list, row, scrollable,
        text, text_editor, text_input, tooltip,
    },
};

#[derive(Debug, Clone)]
enum Message {
    UrlChanged(String),
    MethodSelected(HttpMethod),
    HeadersAction(text_editor::Action),
    BodyAction(text_editor::Action),
    AuthTypeSelected(AuthType),
    BearerTokenChanged(String),
    ContentTypeSelected(ContentType),
    SendRequest,
    ResponseReceived(HttpResponse),
    RequestTabSelected(RequestTab),
    ResponseTabSelected(ResponseTab),
    ToggleLayout,
    PrettifyJson,
    CopyToClipboard,
    ResetCopied,
    JsonThemeChanged(highlighter::Theme),
    AppThemeChanged(iced::Theme),

    // Form data messages
    FormFieldKeyChanged(usize, String),
    FormFieldValueChanged(usize, String),
    FormFieldTypeSelected(usize, FormFieldType),
    FormFieldFileSelect(usize),
    FormFieldFilesSelected(usize, Vec<String>),
    FormFieldRemove(usize),
    FormFieldAdd,

    // Find/Replace messages
    ToggleFindDialog,
    ToggleFindReplaceDialog,
    CloseFindDialog,
    FindTextChanged(String),
    ReplaceTextChanged(String),
    ToggleCaseSensitive,
    ToggleWholeWord,
    FindNext,
    FindPrevious,
    Replace,
    ReplaceAll,

    EventOccurred(Event),
}

struct CrabiPie {
    // Request configuration
    url: String,
    method: HttpMethod,
    headers_content: text_editor::Content,
    body_content: text_editor::Content,
    auth_type: AuthType,
    bearer_token: String,
    content_type: ContentType,
    form_data: Vec<FormField>,

    // Response data
    response_status: String,
    response_headers_content: text_editor::Content,
    response_body_content: text_editor::Content,
    is_response_binary: bool,
    response_filename: String,
    response_bytes: Vec<u8>,
    response_content_type: String,

    // UI state
    loading: bool,
    active_request_tab: RequestTab,
    active_response_tab: ResponseTab,
    copied: bool,
    json_theme: highlighter::Theme,
    app_theme: iced::Theme,

    // Find dialog
    find_dialog_open: bool,
    find_replace_mode: bool,
    find_text: String,
    replace_text: String,
    case_sensitive: bool,
    whole_word: bool,
}

impl Default for CrabiPie {
    fn default() -> Self {
        Self {
            url: "https://jsonplaceholder.typicode.com/posts".to_string(),
            method: HttpMethod::GET,
            headers_content: text_editor::Content::with_text(HEADERS_DEFAULT),
            body_content: text_editor::Content::with_text(BODY_DEFAULT),
            auth_type: AuthType::None,
            bearer_token: String::new(),
            content_type: ContentType::Json,
            form_data: vec![FormField {
                key: String::new(),
                value: String::new(),
                files: Vec::new(),
                field_type: FormFieldType::Text,
            }],
            response_status: String::new(),
            response_headers_content: text_editor::Content::new(),
            response_body_content: text_editor::Content::new(),
            is_response_binary: false,
            response_filename: String::new(),
            response_bytes: Vec::new(),
            response_content_type: String::new(),
            loading: false,
            active_request_tab: RequestTab::Body,
            active_response_tab: ResponseTab::None,
            copied: false,
            find_dialog_open: false,
            find_replace_mode: false,
            find_text: String::new(),
            replace_text: String::new(),
            case_sensitive: false,
            whole_word: false,
            json_theme: highlighter::Theme::SolarizedDark,
            app_theme: iced::Theme::default(),
        }
    }
}

impl CrabiPie {
    fn render_request_section(&self) -> Element<'_, Message> {
        let tabs = row![
            button(if self.active_request_tab == RequestTab::Body {
                "[Body]"
            } else {
                "Body"
            })
            .on_press(Message::RequestTabSelected(RequestTab::Body))
            .style(button::success),
            button(if self.active_request_tab == RequestTab::Headers {
                "[Headers]"
            } else {
                "Headers"
            })
            .on_press(Message::RequestTabSelected(RequestTab::Headers))
            .style(button::text),
            button(if self.active_request_tab == RequestTab::Auth {
                "[Auth]"
            } else {
                "Auth"
            })
            .on_press(Message::RequestTabSelected(RequestTab::Auth))
            .style(button::text),
        ]
        .spacing(5);

        let content = match self.active_request_tab {
            RequestTab::Body => self.render_body_tab(),
            RequestTab::Headers => self.render_headers_tab(),
            RequestTab::Auth => self.render_auth_tab(),
        };

        container(
            column![
                text("Request"),
                tabs,
                iced::widget::horizontal_rule(1.0),
                content
            ]
            .spacing(10)
            .padding(10),
        )
        .style(|theme: &iced::Theme| container::Style {
            border: Border {
                width: 1.5,
                color: theme.palette().background,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn render_body_tab(&self) -> Element<'_, Message> {
        if !matches!(
            self.method,
            HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH
        ) {
            return text("Select POST, PUT, or PATCH to edit body.").into();
        }

        let prettify_button: Element<'_, Message> = if self.content_type == ContentType::Json {
            button(text("✨ Prettify").shaping(text::Shaping::Advanced))
                .on_press(Message::PrettifyJson)
                .into()
        } else {
            Space::new(0, 0).into()
        };

        let type_selector = row![
            text("Type:"),
            pick_list(
                &ContentType::ALL[..],
                Some(self.content_type.clone()),
                Message::ContentTypeSelected
            )
            .width(150),
            horizontal_space(),
            prettify_button,
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let editor_content = match self.content_type {
            ContentType::Json => scrollable(
                text_editor(&self.body_content)
                    .on_action(Message::BodyAction)
                    .highlight("json", self.json_theme)
                    .height(Length::Shrink),
            )
            .height(Length::Fill)
            .into(),
            ContentType::FormData => self.render_form_data(),
        };

        column![type_selector, editor_content]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    fn render_form_data(&self) -> Element<'_, Message> {
        let mut fields_col = Column::new().spacing(10);

        for (idx, field) in self.form_data.iter().enumerate() {
            // Match arm returning Element<Message> for the value/file part
            let value_or_file: Element<'_, Message> = match field.field_type {
                FormFieldType::Text => row![
                    text("Value:"),
                    text_input("value", &field.value)
                        .on_input(move |val| Message::FormFieldValueChanged(idx, val))
                        .width(200),
                ]
                .spacing(5)
                .into(),

                FormFieldType::File => {
                    let file_count_text: Element<'_, Message> = if !field.files.is_empty() {
                        text(format!("📎 {} file(s)", field.files.len())).into()
                    } else {
                        Space::new(0, 0).into()
                    };

                    row![
                        text("File:"),
                        button("📁 Choose").on_press(Message::FormFieldFileSelect(idx)),
                        file_count_text
                    ]
                    .spacing(5)
                    .into()
                }
            };

            let field_row = row![
                text("Key:"),
                text_input("key", &field.key)
                    .on_input(move |key| Message::FormFieldKeyChanged(idx, key))
                    .width(150),
                pick_list(
                    &FormFieldType::ALL[..],
                    Some(field.field_type.clone()),
                    move |ft| Message::FormFieldTypeSelected(idx, ft)
                )
                .width(80),
                value_or_file,
                button("❌").on_press(Message::FormFieldRemove(idx)),
            ]
            .spacing(10)
            .align_y(Alignment::Center);

            fields_col = fields_col.push(field_row);

            if field.field_type == FormFieldType::File && !field.files.is_empty() {
                let mut files_col = Column::new().spacing(2);
                for file in &field.files {
                    let filename = std::path::Path::new(file)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(file);
                    files_col = files_col.push(text(format!("  • {}", filename)).size(12));
                }
                fields_col = fields_col.push(container(files_col).padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 20.0,
                }));
            }
        }

        fields_col = fields_col.push(button("➕ Add Field").on_press(Message::FormFieldAdd));

        scrollable(fields_col).height(Length::Fill).into()
    }

    fn render_headers_tab(&self) -> Element<'_, Message> {
        scrollable(
            text_editor(&self.headers_content)
                .on_action(Message::HeadersAction)
                .highlight("json", self.json_theme)
                .height(Length::Shrink),
        )
        .height(Length::Fill)
        .into()
    }

    fn render_auth_tab(&self) -> Element<'_, Message> {
        let type_selector = row![
            text("Type:"),
            pick_list(
                &AuthType::ALL[..],
                Some(self.auth_type.clone()),
                Message::AuthTypeSelected
            )
            .width(150),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let token_input: Element<'_, Message> = if self.auth_type == AuthType::Bearer {
            row![
                text("Token:"),
                text_input("", &self.bearer_token)
                    .on_input(Message::BearerTokenChanged)
                    .width(Length::Fill)
                    .padding(10)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        } else {
            Space::new(0, 0).into()
        };

        column![type_selector, token_input].spacing(10).into()
    }

    fn render_response_section(&self) -> Element<'_, Message> {
        let status_view: Element<'_, Message> = if self.loading {
            text("Loading...").into()
        } else if !self.response_status.is_empty() {
            text(&self.response_status).into()
        } else {
            Space::new(0, 0).into()
        };

        let header_row =
            row![text("Response"), horizontal_space(), status_view,].align_y(Alignment::Center);

        let tabs = row![
            button(if self.active_response_tab == ResponseTab::Body {
                "[Body]"
            } else {
                "Body"
            })
            .on_press(Message::ResponseTabSelected(ResponseTab::Body))
            .style(button::text),
            button(if self.active_response_tab == ResponseTab::Headers {
                "[Headers]"
            } else {
                "Headers"
            })
            .on_press(Message::ResponseTabSelected(ResponseTab::Headers))
            .style(button::text),
            horizontal_space(),
            text("Json Theme: "),
            pick_list(
                &highlighter::Theme::ALL[..],
                Some(&self.json_theme),
                Message::JsonThemeChanged,
            ),
            tooltip(
                button(
                    text(if self.copied { "✅" } else { "📋" }).shaping(text::Shaping::Advanced)
                )
                .on_press(Message::CopyToClipboard)
                .style(button::text),
                if self.copied {
                    "Copied"
                } else {
                    "Copy to Clipboard"
                },
                tooltip::Position::Bottom
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let content: Element<Message> = match self.active_response_tab {
            ResponseTab::None => Space::new(0, 0).into(),
            ResponseTab::Body => {
                if self.is_response_binary {
                    column![
                        text(format!(
                            "📄 Binary file received: {}",
                            self.response_filename
                        ))
                        .style(|_| text::Style {
                            color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                        }),
                        text(format!("Size: {} bytes", self.response_bytes.len())),
                    ]
                    .spacing(10)
                    .into()
                } else {
                    scrollable(
                        text_editor(&self.response_body_content)
                            .on_action(Message::BodyAction)
                            .highlight("json", self.json_theme)
                            .height(Length::Shrink),
                    )
                    .height(Length::Fill)
                    .into()
                }
            }
            ResponseTab::Headers => scrollable(
                text_editor(&self.response_headers_content)
                    .on_action(Message::HeadersAction)
                    .highlight("json", self.json_theme)
                    .height(Length::Shrink),
            )
            .height(Length::Fill)
            .into(),
        };

        container(
            column![
                header_row,
                tabs,
                iced::widget::horizontal_rule(1.0),
                content
            ]
            .spacing(10)
            .padding(10),
        )
        .style(|theme: &iced::Theme| container::Style {
            border: Border {
                width: 1.5,
                color: theme.palette().background,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn send_request(&self) -> iced::Task<Message> {
        let url = self.url.clone();
        let method = self.method.clone();
        let body = self.body_content.text();
        let headers_text = self.headers_content.text();
        let auth_type = self.auth_type.clone();
        let bearer_token = self.bearer_token.clone();
        let content_type = self.content_type.clone();
        let form_data = self.form_data.clone();

        iced::Task::perform(
            async move {
                // Parse headers
                let mut header_map = reqwest::header::HeaderMap::new();

                for line in headers_text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }

                    if let Some((key, value)) = line.split_once(':') {
                        let key = key.trim();
                        let value = value.trim();

                        if let (Ok(header_name), Ok(header_value)) = (
                            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                            reqwest::header::HeaderValue::from_str(value),
                        ) {
                            header_map.insert(header_name, header_value);
                        }
                    }
                }
                // Add auth header
                if auth_type == AuthType::Bearer && !bearer_token.is_empty() {
                    if let Ok(hv) =
                        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", bearer_token))
                    {
                        header_map.insert(reqwest::header::AUTHORIZATION, hv);
                    }
                }

                let client = reqwest::Client::new();

                let mut request = match method {
                    HttpMethod::GET => client.get(&url),
                    HttpMethod::DELETE => client.delete(&url),
                    HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH => {
                        let req = match method {
                            HttpMethod::POST => client.post(&url),
                            HttpMethod::PUT => client.put(&url),
                            HttpMethod::PATCH => client.patch(&url),
                            _ => unreachable!(),
                        };

                        match content_type {
                            ContentType::Json => {
                                req.body(body).header("Content-Type", "application/json")
                            }
                            ContentType::FormData => {
                                let mut form = reqwest::multipart::Form::new();
                                for field in form_data {
                                    if !field.key.is_empty() {
                                        match field.field_type {
                                            FormFieldType::Text => {
                                                form = form.text(field.key, field.value);
                                            }
                                            FormFieldType::File => {
                                                for fp in field.files {
                                                    if let Ok(fc) = std::fs::read(&fp) {
                                                        let fname = std::path::Path::new(&fp)
                                                            .file_name()
                                                            .and_then(|n| n.to_str())
                                                            .unwrap_or("file")
                                                            .to_string();
                                                        let part =
                                                            reqwest::multipart::Part::bytes(fc)
                                                                .file_name(fname);
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
                };

                request = request.headers(header_map.to_owned());

                match request.send().await {
                    Ok(resp) => {
                        let status = format!(
                            "{} {}",
                            resp.status().as_u16(),
                            resp.status().canonical_reason().unwrap_or("")
                        );
                        let hm = resp.headers().clone();
                        let headers = format!("{:#?}", hm);
                        let ct = hm
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string();

                        let is_binary = ct.starts_with("image/")
                            || ct.starts_with("application/pdf")
                            || ct.starts_with("application/octet-stream")
                            || ct.starts_with("video/")
                            || ct.starts_with("audio/");

                        let filename = hm
                            .get("content-disposition")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| {
                                s.split("filename=")
                                    .nth(1)
                                    .map(|f| f.trim_matches(|c| c == '"' || c == '\'').to_string())
                            })
                            .unwrap_or_else(|| {
                                url.split('/').last().unwrap_or("download").to_string()
                            });

                        let (body, bytes) = if is_binary {
                            match resp.bytes().await {
                                Ok(b) => (
                                    format!(
                                        "Binary file ({} bytes)\n\nContent-Type: {}",
                                        b.len(),
                                        ct
                                    ),
                                    b.to_vec(),
                                ),
                                Err(e) => (format!("Error reading binary data: {}", e), Vec::new()),
                            }
                        } else {
                            let bt = resp
                                .text()
                                .await
                                .unwrap_or_else(|e| format!("Error reading body: {}", e));
                            let body = if let Ok(j) = serde_json::from_str::<serde_json::Value>(&bt)
                            {
                                serde_json::to_string_pretty(&j).unwrap_or(bt)
                            } else {
                                bt
                            };
                            (body, Vec::new())
                        };

                        HttpResponse {
                            status,
                            headers,
                            body,
                            is_binary,
                            filename,
                            bytes,
                            content_type: ct,
                        }
                    }
                    Err(e) => HttpResponse {
                        status: "Error".to_string(),
                        headers: String::new(),
                        body: format!("Request failed: {}", e),
                        is_binary: false,
                        filename: String::new(),
                        bytes: Vec::new(),
                        content_type: String::new(),
                    },
                }
            },
            Message::ResponseReceived,
        )
    }
}

fn update(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::MethodSelected(method) => {
            app.method = method;
            iced::Task::none()
        }
        Message::UrlChanged(url) => {
            app.url = url;
            iced::Task::none()
        }
        Message::SendRequest => {
            if !app.loading && !app.url.trim().is_empty() {
                app.loading = true;
                app.send_request()
            } else {
                iced::Task::none()
            }
        }
        Message::RequestTabSelected(request_tab) => {
            app.active_request_tab = request_tab;
            iced::Task::none()
        }
        Message::HeadersAction(action) => {
            app.headers_content.perform(action);
            iced::Task::none()
        }
        Message::BodyAction(action) => {
            app.response_body_content.perform(action);
            iced::Task::none()
        }
        Message::AuthTypeSelected(auth_type) => {
            app.auth_type = auth_type;
            iced::Task::none()
        }
        Message::BearerTokenChanged(token) => {
            app.bearer_token = token;
            iced::Task::none()
        }
        Message::ContentTypeSelected(content_type) => {
            app.content_type = content_type;
            iced::Task::none()
        }
        Message::ResponseReceived(resp) => {
            app.response_status = resp.status;
            app.response_headers_content = text_editor::Content::with_text(&resp.headers);
            app.response_body_content = text_editor::Content::with_text(&resp.body);
            app.is_response_binary = resp.is_binary;
            app.response_filename = resp.filename;
            app.response_bytes = resp.bytes;
            app.response_content_type = resp.content_type;
            app.loading = false;
            app.active_response_tab = ResponseTab::Body;
            iced::Task::none()
        }
        Message::ResponseTabSelected(response_tab) => {
            app.active_response_tab = response_tab;
            iced::Task::none()
        }
        Message::ToggleLayout => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::PrettifyJson => {
            let body_text = app.body_content.text();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                    app.body_content = text_editor::Content::with_text(&pretty);
                }
            }
            iced::Task::none()
        }
        Message::CopyToClipboard => {
            app.copied = true;
            let text = app.response_body_content.text();

            iced::Task::perform(
                async {
                    let _ = iced::clipboard::write::<String>(text);
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                },
                |_| Message::ResetCopied,
            )
        }
        Message::ResetCopied => {
            app.copied = false;
            iced::Task::none()
        }
        Message::FormFieldKeyChanged(index, key) => {
            if let Some(field) = app.form_data.get_mut(index) {
                field.key = key;
            }
            iced::Task::none()
        }
        Message::FormFieldValueChanged(index, value) => {
            if let Some(field) = app.form_data.get_mut(index) {
                field.value = value;
            }
            iced::Task::none()
        }
        Message::FormFieldTypeSelected(idx, form_field_type) => {
            if let Some(field) = app.form_data.get_mut(idx) {
                field.field_type = form_field_type;
                field.value.clear();
                field.files.clear();
            }
            iced::Task::none()
        }
        Message::JsonThemeChanged(theme) => {
            app.json_theme = theme;
            iced::Task::none()
        }
        Message::AppThemeChanged(theme) => {
            app.app_theme = theme;
            iced::Task::none()
        }
        Message::FormFieldFileSelect(_) => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::FormFieldFilesSelected(_, _items) => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::FormFieldRemove(index) => {
            app.form_data.remove(index);
            iced::Task::none()
        }
        Message::FormFieldAdd => {
            app.form_data.push(FormField {
                key: String::new(),
                value: String::new(),
                files: Vec::new(),
                field_type: FormFieldType::Text,
            });
            iced::Task::none()
        }
        Message::ToggleFindDialog => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::ToggleFindReplaceDialog => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::CloseFindDialog => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::FindTextChanged(_) => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::ReplaceTextChanged(_) => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::ToggleCaseSensitive => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::ToggleWholeWord => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::FindNext => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::FindPrevious => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::Replace => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::ReplaceAll => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::EventOccurred(_event) => {
            println!("Event fired");
            iced::Task::none()
        }
    }
}

fn view(app: &CrabiPie) -> Element<'_, Message> {
    let title_row = row![
        text("CrabiPie HTTP Client").size(16),
        horizontal_space(),
        text("App theme"),
        pick_list(
            &iced::Theme::ALL[..],
            Some(&app.app_theme),
            Message::AppThemeChanged,
        )
    ]
    .spacing(10);

    let method_picker = pick_list(
        &HttpMethod::ALL[..],
        Some(app.method.clone()),
        Message::MethodSelected,
    )
    .width(100)
    .padding(10);

    let url_input = text_input("https://api.example.com/endpoint", &app.url)
        .on_input(Message::UrlChanged)
        .padding(10)
        .width(Length::Fill);

    let send_button = button(
        text(if app.loading { "Sending…" } else { "Send" })
            .align_x(alignment::Horizontal::Center)
            .width(Length::Fill),
    )
    .on_press_maybe(if !app.loading && !app.url.trim().is_empty() {
        Some(Message::SendRequest)
    } else {
        None
    })
    .padding(10)
    .width(100);

    let request_row = container(row![method_picker, url_input, send_button].spacing(10))
        .style(|theme: &iced::Theme| container::Style {
            border: Border {
                width: 1.5,
                color: theme.palette().background,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .padding(10);

    let app_container = container(
        column![
            title_row,
            request_row,
            row![app.render_request_section(), app.render_response_section()].spacing(10)
        ]
        .spacing(10),
    )
    .padding(10);

    app_container.into()
}

fn main() -> iced::Result {
    iced::application("CrabiPie", update, view)
        .theme(|app| app.app_theme.clone())
        .window_size(iced::Size::new(1500.0, 800.0))
        .run()
}

const HEADERS_DEFAULT: &str =
    "# Add headers as key: value pairs\n# Example:\n# X-Custom-Header: value";

const BODY_DEFAULT: &str = r#"{
  "title": "foo",
  "body": "bar",
  "userId": 1,
  "foo": "bar"
}"#;

#[derive(Debug, Clone, PartialEq)]
enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl HttpMethod {
    const ALL: [HttpMethod; 5] = [
        HttpMethod::GET,
        HttpMethod::POST,
        HttpMethod::PUT,
        HttpMethod::DELETE,
        HttpMethod::PATCH,
    ];
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum RequestTab {
    Body,
    Headers,
    Auth,
}

#[derive(Debug, Clone, PartialEq)]
enum ContentType {
    Json,
    FormData,
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Json => write!(f, "JSON"),
            ContentType::FormData => write!(f, "Form Data"),
        }
    }
}

impl ContentType {
    const ALL: [ContentType; 2] = [ContentType::Json, ContentType::FormData];
}

#[derive(Debug, Clone, PartialEq)]
enum FormFieldType {
    Text,
    File,
}

impl std::fmt::Display for FormFieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormFieldType::Text => write!(f, "Text"),
            FormFieldType::File => write!(f, "File"),
        }
    }
}

impl FormFieldType {
    const ALL: [FormFieldType; 2] = [FormFieldType::Text, FormFieldType::File];
}

#[derive(Debug, Clone)]
struct FormField {
    key: String,
    value: String,
    files: Vec<String>,
    field_type: FormFieldType,
}

#[derive(Debug, Clone, PartialEq)]
enum AuthType {
    None,
    Bearer,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "No Auth"),
            AuthType::Bearer => write!(f, "Bearer Token"),
        }
    }
}

impl AuthType {
    const ALL: [AuthType; 2] = [AuthType::None, AuthType::Bearer];
}

#[derive(Debug, Clone, PartialEq)]
enum ResponseTab {
    None,
    Body,
    Headers,
}

#[derive(Debug, Clone)]
struct HttpResponse {
    status: String,
    headers: String,
    body: String,
    is_binary: bool,
    filename: String,
    bytes: Vec<u8>,
    content_type: String,
}

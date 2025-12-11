#![allow(unused)]

use std::sync::atomic::Ordering;

use iced::{
    Alignment, Border, Element, Event, Length, Padding, alignment,
    border::width,
    highlighter,
    widget::{
        Column, Space, button, checkbox, column, container, pick_list, row, rule, scrollable,
        space, text, text_editor, text_input, tooltip,
    },
};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

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
    ResponseBodyAction(text_editor::Action),
    ResponseHeadersAction(text_editor::Action),
    ToggleLayout,
    PrettifyJson,
    CopyToClipboard,
    ResetCopied,
    JsonThemeChanged(highlighter::Theme),
    AppThemeChanged(iced::Theme),
    SaveRequest,
    LoadRequest,
    RequestLoaded(SavedState),
    RequestLoadFailed(String),
    CancelRequest,
    SaveBinaryResponse,
    FileSaved(Result<String, String>),

    // Query params
    QueryParamAdd,
    QueryParamRemove(usize),
    QueryParamKeyChanged(usize, String),
    QueryParamValueChanged(usize, String),
    QueryParamToggled(usize),

    //Subscription
    Tick,

    //Video
    TogglePause,
    VideoVolume(f64),
    ToggleLoop,
    Seek(f64),
    SeekRelease,
    EndOfStream,
    NewFrame,

    // Form data messages
    FormFieldKeyChanged(usize, String),
    FormFieldValueChanged(usize, String),
    FormFieldTypeSelected(usize, FormFieldType),
    FormFieldFileSelect(usize),
    FormFieldFilesSelected(usize, Vec<String>),
    FormFieldRemove(usize),
    FormFieldAdd,
    FormFieldToggled(usize),

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
    url_id: iced::widget::Id,
    base_url: String,
    url: String,
    method: HttpMethod,
    headers_content: text_editor::Content,
    body_content: text_editor::Content,
    auth_type: AuthType,
    bearer_token: String,
    content_type: ContentType,
    query_params: Vec<QueryParam>,
    form_data: Vec<FormField>,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    //video response
    video_player: Option<iced_video_player::Video>,
    video_state: Option<VideoState>,

    // Response data
    response_status: String,
    response_headers_content: text_editor::Content,
    response_body_content: text_editor::Content,
    is_response_binary: bool,
    response_filename: String,
    response_bytes: Vec<u8>,
    response_content_type: String,
    response_time: Option<std::time::Duration>,

    // UI state
    loading: bool,
    active_request_tab: RequestTab,
    active_response_tab: ResponseTab,
    copied: bool,
    json_theme: highlighter::Theme,
    app_theme: iced::Theme,
    svg_rotation: f32,

    // Find dialog
    find_dialog_open: bool,
    find_replace_mode: bool,
    find_text: String,
    replace_text: String,
    case_sensitive: bool,
    whole_word: bool,
}

impl CrabiPie {
    fn new() -> (Self, iced::Task<Message>) {
        let base = "https://jsonplaceholder.typicode.com/posts".to_string();
        (
            Self {
                url_id: iced::widget::Id::unique(),
                base_url: base.clone(),
                url: base,
                method: HttpMethod::GET,
                headers_content: text_editor::Content::with_text(HEADERS_DEFAULT),
                body_content: text_editor::Content::with_text(BODY_DEFAULT),
                auth_type: AuthType::None,
                bearer_token: String::new(),
                content_type: ContentType::Json,
                query_params: vec![QueryParam::new()],
                form_data: vec![FormField::new()],
                video_player: None,
                video_state: None,
                cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                response_status: String::new(),
                response_headers_content: text_editor::Content::new(),
                response_body_content: text_editor::Content::new(),
                is_response_binary: false,
                response_filename: String::new(),
                response_bytes: Vec::new(),
                response_content_type: String::new(),
                response_time: None,
                loading: false,
                svg_rotation: 0.0,
                active_request_tab: RequestTab::Query,
                active_response_tab: ResponseTab::Body,
                copied: false,
                find_dialog_open: false,
                find_replace_mode: false,
                find_text: String::new(),
                replace_text: String::new(),
                case_sensitive: false,
                whole_word: false,
                json_theme: highlighter::Theme::SolarizedDark,
                app_theme: iced::Theme::CatppuccinMocha,
            },
            iced::Task::none(),
        )
    }
}

impl CrabiPie {
    fn render_request_section(&self) -> Element<'_, Message> {
        let tabs = row![
            button(if self.active_request_tab == RequestTab::Query {
                "[Query]"
            } else {
                "Query"
            })
            .on_press(Message::RequestTabSelected(RequestTab::Query))
            .style(button::text),
            button(if self.active_request_tab == RequestTab::Body {
                "[Body]"
            } else {
                "Body"
            })
            .on_press(Message::RequestTabSelected(RequestTab::Body))
            .style(button::text),
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
            RequestTab::Query => self.render_query_tab(),
            RequestTab::Body => self.render_body_tab(),
            RequestTab::Headers => self.render_headers_tab(),
            RequestTab::Auth => self.render_auth_tab(),
        };

        container(
            column![text("Request"), tabs, rule::horizontal(1.0), content]
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
            Space::new().into()
        };

        let type_selector = row![
            text("Type:"),
            pick_list(
                &ContentType::ALL[..],
                Some(self.content_type.clone()),
                Message::ContentTypeSelected
            ),
            space::horizontal(),
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
            _ => self.render_form_data(),
        };

        column![type_selector, editor_content]
            .spacing(10)
            .height(Length::Fill)
            .into()
    }

    fn render_form_data(&self) -> Element<'_, Message> {
        let is_url_encoded = matches!(self.content_type, ContentType::XWWWFormUrlEncoded);

        let mut fields_col = Column::new().spacing(10);

        for (idx, field) in self.form_data.iter().enumerate() {
            // Force text type if URL-encoded
            let effective_type = if is_url_encoded {
                FormFieldType::Text
            } else {
                field.field_type.clone()
            };

            // Value input (always shown)
            let value_input = text_input("value", &field.value)
                .on_input(move |val| Message::FormFieldValueChanged(idx, val))
                .width(280);

            let value_or_file: Element<'_, Message> = if effective_type == FormFieldType::Text {
                row![text("Value:"), value_input].spacing(8).into()
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
                    file_count_text
                ]
                .spacing(8)
                .into()
            };

            // Build the main row — conditionally include type picker
            let mut field_row = row![
                checkbox(field.enabled).on_toggle(move |_| Message::FormFieldToggled(idx)),
                text("Key:"),
                text_input("key", &field.key)
                    .on_input(move |key| Message::FormFieldKeyChanged(idx, key))
                    .width(160),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            // Only show type selector if NOT urlencoded
            if !is_url_encoded {
                field_row = field_row.push(pick_list(
                    &FormFieldType::ALL[..],
                    Some(field.field_type.clone()),
                    move |ft| Message::FormFieldTypeSelected(idx, ft),
                ));
            }

            field_row = field_row.push(value_or_file).push(
                button(text("❌").shaping(text::Shaping::Advanced))
                    .on_press(Message::FormFieldRemove(idx)),
            );

            fields_col = fields_col.push(field_row);

            // Show selected files (only for File type and not urlencoded)
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
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 20.0,
                }));
            }
        }

        fields_col = fields_col.push(
            button(text("➕ Add").shaping(text::Shaping::Advanced)).on_press(Message::FormFieldAdd),
        );

        scrollable(fields_col).height(Length::Fill).into()
    }

    fn render_query_tab(&self) -> Element<'_, Message> {
        let mut params_col = Column::new().spacing(10);

        for (idx, param) in self.query_params.iter().enumerate() {
            let checkbox =
                checkbox(param.enabled).on_toggle(move |_| Message::QueryParamToggled(idx));

            let key_input = text_input("key", &param.key)
                .on_input(move |key| Message::QueryParamKeyChanged(idx, key))
                .width(200);

            let value_input = text_input("value", &param.value)
                .on_input(move |val| Message::QueryParamValueChanged(idx, val))
                .width(300);

            let remove_btn = button(text("❌").shaping(text::Shaping::Advanced))
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
            .align_y(Alignment::Center);

            params_col = params_col.push(param_row);
        }

        params_col = params_col.push(
            button(text("➕ Add").shaping(text::Shaping::Advanced))
                .on_press(Message::QueryParamAdd),
        );

        scrollable(params_col).height(Length::Fill).into()
    }

    fn build_query_string(&self) -> String {
        let params: Vec<String> = self
            .query_params
            .iter()
            .filter(|p| p.enabled && !p.key.is_empty())
            .map(|p| {
                format!(
                    "{}={}",
                    urlencoding::encode(&p.key),
                    urlencoding::encode(&p.value)
                )
            })
            .collect();

        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        }
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
            Space::new().into()
        };

        column![type_selector, token_input].spacing(10).into()
    }

    fn render_response_section(&self) -> Element<'_, Message> {
        let status_view: Element<'_, Message> = if self.loading {
            text("Loading...").into()
        } else if !self.response_status.is_empty() {
            text(&self.response_status).into()
        } else {
            Space::new().into()
        };
        let header_row =
            row![text("Response"), space::horizontal(), status_view,].align_y(Alignment::Center);
        let mut tabs = iced::widget::Row::new()
            .spacing(10)
            .align_y(Alignment::Center);
        tabs = tabs.push(
            button(if self.active_response_tab == ResponseTab::Body {
                "[Body]"
            } else {
                "Body"
            })
            .on_press(Message::ResponseTabSelected(ResponseTab::Body))
            .style(button::text),
        );
        tabs = tabs.push(
            button(if self.active_response_tab == ResponseTab::Headers {
                "[Headers]"
            } else {
                "Headers"
            })
            .on_press(Message::ResponseTabSelected(ResponseTab::Headers))
            .style(button::text),
        );
        if let Some(resp_time) = self.response_time {
            tabs = tabs.push(
                text(format!("⏱️{:.2}ms", resp_time.as_secs_f32() * 1000.0))
                    .shaping(text::Shaping::Advanced),
            );
        }
        if self.is_response_binary {
            tabs = tabs.push(
                text(format!(
                    "🗃️{:.2} KB",
                    self.response_bytes.len() as f32 / 1024.0
                ))
                .shaping(text::Shaping::Advanced),
            );
        }
        tabs = tabs.push(space::horizontal());
        tabs = tabs.push(text("Json Theme: "));
        tabs = tabs.push(pick_list(
            &highlighter::Theme::ALL[..],
            Some(&self.json_theme),
            Message::JsonThemeChanged,
        ));
        tabs = tabs.push(tooltip(
            button(text(if self.copied { "✅" } else { "📋" }).shaping(text::Shaping::Advanced))
                .on_press(Message::CopyToClipboard)
                .style(button::text),
            if self.copied {
                "Copied"
            } else {
                "Copy to Clipboard"
            },
            tooltip::Position::Bottom,
        ));

        let loading_overlay = if self.loading {
            Some(
                container(column![
                    iced::widget::svg(iced::advanced::svg::Handle::from_memory(include_bytes!(
                        "./assets/ring-with-bg.svg"
                    )))
                    .width(80)
                    .height(80)
                    .rotation(iced::Radians::from(
                        self.svg_rotation * std::f32::consts::PI / 180.0,
                    )),
                    text("📤 Sending...").shaping(text::Shaping::Advanced)
                ])
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                .style(|theme: &iced::Theme| {
                    container::Style {
                        background: Some(iced::Background::Color(
                            iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5), // Semi-transparent overlay
                        )),
                        ..Default::default()
                    }
                }),
            )
        } else {
            None
        };

        let content: Element<Message> = match self.active_response_tab {
            ResponseTab::None => Space::new().into(),
            ResponseTab::Body => {
                if self.is_response_binary {
                    let mut body_column = iced::widget::Column::new().spacing(5);
                    body_column = body_column.push(
                        button(text("💾 Save").shaping(text::Shaping::Advanced))
                            .on_press(Message::SaveBinaryResponse)
                            .style(|_, _| button::Style {
                                text_color: iced::Color::from_rgb(1.0, 0.65, 0.0),
                                background: None,
                                ..Default::default()
                            }),
                    );
                    if self.response_content_type.starts_with("image/") {
                        body_column = body_column.push(
                            scrollable(
                                iced::widget::image(iced::advanced::image::Handle::from_bytes(
                                    self.response_bytes.clone(),
                                ))
                                .content_fit(iced::ContentFit::None),
                            )
                            .height(Length::Fill)
                            .width(Length::Fill),
                        );
                    } else if self.response_content_type.starts_with("video/") {
                        // Video playback
                        if let Some(video) = &self.video_player {
                            let vs = self.video_state.as_ref().unwrap();
                            body_column = body_column
                                .push(
                                    container::Container::new(
                                        iced_video_player::VideoPlayer::new(video)
                                            .width(iced::Length::Fill)
                                            .height(iced::Length::Fill)
                                            .content_fit(iced::ContentFit::Contain)
                                            .on_end_of_stream(Message::EndOfStream)
                                            .on_new_frame(Message::NewFrame),
                                    )
                                    .align_x(iced::Alignment::Center)
                                    .align_y(iced::Alignment::Center)
                                    .width(iced::Length::Fill)
                                    .height(iced::Length::Fill),
                                )
                                .push(
                                    container::Container::new(
                                        iced::widget::Slider::new(
                                            0.0..=video.duration().as_secs_f64(),
                                            vs.position,
                                            Message::Seek,
                                        )
                                        .step(0.1)
                                        .on_release(Message::SeekRelease),
                                    )
                                    .padding(iced::Padding::new(5.0).left(10.0).right(10.0)),
                                )
                                .spacing(4)
                                .push(
                                    iced::widget::Row::new()
                                        .spacing(2)
                                        .align_y(iced::alignment::Vertical::Center)
                                        .padding(iced::Padding::new(10.0).top(0.0))
                                        .push(
                                            button::Button::new(
                                                text::Text::new(if video.paused() {
                                                    "▶️"
                                                } else {
                                                    "⏸️"
                                                })
                                                .shaping(text::Shaping::Advanced),
                                            )
                                            .style(button::text)
                                            .on_press(Message::TogglePause),
                                        )
                                        .push(
                                            button::Button::new(
                                                text::Text::new(if video.looping() {
                                                    "🔁❌"
                                                } else {
                                                    "🔁"
                                                })
                                                .shaping(text::Shaping::Advanced),
                                            )
                                            .style(button::text)
                                            .on_press(Message::ToggleLoop),
                                        )
                                        .push(
                                            text::Text::new(format!(
                                                "{}:{:02}s / {}:{:02}s",
                                                vs.position as u64 / 60,
                                                vs.position as u64 % 60,
                                                video.duration().as_secs() / 60,
                                                video.duration().as_secs() % 60,
                                            ))
                                            .width(iced::Length::Fill)
                                            .align_x(iced::alignment::Horizontal::Right),
                                        ),
                                );
                        } else {
                            body_column = body_column.push(
                                text("🎬 Loading video...")
                                    .shaping(text::Shaping::Advanced)
                                    .style(|_| text::Style {
                                        color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                                    }),
                            );
                        }
                    } else {
                        body_column = body_column.push(
                            text(format!(
                                "📄 Binary file received: {}",
                                self.response_filename
                            ))
                            .shaping(text::Shaping::Advanced)
                            .style(|_| text::Style {
                                color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                            }),
                        );
                        body_column = body_column
                            .push(text(format!("Size: {} bytes", self.response_bytes.len())));
                    }
                    body_column.into()
                } else {
                    scrollable(
                        text_editor(&self.response_body_content)
                            .on_action(Message::ResponseBodyAction)
                            .highlight("json", self.json_theme)
                            .height(Length::Shrink),
                    )
                    .height(Length::Fill)
                    .into()
                }
            }
            ResponseTab::Headers => scrollable(
                text_editor(&self.response_headers_content)
                    .on_action(Message::ResponseHeadersAction)
                    .highlight("json", self.json_theme)
                    .height(Length::Shrink),
            )
            .height(Length::Fill)
            .into(),
        };

        let content: Element<'_, Message> = if let Some(overlay) = loading_overlay {
            iced::widget::stack![content, overlay].into()
        } else {
            content.into()
        };

        container(
            column![header_row, tabs, rule::horizontal(1.0), content]
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

    fn update_query<F: FnOnce(&mut QueryParam)>(&mut self, idx: usize, f: F) {
        if let Some(param) = self.query_params.get_mut(idx) {
            f(param);
        }
        self.rebuild_url();
    }

    fn rebuild_url(&mut self) {
        let base = self.base_url.trim_end_matches('?').to_string();
        let mut qp: Vec<String> = Vec::new();

        for p in &self.query_params {
            if p.enabled && !p.key.is_empty() {
                qp.push(format!("{}={}", p.key, p.value));
            }
        }

        if qp.is_empty() {
            self.url = base;
        } else {
            self.url = format!("{}?{}", base, qp.join("&"));
        }
    }

    fn save_request(&self) -> SavedState {
        let is_text = !self.is_response_binary;

        SavedState {
            base_url: self.base_url.clone(),
            url: self.url.clone(),
            method: self.method,
            headers: self.headers_content.text(),
            body: self.body_content.text(),
            auth_type: self.auth_type,
            bearer_token: self.bearer_token.clone(),
            content_type: self.content_type,
            query_params: self.query_params.clone(),
            form_data: self.form_data.clone(),
            json_theme: self.json_theme.to_string(),
            app_theme: self.app_theme.to_string(),

            // Save text-only response
            response_status: is_text.then(|| self.response_status.clone()),
            response_headers: is_text.then(|| self.response_headers_content.text()),
            response_body: is_text.then(|| self.response_body_content.text()),
        }
    }

    fn load_request(&mut self, s: SavedState) {
        self.base_url = s.base_url;
        self.url = s.url;
        self.method = s.method;
        self.headers_content = text_editor::Content::with_text(&s.headers);
        self.body_content = text_editor::Content::with_text(&s.body);
        self.auth_type = s.auth_type;
        self.bearer_token = s.bearer_token;
        self.content_type = s.content_type;
        self.query_params = s.query_params;
        self.form_data = s.form_data;
        // Optionally load response if it's not binary
        if let Some(body) = s.response_body {
            self.response_status = s.response_status.unwrap_or_default();
            self.response_headers_content =
                text_editor::Content::with_text(&s.response_headers.unwrap_or_default());
            self.response_body_content = text_editor::Content::with_text(&body);
            self.is_response_binary = false;
        }
    }

    fn send_request(&mut self) -> iced::Task<Message> {
        let url = self.url.clone();
        let method = self.method.clone();
        let body = self.body_content.text();
        let headers_text = self.headers_content.text();
        let auth_type = self.auth_type.clone();
        let bearer_token = self.bearer_token.clone();
        let content_type = self.content_type.clone();
        let form_data = self.form_data.clone();

        // Reset cancel flag, start timer, and clear previous response time
        self.cancel_flag.store(false, Ordering::Relaxed);
        self.response_time = None;

        let cancel_flag = self.cancel_flag.clone();

        iced::Task::perform(
            async move {
                if cancel_flag.load(Ordering::Relaxed) {
                    return HttpResponse {
                        status: "Cancelled".to_string(),
                        headers: String::new(),
                        body: "Request was cancelled".to_string(),
                        is_binary: false,
                        filename: String::new(),
                        bytes: Vec::new(),
                        content_type: String::new(),
                        response_time: None,
                        accepts_range: false,
                    };
                }
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

                // let client = reqwest::Client::new();
                let client = &HTTP_CLIENT;

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
                            ContentType::XWWWFormUrlEncoded => {
                                let mut params = vec![];
                                for field in &form_data {
                                    if field.enabled
                                        && !field.key.is_empty()
                                        && field.field_type == FormFieldType::Text
                                    {
                                        params.push((field.key.clone(), field.value.clone()));
                                    }
                                }
                                req.form(&params)
                            }
                            ContentType::FormData => {
                                let mut form = reqwest::multipart::Form::new();
                                for field in form_data {
                                    if field.enabled && !field.key.is_empty() {
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

                // Check cancellation before sending
                if cancel_flag.load(Ordering::Relaxed) {
                    return HttpResponse {
                        status: "Cancelled".to_string(),
                        headers: String::new(),
                        body: "Request was cancelled".to_string(),
                        is_binary: false,
                        filename: String::new(),
                        bytes: Vec::new(),
                        content_type: String::new(),
                        response_time: None,
                        accepts_range: false,
                    };
                }

                request = request.headers(header_map.to_owned());

                let start_time = tokio::time::Instant::now();

                match request.send().await {
                    Ok(resp) => {
                        let response_time = start_time.elapsed();

                        // Check cancellation after receiving response
                        if cancel_flag.load(Ordering::Relaxed) {
                            return HttpResponse {
                                status: "Cancelled".to_string(),
                                headers: String::new(),
                                body: "Request was cancelled".to_string(),
                                is_binary: false,
                                filename: String::new(),
                                bytes: Vec::new(),
                                content_type: String::new(),
                                response_time: Some(response_time),
                                accepts_range: false,
                            };
                        }

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

                        let accepts_ranges = hm
                            .get("accept-ranges")
                            .and_then(|h| h.to_str().ok())
                            .is_some();

                        if accepts_ranges {
                            return HttpResponse {
                                status,
                                headers,
                                body: String::new(),
                                bytes: Vec::new(),
                                is_binary,
                                filename,
                                content_type: ct,
                                response_time: Some(response_time),
                                accepts_range: accepts_ranges,
                            };
                        }

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

                            if cancel_flag.load(Ordering::Relaxed) {
                                return HttpResponse {
                                    status: "Cancelled".to_string(),
                                    headers: String::new(),
                                    body: "Request was cancelled".to_string(),
                                    is_binary: false,
                                    filename: String::new(),
                                    bytes: Vec::new(),
                                    content_type: String::new(),
                                    response_time: Some(response_time),
                                    accepts_range: accepts_ranges,
                                };
                            }

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
                            response_time: Some(response_time),
                            accepts_range: accepts_ranges,
                        }
                    }
                    Err(e) => {
                        let response_time = start_time.elapsed();
                        let error_msg = if e.is_timeout() {
                            format!("Request timed out")
                        } else {
                            format!("Request failed: {}", e)
                        };

                        HttpResponse {
                            status: "Error".to_string(),
                            headers: String::new(),
                            body: error_msg,
                            is_binary: false,
                            filename: String::new(),
                            bytes: Vec::new(),
                            content_type: String::new(),
                            response_time: Some(response_time),
                            accepts_range: false,
                        }
                    }
                }
            },
            Message::ResponseReceived,
        )
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([self.svg_rotation_subscription(), self.event_subscription()])
    }

    fn svg_rotation_subscription(&self) -> iced::Subscription<Message> {
        if self.loading {
            iced::time::every(std::time::Duration::from_millis(1)).map(|_| Message::Tick)
        } else {
            iced::Subscription::none()
        }
    }

    fn event_subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen().map(Message::EventOccurred)
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
            app.body_content.perform(action);
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
        Message::CancelRequest => {
            app.cancel_flag.store(true, Ordering::Relaxed);
            app.loading = false;
            app.response_body_content =
                text_editor::Content::with_text("Request cancelled by user");
            app.response_status = "Cancelled".to_string();
            iced::Task::none()
        }
        Message::SaveBinaryResponse => {
            if !app.is_response_binary {
                return iced::Task::none();
            }

            let file_name = app.response_filename.clone();
            let response_bytes = app.response_bytes.clone();

            iced::Task::perform(
                async move {
                    match rfd::AsyncFileDialog::new()
                        .set_file_name(&file_name)
                        .save_file()
                        .await
                    {
                        Some(file) => match file.write(&response_bytes).await {
                            Ok(_) => Message::FileSaved(Ok(file.file_name().to_string())),
                            Err(e) => Message::FileSaved(Err(format!("Failed to save: {}", e))),
                        },
                        None => Message::FileSaved(Err("Save dialog cancelled".to_string())),
                    }
                },
                |message| message, // Pass through the message
            )
        }
        Message::FileSaved(result) => {
            match result {
                Ok(filename) => {
                    app.response_body_content = text_editor::Content::with_text(&format!(
                        "File saved successfully: {}",
                        filename
                    ))
                }
                Err(error) => {
                    app.response_body_content =
                        text_editor::Content::with_text(&format!("Error saving file: {}", error))
                }
            }
            iced::Task::none()
        }
        Message::Tick => {
            app.svg_rotation = (app.svg_rotation + 4.0) % 360.0;
            iced::Task::none()
        }
        Message::TogglePause => {
            if let Some(vp) = app.video_player.as_mut() {
                vp.set_paused(!vp.paused());
            }
            iced::Task::none()
        }

        Message::ToggleLoop => {
            if let Some(vp) = app.video_player.as_mut() {
                vp.set_looping(!vp.looping());
            }
            iced::Task::none()
        }
        Message::VideoVolume(vol) => {
            if let Some(vp) = app.video_player.as_mut() {
                vp.set_volume(vol);
            }
            iced::Task::none()
        }
        Message::Seek(secs) => {
            if let Some(vs) = app.video_state.as_mut() {
                vs.dragging = true;
                vs.position = secs;
            }
            if let Some(vp) = app.video_player.as_mut() {
                vp.set_paused(true);
            }
            iced::Task::none()
        }
        Message::SeekRelease => {
            if let (Some(vs), Some(vp)) = (app.video_state.as_mut(), app.video_player.as_mut()) {
                vs.dragging = false;

                vp.seek(std::time::Duration::from_secs_f64(vs.position), false)
                    .expect("seek");

                vp.set_paused(false);
            }
            iced::Task::none()
        }
        Message::EndOfStream => {
            println!("end of stream");
            iced::Task::none()
        }
        Message::NewFrame => {
            if let (Some(vs), Some(vp)) = (app.video_state.as_mut(), app.video_player.as_mut()) {
                if !vs.dragging {
                    vs.position = vp.position().as_secs_f64();
                }
            }
            iced::Task::none()
        }
        Message::ResponseReceived(resp) => {
            app.loading = false;
            app.active_response_tab = ResponseTab::Body;
            app.is_response_binary = resp.is_binary;

            if resp.content_type.starts_with("video/") && resp.accepts_range {
                let url = url::Url::parse(&app.url).unwrap();

                match iced_video_player::Video::new(&url) {
                    Ok(video) => {
                        app.video_player = Some(video);
                        app.video_state = Some(VideoState {
                            playing: true,
                            buffering: true,
                            position: 0.0,
                            duration: 0.0,
                            volume: 0.8,
                            dragging: false,
                        });
                    }
                    Err(e) => {
                        eprintln!("Failed to load video: {e:?}");
                        app.video_player = None;
                    }
                }
            } else {
                app.video_player = None;
                app.response_headers_content = text_editor::Content::with_text(&resp.headers);
                app.response_body_content = text_editor::Content::with_text(&resp.body);
                app.response_bytes = resp.bytes;
            }

            app.response_status = resp.status;
            app.response_content_type = resp.content_type.clone();
            app.response_time = resp.response_time;

            iced::Task::none()
        }
        Message::ResponseBodyAction(action) => {
            match action {
                text_editor::Action::Edit(edit) => {}
                _ => app.response_body_content.perform(action),
            };
            iced::Task::none()
        }
        Message::ResponseHeadersAction(action) => {
            match action {
                text_editor::Action::Edit(edit) => {}
                _ => app.response_headers_content.perform(action),
            };
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
            if app.is_response_binary {
                return iced::Task::none();
            }
            app.copied = true;
            let text = match app.active_response_tab {
                ResponseTab::Body => app.response_body_content.text(),
                ResponseTab::Headers => app.response_headers_content.text(),
                ResponseTab::None => String::new(),
            };
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
        Message::QueryParamAdd => {
            app.query_params.push(QueryParam::new());
            app.rebuild_url();
            iced::Task::none()
        }

        Message::QueryParamRemove(idx) => {
            if idx < app.query_params.len() {
                app.query_params.remove(idx);
            }
            app.rebuild_url();
            iced::Task::none()
        }

        Message::QueryParamKeyChanged(idx, key) => {
            app.update_query(idx, |p| p.key = key);
            iced::Task::none()
        }

        Message::QueryParamValueChanged(idx, value) => {
            app.update_query(idx, |p| p.value = value);
            iced::Task::none()
        }

        Message::QueryParamToggled(idx) => {
            app.update_query(idx, |p| p.enabled = !p.enabled);
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
        Message::FormFieldToggled(idx) => {
            if let Some(field) = app.form_data.get_mut(idx) {
                field.enabled = !field.enabled;
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
        Message::SaveRequest => {
            let state = app.save_request();

            iced::Task::perform(
                async move {
                    match rfd::AsyncFileDialog::new()
                        .set_title("Save CrabiPie State")
                        .set_file_name("crabipie_state.json")
                        .save_file()
                        .await
                    {
                        Some(file_handle) => {
                            // Serialize JSON
                            let json = serde_json::to_string_pretty(&state)
                                .map_err(|e| format!("Serialization error: {}", e))?;

                            // Async write
                            tokio::fs::write(file_handle.path(), json)
                                .await
                                .map_err(|e| format!("Failed to write file: {}", e))?;

                            Ok::<_, String>(file_handle.file_name().to_string())
                        }
                        None => Err("Save dialog cancelled".to_string()),
                    }
                },
                |result| match result {
                    Ok(filename) => Message::FileSaved(Ok(filename)),
                    Err(err) => Message::FileSaved(Err(err)),
                },
            )
        }
        Message::LoadRequest => {
            iced::Task::perform(
                async move {
                    match rfd::AsyncFileDialog::new()
                        .set_title("Open CrabiPie State")
                        .pick_file()
                        .await
                    {
                        Some(file_handle) => {
                            // Async read file
                            let bytes = tokio::fs::read(file_handle.path())
                                .await
                                .map_err(|e| format!("Failed to read file: {}", e))?;

                            // Convert bytes to string
                            let content = String::from_utf8(bytes)
                                .map_err(|e| format!("Invalid UTF-8 in file: {}", e))?;

                            // Deserialize JSON into SavedState
                            let saved_state: SavedState = serde_json::from_str(&content)
                                .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                            Ok::<_, String>(saved_state)
                        }
                        None => Err("Open file dialog cancelled".to_string()),
                    }
                },
                |result| match result {
                    Ok(saved_state) => Message::RequestLoaded(saved_state),
                    Err(err) => Message::RequestLoadFailed(err),
                },
            )
        }
        Message::RequestLoaded(saved_state) => {
            app.load_request(saved_state);
            iced::Task::none()
        }
        Message::RequestLoadFailed(err) => iced::Task::none(),
        Message::FormFieldFileSelect(idx) => {
            let future = async move {
                let files = rfd::AsyncFileDialog::new()
                    .set_directory("~/Downloads")
                    .pick_files()
                    .await;

                // Extract file paths as Strings
                let paths = files
                    .map(|handles| {
                        handles
                            .into_iter()
                            .filter_map(|handle| handle.path().to_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Message::FormFieldFilesSelected(idx, paths)
            };

            iced::Task::perform(future, std::convert::identity)
        }
        Message::FormFieldFilesSelected(index, files) => {
            if let Some(field) = app.form_data.get_mut(index) {
                field.files = files;
            }
            iced::Task::none()
        }
        Message::FormFieldRemove(index) => {
            if index < app.form_data.len() {
                app.form_data.remove(index);
            }
            iced::Task::none()
        }
        Message::FormFieldAdd => {
            app.form_data.push(FormField::new());
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
        Message::EventOccurred(event) => {
            use iced::keyboard::{Event as KeyEvent, Key, Modifiers};
            if let Event::Keyboard(key_event) = event {
                match key_event {
                    KeyEvent::KeyPressed { key, modifiers, .. } if modifiers.control() => {
                        if let Key::Character(c) = &key {
                            if c.as_str() == "l" {
                                println!("Key event: ctrl + l");
                                return iced::widget::operation::focus(app.url_id.clone()).chain(
                                    iced::widget::operation::select_all(app.url_id.clone()),
                                );
                            }
                        }
                        if matches!(key, Key::Named(iced::keyboard::key::Named::Enter)) {
                            println!("Key event: ctrl + Enter");
                            return iced::Task::done(Message::SendRequest);
                        }
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Enter),
                        ..
                    } => {
                        return iced::widget::operation::is_focused(app.url_id.clone()).then(
                            |focused| {
                                if focused {
                                    iced::Task::done(Message::SendRequest)
                                } else {
                                    iced::Task::none()
                                }
                            },
                        );
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Tab),
                        modifiers,
                        ..
                    } if modifiers.shift() => {
                        println!("Key event: shift + tab");
                        return iced::widget::operation::focus_previous();
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Tab),
                        ..
                    } => {
                        println!("Key event: Tab");
                        return iced::widget::operation::focus_next();
                    }
                    _ => {}
                }
            }
            iced::Task::none()
        }
    }
}

fn view(app: &CrabiPie) -> Element<'_, Message> {
    let title_row = row![
        text("CrabiPie HTTP Client").size(16),
        space::horizontal(),
        text("App theme"),
        pick_list(
            &iced::Theme::ALL[..],
            Some(&app.app_theme),
            Message::AppThemeChanged,
        ),
        button("Open").on_press(Message::LoadRequest),
        button("Save").on_press(Message::SaveRequest)
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
        .id(app.url_id.clone())
        .on_input(Message::UrlChanged)
        .size(20)
        .padding(8)
        .width(Length::Fill);

    let send_button = if app.loading {
        button(
            text("⏹ Cancel")
                .align_x(alignment::Horizontal::Center)
                .shaping(text::Shaping::Advanced)
                .width(Length::Fill),
        )
        .on_press(Message::CancelRequest)
        .padding(10)
        .width(100)
    } else {
        button(
            text("📤 Send")
                .shaping(text::Shaping::Advanced)
                .align_x(alignment::Horizontal::Center)
                .width(Length::Fill),
        )
        .on_press_maybe(if !app.url.trim().is_empty() {
            Some(Message::SendRequest)
        } else {
            None
        })
        .padding(10)
        .width(100)
    };

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
    iced::application(CrabiPie::new, update, view)
        .theme(|app: &CrabiPie| app.app_theme.clone())
        .subscription(|app| app.subscription())
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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
enum RequestTab {
    Body,
    Headers,
    Auth,
    Query,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
enum ContentType {
    Json,
    FormData,
    XWWWFormUrlEncoded,
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Json => write!(f, "JSON"),
            ContentType::FormData => write!(f, "Form Data"),
            ContentType::XWWWFormUrlEncoded => write!(f, "x-www-form"),
        }
    }
}

impl ContentType {
    const ALL: [ContentType; 3] = [
        ContentType::Json,
        ContentType::FormData,
        ContentType::XWWWFormUrlEncoded,
    ];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FormField {
    enabled: bool,
    key: String,
    value: String,
    files: Vec<String>,
    field_type: FormFieldType,
}

impl FormField {
    fn new() -> Self {
        Self {
            enabled: true,
            key: String::new(),
            value: String::new(),
            files: Vec::new(),
            field_type: FormFieldType::Text,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq)]
enum ResponseTab {
    None,
    Body,
    Headers,
}

#[derive(Debug, Clone)]
struct HttpResponse {
    status: String,
    headers: String,
    accepts_range: bool,
    body: String,
    is_binary: bool,
    filename: String,
    bytes: Vec<u8>,
    content_type: String,
    response_time: Option<tokio::time::Duration>,
}

#[derive(Debug, Clone)]
pub struct VideoState {
    playing: bool,
    position: f64,
    duration: f64,
    dragging: bool,
    volume: f64,
    buffering: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParam {
    key: String,
    value: String,
    enabled: bool,
}

impl QueryParam {
    fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }
}

static HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
    reqwest::Client::builder()
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("failed to build http client")
});

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedState {
    base_url: String,
    url: String,
    method: HttpMethod,
    headers: String,
    body: String,
    auth_type: AuthType,
    bearer_token: String,
    content_type: ContentType,
    query_params: Vec<QueryParam>,
    form_data: Vec<FormField>,
    json_theme: String,
    app_theme: String,

    // Response (only when NOT binary)
    response_status: Option<String>,
    response_headers: Option<String>,
    response_body: Option<String>,
}

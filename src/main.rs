#![allow(unused)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures::channel::mpsc;
use iced_aw::iced_aw_font::down_open;
use reqwest_websocket::RequestBuilderExt;
use std::sync::atomic::Ordering;

use iced::{
    Alignment, Border, Element, Event, Length, Padding, alignment,
    widget::{
        Column, Space, button, checkbox, column, container, pick_list, row, rule, scrollable,
        space, text, text_editor, text_input, tooltip,
    },
};
use serde::{Deserialize, Serialize};

mod json_highlighter;

//TODOS:
//1 base url
//2 collections

#[derive(Debug, Clone)]
enum Message {
    NoOp,
    //Tabs
    AddNewTab,
    TabSelected(usize),
    CloseTab(usize),

    UrlChanged(String),
    RequestTypeSelected(RequestType),
    MethodSelected(HttpMethod),

    //headers actions
    HeaderAdd,
    HeaderRemove(usize),
    HeaderKeyChanged(usize, String),
    HeaderValueChanged(usize, String),
    HeaderToggled(usize),

    BodyAction(text_editor::Action),
    AuthTypeSelected(AuthType),
    ApiKeyNameChanged(String),
    ApiKeyChanged(String),
    ApiKeyPositionChanged(ApiKeyPosition),
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
    JsonPrettified(Result<String, String>),
    CopyToClipboard,
    ResetCopied,
    JsonThemeChanged(json_highlighter::JsonThemeWrapper),
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

    // WebSocket messages
    WsConnect,
    WsDisconnect,
    WsEvent(WsEvent),
    WsMessageInputChanged(String),
    WsSendMessage,
    WsClearMessages,
    WsToggleAutoScroll,

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
    // Tab managemen
    tabs: Vec<TabState>,
    active_tab: usize,
    next_tab_id: usize,

    // Global UI state (shared across all tabs)
    json_theme: json_highlighter::JsonThemeWrapper,
    app_theme: iced::Theme,
    svg_rotation: f32,

    // Find dialog (global)
    find_dialog_open: bool,
    find_replace_mode: bool,
    find_text: String,
    replace_text: String,
    case_sensitive: bool,
    whole_word: bool,
    current_match: usize,
    current_match_pos: Option<usize>,
    total_matches: usize,

    // For highlighter
    search_match_positions: Vec<(usize, usize)>,
    current_match_line_col: Option<(usize, usize)>,
    search_match_length: usize,
}

struct TabState {
    id: usize,
    title: String,

    // Request configuration
    url_id: iced::widget::Id,
    base_url: String,
    url: String,
    request_type: RequestType,
    method: HttpMethod,
    headers: Vec<RequestHeaders>,
    body_content: text_editor::Content,
    auth_type: AuthType,
    bearer_token: String,
    api_key_name: String,
    api_key: String,
    api_key_position: ApiKeyPosition,
    content_type: ContentType,
    query_params: Vec<QueryParam>,
    form_data: Vec<FormField>,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    // WebSocket-specific fields
    ws_connected: bool,
    ws_connection: Option<WsConnection>,
    ws_messages: Vec<WsMessageDisplay>,
    ws_auto_scroll: bool,
    ws_input: String,

    //image handle
    image_handle: Option<iced::widget::image::Handle>,

    // Video response
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

    // Tab-specific UI state
    loading: bool,
    active_request_tab: RequestTab,
    active_response_tab: ResponseTab,
    copied: bool,
    ws_connection_id: usize,
}

impl TabState {
    fn new(id: usize) -> Self {
        let base = "https://jsonplaceholder.typicode.com/posts".to_string();
        Self {
            id,
            title: format!("Request {}", id + 1),
            url_id: iced::widget::Id::unique(),
            base_url: base.clone(),
            url: base,
            request_type: RequestType::HTTP,
            method: HttpMethod::GET,
            headers: RequestHeaders::default(),
            body_content: text_editor::Content::with_text(BODY_DEFAULT),
            auth_type: AuthType::None,
            bearer_token: String::new(),
            api_key_name: String::new(),
            api_key: String::new(),
            api_key_position: ApiKeyPosition::Header,
            content_type: ContentType::Json,
            query_params: vec![QueryParam::new()],
            form_data: vec![FormField::new()],
            image_handle: None,
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
            active_request_tab: RequestTab::Query,
            active_response_tab: ResponseTab::Body,
            copied: false,
            ws_connected: false,
            ws_messages: Vec::new(),
            ws_input: String::new(),
            ws_auto_scroll: true,
            ws_connection: None,
            ws_connection_id: 0,
        }
    }

    fn from_saved(saved: SavedState) -> Self {
        Self {
            id: saved.id,
            title: saved.title,
            url_id: iced::widget::Id::unique(),
            base_url: saved.base_url,
            url: saved.url,
            request_type: saved.request_type,
            method: saved.method,
            headers: saved.headers,
            body_content: text_editor::Content::with_text(&saved.body),
            auth_type: saved.auth_type,
            bearer_token: saved.bearer_token,
            api_key_name: saved.api_key_name,
            api_key: saved.api_key,
            api_key_position: saved.api_key_position,
            content_type: saved.content_type,
            query_params: saved.query_params,
            form_data: saved.form_data,
            image_handle: None,
            video_player: None,
            video_state: None,
            cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            response_status: saved.response_status.unwrap_or_default(),
            response_headers_content: text_editor::Content::with_text(
                &saved.response_headers.unwrap_or_default(),
            ),
            response_body_content: text_editor::Content::with_text(
                &saved.response_body.unwrap_or_default(),
            ),
            is_response_binary: false,
            response_filename: String::new(),
            response_bytes: Vec::new(),
            response_content_type: String::new(),
            response_time: None,
            loading: false,
            active_request_tab: RequestTab::Query,
            active_response_tab: ResponseTab::Body,
            copied: false,
            ws_connected: false,
            ws_messages: Vec::new(),
            ws_input: String::new(),
            ws_auto_scroll: true,
            ws_connection: None,
            ws_connection_id: 0,
        }
    }

    fn to_saved(&self, json_theme: &str, app_theme: &str) -> SavedState {
        SavedState {
            id: self.id,
            title: self.title.clone(),
            base_url: self.base_url.clone(),
            url: self.url.clone(),
            request_type: self.request_type.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
            body: self.body_content.text(),
            auth_type: self.auth_type.clone(),
            bearer_token: self.bearer_token.clone(),
            api_key_name: self.api_key_name.clone(),
            api_key: self.api_key.clone(),
            api_key_position: self.api_key_position,
            content_type: self.content_type.clone(),
            query_params: self.query_params.clone(),
            form_data: self.form_data.clone(),
            json_theme: json_theme.to_string(),
            app_theme: app_theme.to_string(),
            response_status: if self.response_status.is_empty() {
                None
            } else {
                Some(self.response_status.clone())
            },
            response_headers: if self.response_headers_content.text().is_empty() {
                None
            } else {
                Some(self.response_headers_content.text())
            },
            response_body: if self.response_body_content.text().is_empty() {
                None
            } else {
                Some(self.response_body_content.text())
            },
        }
    }
}

impl CrabiPie {
    fn new() -> (Self, iced::Task<Message>) {
        (
            Self {
                tabs: vec![TabState::new(0)],
                active_tab: 0,
                next_tab_id: 1,
                json_theme: json_highlighter::JsonThemeWrapper::Custom(
                    json_highlighter::CustomJsonTheme::VSCODE_DARK,
                ),
                app_theme: iced::Theme::CatppuccinMocha,
                svg_rotation: 0.0,
                find_dialog_open: false,
                find_replace_mode: false,
                find_text: String::new(),
                replace_text: String::new(),
                case_sensitive: false,
                whole_word: false,
                current_match: 0,
                current_match_pos: None,
                total_matches: 0,
                search_match_positions: Vec::new(),
                current_match_line_col: None,
                search_match_length: 0,
            },
            iced::Task::none(),
        )
    }

    fn title(&self) -> String {
        "CrabIce".to_string()
    }

    fn current_tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn current_tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    fn add_tab(&mut self) {
        let new_tab = TabState::new(self.next_tab_id);
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
        self.next_tab_id += 1;
    }

    fn close_tab(&mut self, index: usize) {
        if self.tabs.len() > 1 {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            } else if self.active_tab > index {
                self.active_tab -= 1;
            }
        }
    }

    fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    fn get_highlighter_settings(&self) -> json_highlighter::JsonHighlighterSettings {
        json_highlighter::JsonHighlighterSettings::new(self.json_theme).with_search(
            self.search_match_positions.clone(),
            self.current_match_line_col,
            self.search_match_length,
        )
    }

    fn view_find_replace(&self) -> Element<'_, Message> {
        let toggle: Element<'_, Message> = tooltip(
            button(
                text(if !self.find_replace_mode {
                    "⬇️"
                } else {
                    "⬆️"
                })
                .shaping(text::Shaping::Advanced),
            )
            .style(button::text)
            .on_press(Message::ToggleFindReplaceDialog),
            "Toggle between find and replace",
            tooltip::Position::Bottom,
        )
        .into();

        let find_input: Element<'_, Message> = text_input("Find...", &self.find_text)
            .id("find_input")
            .on_input(Message::FindTextChanged)
            .into();

        let find_btns_row = row![
            tooltip(
                button(text("🔍").shaping(text::Shaping::Advanced))
                    .style(button::text)
                    .on_press(Message::FindNext),
                "Find Next",
                tooltip::Position::Bottom
            ),
            tooltip(
                button(text("✖").shaping(text::Shaping::Advanced))
                    .style(button::text)
                    .on_press(Message::CloseFindDialog),
                "Close",
                tooltip::Position::Bottom
            )
        ];

        let replace_input_or_space: Element<'_, Message> = if self.find_replace_mode {
            text_input("Replace with...", &self.replace_text)
                .id("replace_input")
                .on_input(Message::ReplaceTextChanged)
                .into()
        } else {
            space::horizontal().width(0).into()
        };

        let replace_btns_or_space: Element<'_, Message> = if self.find_replace_mode {
            row![
                tooltip(
                    button(text("✏️").shaping(text::Shaping::Advanced))
                        .style(button::text)
                        .on_press(Message::FindNext),
                    "Replace Next",
                    tooltip::Position::Bottom
                ),
                tooltip(
                    button(text("🔁").shaping(text::Shaping::Advanced))
                        .style(button::text)
                        .on_press(Message::ReplaceAll),
                    "Replace All",
                    tooltip::Position::Bottom
                ),
            ]
            .align_y(iced::Alignment::End)
            .into()
        } else {
            space::horizontal().width(0).into()
        };

        let match_info: Element<'_, Message> = if !self.find_text.is_empty() {
            text(format!(
                "{} / {} matches",
                self.current_match, self.total_matches,
            ))
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            space::horizontal().width(0).into()
        };

        let find_mode_buttons = row![
            tooltip(
                button("Aa")
                    .style(if !self.case_sensitive {
                        button::text
                    } else {
                        button::subtle
                    })
                    .on_press(Message::ToggleCaseSensitive),
                "Match case",
                tooltip::Position::Bottom
            ),
            tooltip(
                button("[ab]")
                    .style(if !self.whole_word {
                        button::text
                    } else {
                        button::subtle
                    })
                    .on_press(Message::ToggleWholeWord),
                "Match whole word",
                tooltip::Position::Bottom
            ),
            match_info
        ];

        let find_replace_col = column![find_input, replace_input_or_space].spacing(5.0);

        let content = row![
            toggle,
            column![find_replace_col, find_mode_buttons].spacing(5.0),
            column![find_btns_row, replace_btns_or_space]
        ];

        container(content)
            .width(Length::Fixed(400.0))
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.palette().primary,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .padding(5.0)
            .into()
    }

    fn position_to_line_col(text: &str, byte_pos: usize) -> (usize, usize) {
        let mut line_idx = 0;
        let mut line_start_byte = 0;

        for line in text.split_inclusive('\n') {
            let line_end_byte = line_start_byte + line.len();

            if byte_pos >= line_start_byte && byte_pos < line_end_byte {
                let byte_offset_in_line = byte_pos - line_start_byte;

                // Convert byte offset → char offset
                let char_offset = line[..byte_offset_in_line].chars().count();

                return (line_idx, char_offset);
            }

            line_start_byte = line_end_byte;
            line_idx += 1;
        }

        (0, 0)
    }

    fn find_matches(&self, text: &str, pattern: &str) -> Vec<usize> {
        if pattern.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();

        if self.case_sensitive {
            if self.whole_word {
                // Case sensitive + whole word
                let mut start = 0;
                while let Some(pos) = text[start..].find(pattern) {
                    let abs_pos = start + pos;
                    let before =
                        abs_pos == 0 || !text[..abs_pos].chars().last().unwrap().is_alphanumeric();
                    let after_pos = abs_pos + pattern.len();
                    let after = after_pos >= text.len()
                        || !text[after_pos..].chars().next().unwrap().is_alphanumeric();

                    if before && after {
                        matches.push(abs_pos);
                    }
                    start = abs_pos + 1;
                }
            } else {
                // Case sensitive only
                let mut start = 0;
                while let Some(pos) = text[start..].find(pattern) {
                    matches.push(start + pos);
                    start += pos + 1;
                }
            }
        } else {
            let text_lower = text.to_lowercase();
            let pattern_lower = pattern.to_lowercase();

            if self.whole_word {
                // Case insensitive + whole word
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&pattern_lower) {
                    let abs_pos = start + pos;
                    let before =
                        abs_pos == 0 || !text[..abs_pos].chars().last().unwrap().is_alphanumeric();
                    let after_pos = abs_pos + pattern.len();
                    let after = after_pos >= text.len()
                        || !text[after_pos..].chars().next().unwrap().is_alphanumeric();

                    if before && after {
                        matches.push(abs_pos);
                    }
                    start = abs_pos + 1;
                }
            } else {
                // Case insensitive only
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&pattern_lower) {
                    matches.push(start + pos);
                    start += pos + 1;
                }
            }
        }

        matches
    }

    fn find_next(&mut self) {
        if !self.find_dialog_open {
            return;
        }

        let text = self.current_tab().response_body_content.text();
        println!("=== FIND NEXT ===");
        println!("Text length: {}", text.len());
        println!("Number of lines: {}", text.lines().count());
        println!(
            "First 200 chars: {:?}",
            &text.chars().take(200).collect::<String>()
        );

        let matches = self.find_matches(&text, &self.find_text);

        println!("Search text: '{}'", self.find_text);
        println!("Found {} matches", matches.len());
        println!("Match positions: {:?}", matches);

        self.total_matches = matches.len();

        if matches.is_empty() {
            self.current_match = 0;
            self.current_match_pos = None;
            self.search_match_positions = Vec::new();
            self.current_match_line_col = None;
            self.search_match_length = 0;
            return;
        }

        // Convert all matches to line/col positions
        let match_positions: Vec<(usize, usize)> = matches
            .iter()
            .map(|&pos| Self::position_to_line_col(&text, pos))
            .collect();

        // println!("Line/col positions: {:?}", match_positions);

        // Update search state
        self.search_match_positions = match_positions;
        self.search_match_length = self.find_text.chars().count();

        println!("Match length (chars): {}", self.search_match_length);

        // Move to next match
        if self.current_match == 0 || self.current_match >= matches.len() {
            self.current_match = 1;
        } else {
            self.current_match += 1;
        }

        if self.current_match > matches.len() {
            self.current_match = 1;
        }

        if self.current_match > 0 {
            let match_pos = matches[self.current_match - 1];
            self.current_match_pos = Some(match_pos);
            self.current_match_line_col = Some(self.search_match_positions[self.current_match - 1]);
        }
    }

    fn find_previous(&mut self) {
        if !self.find_dialog_open {
            return;
        }

        let text = self.current_tab().response_body_content.text();
        let matches = self.find_matches(&text, &self.find_text);

        self.total_matches = matches.len();

        if matches.is_empty() {
            self.current_match = 0;
            self.current_match_pos = None;
            self.search_match_positions = Vec::new();
            self.current_match_line_col = None;
            self.search_match_length = 0;
            return;
        }

        // Convert all matches to line/col positions
        let match_positions: Vec<(usize, usize)> = matches
            .iter()
            .map(|&pos| Self::position_to_line_col(&text, pos))
            .collect();

        // Update search state
        self.search_match_positions = match_positions;
        self.search_match_length = self.find_text.chars().count();

        // Move to previous match
        if self.current_match <= 1 {
            self.current_match = matches.len();
        } else {
            self.current_match -= 1;
        }

        if self.current_match > 0 {
            let match_pos = matches[self.current_match - 1];
            self.current_match_pos = Some(match_pos);
            self.current_match_line_col = Some(self.search_match_positions[self.current_match - 1]);
        }
    }

    fn render_websocket_panel(&self) -> Element<'_, Message> {
        let tab = self.current_tab();
        let is_connected = tab.ws_connected;

        // Connection controls
        let url_input = text_input("wss://echo.websocket.org", &tab.url)
            .on_input(Message::UrlChanged)
            .padding(10)
            .width(Length::Fill);

        let connect_button = if is_connected {
            button(text("Disconnect").size(14))
                .on_press(Message::WsDisconnect)
                .padding(10)
                .style(button::danger)
        } else if tab.loading {
            button(text("Connecting...").size(14)).padding(10)
        } else {
            button(text("Connect").size(14))
                .on_press(Message::WsConnect)
                .padding(10)
                .style(button::primary)
        };

        let clear_button = button(text("Clear").size(14))
            .on_press(Message::WsClearMessages)
            .padding(10);

        let status_text = if is_connected {
            text("🟢 Connected").color(iced::Color::from_rgb(0.0, 0.8, 0.0))
        } else {
            text("🔴 Disconnected").color(iced::Color::from_rgb(0.8, 0.0, 0.0))
        };

        let connection_row = row![url_input, connect_button, clear_button, status_text,]
            .spacing(10)
            .padding(Padding::new(0.0).top(10.0))
            .align_y(Alignment::Center);

        // Message display area
        let messages_list =
            tab.ws_messages
                .iter()
                .fold(column![].spacing(8).padding(10), |col, msg| {
                    let (prefix, color) = match msg.direction {
                        MessageDirection::Sent => ("→", iced::Color::from_rgb(0.2, 0.6, 1.0)),
                        MessageDirection::Received => ("←", iced::Color::from_rgb(0.0, 0.8, 0.0)),
                        MessageDirection::System => ("•", iced::Color::from_rgb(0.6, 0.6, 0.6)),
                    };

                    let message_row = row![
                        text(&msg.timestamp)
                            .size(11)
                            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                        text(prefix).size(14).color(color),
                        text_input("", &msg.content)
                            .size(13)
                            .style(|t: &iced::Theme, s| text_input::Style {
                                border: Border {
                                    width: 0.0,
                                    ..Default::default()
                                },
                                background: iced::Background::Color(t.palette().background),
                                ..text_input::default(t, s)
                            }),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center);

                    col.push(message_row)
                });

        let messages_scroll = scrollable(messages_list)
            .id("messages")
            .height(Length::Fill)
            .width(Length::Fill)
            .anchor_bottom();

        // Message input area
        let message_input = text_input("Type message...", &tab.ws_input)
            .on_input(Message::WsMessageInputChanged)
            .on_submit(Message::WsSendMessage)
            .padding(8)
            .width(Length::Fill);

        let send_button = button(text("Send").size(14))
            .on_press_maybe(if is_connected && !tab.ws_input.is_empty() {
                Some(Message::WsSendMessage)
            } else {
                None
            })
            .padding(10)
            .style(button::primary);

        let input_row = row![message_input, send_button].spacing(10);

        // Stats
        let stats = text(format!(
            "Messages: {} | Sent: {} | Received: {}",
            tab.ws_messages.len(),
            tab.ws_messages
                .iter()
                .filter(|m| m.direction == MessageDirection::Sent)
                .count(),
            tab.ws_messages
                .iter()
                .filter(|m| m.direction == MessageDirection::Received)
                .count(),
        ))
        .size(11)
        .color(iced::Color::from_rgb(0.5, 0.5, 0.5));

        let stats_row = container(stats).padding(5).center_x(Length::Fill);

        // Combine everything
        column![
            connection_row,
            container(messages_scroll)
                .height(Length::Fill)
                .padding(10)
                .style(|th| container::Style {
                    background: Some(iced::Background::Color(th.palette().background)),
                    border: Border {
                        width: 1.0,
                        color: th.extended_palette().background.weak.color,
                        radius: 5.0.into()
                    },
                    ..Default::default()
                }),
            stats_row,
            input_row,
        ]
        .spacing(10)
        .into()
    }

    // Helper function to connect
    async fn connect_ws(
        url: &str,
    ) -> Result<reqwest_websocket::WebSocket, Box<dyn std::error::Error + Send + Sync>> {
        let response = reqwest::Client::new().get(url).upgrade().send().await?;
        let websocket = response.into_websocket().await?;
        Ok(websocket)
    }

    fn add_ws_system_message(&mut self, content: &str) {
        let msg = WsMessageDisplay {
            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            direction: MessageDirection::System,
            content: content.to_string(),
        };
        self.current_tab_mut().ws_messages.push(msg);
    }

    fn add_ws_sent_message(&mut self, content: &str) {
        let msg = WsMessageDisplay {
            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            direction: MessageDirection::Sent,
            content: content.to_string(),
        };
        self.current_tab_mut().ws_messages.push(msg);
    }

    fn add_ws_received_message(&mut self, content: &str) {
        let msg = WsMessageDisplay {
            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            direction: MessageDirection::Received,
            content: content.to_string(),
        };
        self.current_tab_mut().ws_messages.push(msg);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedState {
    id: usize,
    title: String,
    base_url: String,
    url: String,
    request_type: RequestType,
    method: HttpMethod,
    headers: Vec<RequestHeaders>,
    body: String,
    auth_type: AuthType,
    bearer_token: String,
    api_key_name: String,
    api_key: String,
    api_key_position: ApiKeyPosition,
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

impl Default for SavedState {
    fn default() -> Self {
        let base = "https://jsonplaceholder.typicode.com/posts".to_string();
        Self {
            id: 0,
            title: "Request-1".into(),
            url: base.clone(),
            base_url: base,
            request_type: RequestType::HTTP,
            method: HttpMethod::GET,
            headers: vec![RequestHeaders::new()],
            body: String::new(),
            auth_type: AuthType::None,
            api_key_position: ApiKeyPosition::Header,
            bearer_token: String::new(),
            api_key_name: String::new(),
            api_key: String::new(),
            content_type: ContentType::Json,
            query_params: vec![QueryParam::new()],
            form_data: vec![FormField::new()],
            json_theme: String::new(),
            app_theme: String::new(),
            response_status: None,
            response_headers: None,
            response_body: None,
        }
    }
}

impl CrabiPie {
    fn render_tabs(&self) -> Element<'_, Message> {
        let mut tab_bar = iced::widget::Row::new().spacing(2);

        for (index, tab) in self.tabs.iter().enumerate() {
            let is_active = index == self.active_tab;

            let button_or_space: Element<'_, Message> = if self.tabs.len() > 1 {
                button(text("❌").size(8).shaping(text::Shaping::Advanced))
                    .style(button::text)
                    .on_press(Message::CloseTab(index))
                    .into()
            } else {
                space::horizontal().width(Length::Shrink).into()
            };
            let tab_button = button(
                row![text(&tab.title), button_or_space]
                    .spacing(5)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::TabSelected(index))
            .style(if is_active {
                button::primary
            } else {
                button::secondary
            });

            tab_bar = tab_bar.push(tab_button);
        }

        // Add the "+" button
        let add_button = button(text("+").size(20))
            .on_press(Message::AddNewTab)
            .style(button::text);

        tab_bar = tab_bar.push(add_button);

        container(tab_bar)
            .style(|theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(theme.palette().background)),
                ..Default::default()
            })
            .into()
    }

    fn render_active_tab_content(&self) -> Element<'_, Message> {
        let tab = &self.tabs[self.active_tab];

        // Render the content using the active tab's data
        column![
            self.render_request_row(),
            row![
                self.render_request_section(),
                self.render_response_section()
            ]
            .spacing(10)
        ]
        .spacing(10)
        .into()
    }
    fn render_title_row(&self) -> Element<'_, Message> {
        row![
            text("CrabiPie HTTP Client").size(16),
            space::horizontal(),
            text("App theme"),
            pick_list(
                &iced::Theme::ALL[..],
                Some(&self.app_theme),
                Message::AppThemeChanged,
            ),
            button("Open").on_press(Message::LoadRequest),
            button("Save").on_press(Message::SaveRequest)
        ]
        .spacing(10)
        .into()
    }

    fn render_request_row(&self) -> Element<'_, Message> {
        let req_type = pick_list(
            &RequestType::ALL[..],
            Some(self.current_tab().request_type.clone()),
            Message::RequestTypeSelected,
        )
        .width(110)
        .padding(10);

        let method_picker = pick_list(
            &HttpMethod::ALL[..],
            Some(self.current_tab().method.clone()),
            Message::MethodSelected,
        )
        .width(100)
        .padding(10);

        let url_input = text_input("https://api.example.com/endpoint", &self.current_tab().url)
            .id(self.current_tab().url_id.clone())
            .on_input(Message::UrlChanged)
            .size(20)
            .padding(8)
            .width(Length::Fill);

        let send_button = if self.current_tab().loading {
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
            .on_press_maybe(if !self.current_tab().url.trim().is_empty() {
                Some(Message::SendRequest)
            } else {
                None
            })
            .padding(10)
            .width(100)
        };

        container(row![req_type, method_picker, url_input, send_button].spacing(10))
            // .style(|theme: &iced::Theme| container::Style {
            //     border: Border {
            //         width: 0.5,
            //         color: theme.palette().primary,
            //         radius: 6.0.into(),
            //     },
            //     ..Default::default()
            // })
            .padding(Padding::new(0.0).top(10.0))
            .into()
    }

    fn render_request_section(&self) -> Element<'_, Message> {
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
                    container(self.render_body_tab()).padding(Padding {
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
                .set_active_tab(&self.current_tab().active_request_tab)
                .tab_bar_position(iced_aw::TabBarPosition::Top)
                .into();

        container(column![text("Request").height(20), rule::horizontal(1.0), req_tabs,].spacing(10))
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

    fn render_body_tab(&self) -> Element<'_, Message> {
        if !matches!(
            self.current_tab().method,
            HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH
        ) {
            return text("Select POST, PUT, or PATCH to edit body.").into();
        }

        let prettify_button: Element<'_, Message> =
            if self.current_tab().content_type == ContentType::Json {
                button(text("✨ Prettify").shaping(text::Shaping::Advanced))
                    .on_press(Message::PrettifyJson)
                    .style(button::text)
                    .into()
            } else {
                Space::new().into()
            };

        let type_selector = row![
            text("Type:"),
            pick_list(
                &ContentType::ALL[..],
                Some(self.current_tab().content_type.clone()),
                Message::ContentTypeSelected
            ),
            space::horizontal(),
            prettify_button,
        ]
        .height(20)
        .spacing(10)
        .align_y(Alignment::Center);

        let editor_content = match self.current_tab().content_type {
            ContentType::Json => scrollable(
                text_editor(&self.current_tab().body_content)
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
                    .style(|theme, status| Self::get_editor_style(theme, status)),
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
        let is_url_encoded = matches!(
            self.current_tab().content_type,
            ContentType::XWWWFormUrlEncoded
        );

        let mut fields_col = Column::new().spacing(10);

        for (idx, field) in self.current_tab().form_data.iter().enumerate() {
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
                    .style(button::subtle)
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

        scrollable(fields_col).height(Length::Fill).into()
    }

    fn render_query_tab(&self) -> Element<'_, Message> {
        let mut params_col = Column::new().spacing(10);

        for (idx, param) in self.current_tab().query_params.iter().enumerate() {
            let checkbox =
                checkbox(param.enabled).on_toggle(move |_| Message::QueryParamToggled(idx));

            let key_input = text_input("key", &param.key)
                .on_input(move |key| Message::QueryParamKeyChanged(idx, key))
                .width(200);

            let value_input = text_input("value", &param.value)
                .on_input(move |val| Message::QueryParamValueChanged(idx, val))
                .width(300);

            let remove_btn = button(text("❌").shaping(text::Shaping::Advanced))
                .style(button::subtle)
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
                .style(button::subtle)
                .on_press(Message::QueryParamAdd),
        );

        scrollable(params_col).height(Length::Fill).into()
    }

    fn build_query_string(&self) -> String {
        let params: Vec<String> = self
            .current_tab()
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
        let mut headers_col = Column::new().spacing(10);

        for (idx, header) in self.current_tab().headers.iter().enumerate() {
            let checkbox = checkbox(header.enabled).on_toggle(move |_| Message::HeaderToggled(idx));

            let key_input = text_input("key", &header.key)
                .on_input(move |key| Message::HeaderKeyChanged(idx, key))
                .width(200);

            let value_input = text_input("value", &header.value)
                .on_input(move |val| Message::HeaderValueChanged(idx, val))
                .width(300);

            let remove_btn = button(text("❌").shaping(text::Shaping::Advanced))
                .style(button::subtle)
                .on_press(Message::HeaderRemove(idx));

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

            headers_col = headers_col.push(param_row);
        }

        headers_col = headers_col.push(
            button(text("➕ Add").shaping(text::Shaping::Advanced))
                .style(button::subtle)
                .on_press(Message::HeaderAdd),
        );

        scrollable(headers_col).height(Length::Fill).into()
    }

    fn render_auth_tab(&self) -> Element<'_, Message> {
        let type_selector = row![
            text("Type:"),
            pick_list(
                &AuthType::ALL[..],
                Some(self.current_tab().auth_type.clone()),
                Message::AuthTypeSelected
            )
            .width(150),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let auth_form: Element<'_, Message> = match self.current_tab().auth_type {
            AuthType::None => Space::new().into(),
            AuthType::Bearer => row![
                text("Token:"),
                text_input("", &self.current_tab().bearer_token)
                    .on_input(Message::BearerTokenChanged)
                    .width(Length::Fill)
                    .padding(8)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            AuthType::ApiKey => row![
                column![text("Key:"), text("Value:"), text("Add to:"),]
                    .align_x(Alignment::Center)
                    .spacing(10),
                column![
                    text_input("", &self.current_tab().api_key_name)
                        .on_input(Message::ApiKeyNameChanged)
                        .width(Length::Fill),
                    text_input("", &self.current_tab().api_key)
                        .on_input(Message::ApiKeyChanged)
                        .width(Length::Fill),
                    pick_list(
                        &ApiKeyPosition::ALL[..],
                        Some(self.current_tab().api_key_position),
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

    fn render_response_section(&self) -> Element<'_, Message> {
        fn status_color(status: &str) -> iced::Color {
            let code = status
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u16>().ok());

            match code {
                Some(200..=299) => iced::Color::from_rgb(0.2, 0.8, 0.2), // green
                Some(300..=399) => iced::Color::from_rgb(0.2, 0.6, 0.9), // blue
                Some(400..=499) => iced::Color::from_rgb(0.9, 0.6, 0.2), // orange
                Some(500..=599) => iced::Color::from_rgb(0.9, 0.2, 0.2), // red
                _ => iced::Color::WHITE,
            }
        }

        let status_view: Element<'_, Message> = if self.current_tab().loading {
            text("Loading...").into()
        } else if !self.current_tab().response_status.is_empty() {
            text(&self.current_tab().response_status)
                .color(status_color(&self.current_tab().response_status))
                .into()
        } else {
            Space::new().into()
        };

        let mut header_row = iced::widget::Row::new()
            .spacing(10)
            .height(20)
            .align_y(Alignment::Center);

        header_row = header_row.push(text("Response"));
        header_row = header_row.push(status_view);

        if let Some(resp_time) = self.current_tab().response_time {
            header_row = header_row.push(
                text(format!("⏱️{:.2}ms", resp_time.as_secs_f32() * 1000.0))
                    .shaping(text::Shaping::Advanced),
            );
        }
        if self.current_tab().is_response_binary {
            header_row = header_row.push(
                text(format!(
                    "🗃️{:.2} KB",
                    self.current_tab().response_bytes.len() as f32 / 1024.0
                ))
                .shaping(text::Shaping::Advanced),
            );
        }
        header_row = header_row.push(space::horizontal());
        header_row = header_row.push(text("Json Theme:"));
        header_row = header_row.push(pick_list(
            &json_highlighter::JsonThemeWrapper::ALL[..],
            Some(&self.json_theme),
            Message::JsonThemeChanged,
        ));
        if !self.current_tab().response_body_content.is_empty()
            || !self.current_tab().response_headers_content.is_empty()
        {
            header_row = header_row.push(tooltip(
                button(
                    text(if self.current_tab().copied {
                        "✅"
                    } else {
                        "📋"
                    })
                    .shaping(text::Shaping::Advanced),
                )
                .on_press(Message::CopyToClipboard)
                .style(button::text),
                if self.current_tab().copied {
                    "Copied"
                } else {
                    "Copy to Clipboard"
                },
                tooltip::Position::Bottom,
            ));
        }

        let res_tabs: iced_aw::Tabs<Message, ResponseTab, iced::Theme, iced::Renderer> =
            iced_aw::Tabs::new(Message::ResponseTabSelected)
                .push(
                    ResponseTab::Body,
                    iced_aw::TabLabel::Text("Body".into()),
                    container(self.with_overlay(self.render_response_body())).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .push(
                    ResponseTab::Headers,
                    iced_aw::TabLabel::Text("Header".into()),
                    container(self.with_overlay(self.render_response_headers())).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .height(Length::Fill)
                .set_active_tab(&self.current_tab().active_response_tab)
                .tab_bar_position(iced_aw::TabBarPosition::Top)
                .into();

        container(column![header_row, rule::horizontal(1.0), res_tabs].spacing(10))
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.palette().primary,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .height(Length::Fill)
            .padding(5.0)
            .into()
    }

    fn render_response_body(&self) -> Element<'_, Message> {
        if self.current_tab().is_response_binary {
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

            if self
                .current_tab()
                .response_content_type
                .starts_with("image/")
            {
                if let Some(handle) = &self.current_tab().image_handle {
                    body_column = body_column.push(
                        scrollable(
                            iced::widget::image(handle.clone())
                                .content_fit(iced::ContentFit::Contain),
                        )
                        .height(Length::Fill)
                        .width(Length::Fill),
                    );
                }
            } else if self
                .current_tab()
                .response_content_type
                .starts_with("video/")
            {
                // Video playback
                if let Some(video) = &self.current_tab().video_player {
                    let vs = self.current_tab().video_state.as_ref().unwrap();
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
                            .padding(Padding {
                                right: 10.0,
                                left: 10.0,
                                top: 5.0,
                                bottom: 5.0,
                            }),
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
                        self.current_tab().response_filename
                    ))
                    .shaping(text::Shaping::Advanced)
                    .style(|_| text::Style {
                        color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                    }),
                );
                body_column = body_column.push(text(format!(
                    "Size: {} bytes",
                    self.current_tab().response_bytes.len()
                )));
            }
            body_column.into()
        } else {
            let content: Element<'_, Message> =
                if self.current_tab().response_body_content.is_empty() {
                    space().into()
                } else {
                    text_editor(&self.current_tab().response_body_content)
                        .on_action(Message::ResponseBodyAction)
                        .highlight_with::<json_highlighter::JsonHighlighter>(
                            self.get_highlighter_settings(),
                            |highlight, _theme| {
                                let color = match highlight {
                                    json_highlighter::HighlightType::Syntax(color) => *color,
                                    json_highlighter::HighlightType::SearchMatch => {
                                        iced::Color::from_rgb(1.0, 1.0, 0.0)
                                    }
                                    json_highlighter::HighlightType::CurrentMatch => {
                                        iced::Color::from_rgb(1.0, 0.0, 1.0)
                                    }
                                };

                                iced::advanced::text::highlighter::Format {
                                    color: Some(color),
                                    font: None,
                                }
                            },
                        )
                        .style(|theme: &iced::Theme, status| Self::get_editor_style(theme, status))
                        .into()
                };
            scrollable(content).height(Length::FillPortion(1)).into()
        }
    }

    fn get_editor_style(theme: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
        let mut style = text_editor::Catalog::style(
            theme,
            &<iced::Theme as text_editor::Catalog>::default(),
            status,
        );
        style.border = Border {
            width: 0.0,
            ..style.border
        };
        style
    }

    fn render_response_headers(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> =
            if self.current_tab().response_headers_content.is_empty() {
                space().into()
            } else {
                text_editor(&self.current_tab().response_headers_content)
                    .on_action(Message::ResponseHeadersAction)
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
                    .style(|theme: &iced::Theme, status| Self::get_editor_style(theme, status))
                    .into()
            };

        scrollable(content).height(Length::FillPortion(1)).into()
    }

    fn loading_overlay(&self) -> Option<Element<'_, Message>> {
        if !self.current_tab().loading {
            return None;
        }

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
            })
            .into(),
        )
    }

    fn with_overlay<'a>(
        &'a self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let content = content.into();
        if let Some(overlay) = self.loading_overlay() {
            iced::widget::stack![content, overlay].into()
        } else {
            content
        }
    }

    fn update_query<F: FnOnce(&mut QueryParam)>(&mut self, idx: usize, f: F) {
        if let Some(param) = self.current_tab_mut().query_params.get_mut(idx) {
            f(param);
        }
        self.rebuild_url();
    }

    fn rebuild_url(&mut self) {
        use reqwest::Url;

        let base = self.current_tab().base_url.trim_end_matches('?');

        let mut url = Url::parse(base).expect("invalid base URL");

        {
            let mut pairs = url.query_pairs_mut();
            for p in &self.current_tab().query_params {
                if p.enabled && !p.key.is_empty() {
                    pairs.append_pair(&p.key, &p.value);
                }
            }
        }

        self.current_tab_mut().url = url.to_string();
    }

    fn parse_url_query(&mut self) {
        let tab = self.current_tab_mut();

        tab.query_params.clear();

        if let Some(q_index) = tab.url.find('?') {
            let query = &tab.url[q_index + 1..];

            for pair in query.split('&') {
                if pair.is_empty() {
                    continue;
                }

                let mut parts = pair.splitn(2, '=');
                let key = parts.next().unwrap_or("").to_string();
                let value = parts.next().unwrap_or("").to_string();

                tab.query_params.push(QueryParam {
                    key,
                    value,
                    enabled: true,
                });
            }
        }
    }

    fn send_request(&mut self) -> iced::Task<Message> {
        let mut url = self.current_tab().url.clone();
        let method = self.current_tab().method.clone();
        let body = self.current_tab().body_content.text();
        let headers = self.current_tab().headers.clone();
        let auth_type = self.current_tab().auth_type.clone();
        let bearer_token = self.current_tab().bearer_token.clone();
        let content_type = self.current_tab().content_type.clone();
        let form_data = self.current_tab().form_data.clone();
        let api_key = self.current_tab().api_key.clone();
        let api_key_name = self.current_tab().api_key_name.clone();
        let api_key_position = self.current_tab().api_key_position;

        // Reset cancel flag, start timer, and clear previous response time
        self.current_tab()
            .cancel_flag
            .store(false, Ordering::Relaxed);
        self.current_tab_mut().response_time = None;

        let cancel_flag = self.current_tab().cancel_flag.clone();

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
                let mut header_map: reqwest::header::HeaderMap = headers
                    .iter()
                    .filter(|h| h.enabled)
                    .filter_map(|h| {
                        let name = reqwest::header::HeaderName::from_bytes(h.key.trim().as_bytes())
                            .ok()?;
                        let value = reqwest::header::HeaderValue::from_str(h.value.trim()).ok()?;
                        Some((name, value))
                    })
                    .collect();

                // Add auth header
                if auth_type == AuthType::Bearer && !bearer_token.is_empty() {
                    if let Ok(hv) =
                        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", bearer_token))
                    {
                        header_map.insert(reqwest::header::AUTHORIZATION, hv);
                    }
                } else if auth_type == AuthType::ApiKey
                    && !api_key.is_empty()
                    && !api_key_name.is_empty()
                {
                    if api_key_position == ApiKeyPosition::Header {
                        if let Ok(hv) = reqwest::header::HeaderValue::from_str(api_key.as_str()) {
                            if let Ok(hn) = reqwest::header::HeaderName::try_from(&api_key_name) {
                                header_map.insert(hn, hv);
                            }
                        }
                    } else {
                        if let Ok(mut parsed) = url::Url::parse(&url) {
                            parsed
                                .query_pairs_mut()
                                .append_pair(&api_key_name, &api_key);

                            url = parsed.to_string();
                        }
                    }
                }

                println!("{url}");

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

                        let status_code = resp.status();
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

                        if is_binary && accepts_ranges && ct.starts_with("video/") {
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
                            body: if status_code == reqwest::StatusCode::NOT_FOUND
                                && body.is_empty()
                            {
                                "Ops! the requested resource was not found".to_string()
                            } else {
                                body
                            },
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
        let mut subscriptions = vec![self.svg_rotation_subscription(), self.event_subscription()];
        if self.current_tab().request_type == RequestType::WebSocket
            && self.current_tab().ws_connection_id > 0
        {
            let url = self.current_tab().url.clone();
            subscriptions.push(Self::websocket_subscription(
                self.current_tab().ws_connection_id,
                url,
            ));
        }

        iced::Subscription::batch(subscriptions)
    }

    fn websocket_subscription(ws_connection_id: usize, url: String) -> iced::Subscription<Message> {
        iced::advanced::subscription::from_recipe(WebSocketRecipe {
            ws_connection_id,
            url,
        })
    }

    fn svg_rotation_subscription(&self) -> iced::Subscription<Message> {
        if self.current_tab().loading {
            iced::time::every(std::time::Duration::from_millis(5)).map(|_| Message::Tick)
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
        Message::NoOp => iced::Task::none(),
        Message::TabSelected(index) => {
            app.active_tab = index;
            iced::Task::none()
        }
        Message::AddNewTab => {
            let new_id = app.tabs.len();
            app.tabs.push(TabState::new(new_id));
            app.active_tab = app.tabs.len() - 1;
            iced::Task::none()
        }
        Message::CloseTab(index) => {
            if app.tabs.len() > 1 {
                app.tabs.remove(index);
                if app.active_tab >= app.tabs.len() {
                    app.active_tab = app.tabs.len() - 1;
                } else if app.active_tab > index {
                    app.active_tab -= 1;
                }
            }
            iced::Task::none()
        }
        Message::RequestTypeSelected(req_type) => {
            match req_type {
                RequestType::HTTP => {
                    app.current_tab_mut().request_type = req_type;
                }
                RequestType::WebSocket => {
                    app.current_tab_mut().request_type = req_type;
                    // Initialize WebSocket state
                    app.current_tab_mut().ws_connected = false;
                    app.current_tab_mut().ws_messages = Vec::new();
                    app.current_tab_mut().ws_input = String::new();
                    app.current_tab_mut().ws_auto_scroll = true;
                    app.current_tab_mut().url = "wss://echo.websocket.org".to_string()
                }
                _ => {
                    app.current_tab_mut().response_body_content =
                        text_editor::Content::with_text("Ops! Sorry. Not implemented yet!");
                }
            }
            iced::Task::none()
        }
        Message::MethodSelected(method) => {
            app.current_tab_mut().method = method;
            iced::Task::none()
        }
        Message::UrlChanged(url) => {
            app.current_tab_mut().url = url;
            app.parse_url_query();
            iced::Task::none()
        }
        Message::SendRequest => {
            if !app.current_tab_mut().loading && !app.current_tab_mut().url.trim().is_empty() {
                app.current_tab_mut().loading = true;
                app.send_request()
            } else {
                iced::Task::none()
            }
        }
        Message::BodyAction(action) => {
            app.current_tab_mut().body_content.perform(action);
            iced::Task::none()
        }
        Message::AuthTypeSelected(auth_type) => {
            app.current_tab_mut().auth_type = auth_type;
            iced::Task::none()
        }
        Message::BearerTokenChanged(token) => {
            app.current_tab_mut().bearer_token = token;
            iced::Task::none()
        }
        Message::ContentTypeSelected(content_type) => {
            app.current_tab_mut().content_type = content_type;
            iced::Task::none()
        }
        Message::CancelRequest => {
            app.current_tab_mut()
                .cancel_flag
                .store(true, Ordering::Relaxed);
            app.current_tab_mut().loading = false;
            app.current_tab_mut().response_body_content =
                text_editor::Content::with_text("Request cancelled by user");
            app.current_tab_mut().response_status = "Cancelled".to_string();
            iced::Task::none()
        }
        Message::SaveBinaryResponse => {
            if !app.current_tab().is_response_binary {
                return iced::Task::none();
            }

            let file_name = app.current_tab().response_filename.clone();
            let response_bytes = app.current_tab().response_bytes.clone();

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
                |message| message,
            )
        }
        Message::FileSaved(result) => {
            match result {
                Ok(filename) => {
                    app.current_tab_mut().response_body_content = text_editor::Content::with_text(
                        &format!("File saved successfully: {}", filename),
                    )
                }
                Err(error) => {
                    app.current_tab_mut().response_body_content =
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
            if let Some(vp) = app.current_tab_mut().video_player.as_mut() {
                vp.set_paused(!vp.paused());
            }
            iced::Task::none()
        }
        Message::ToggleLoop => {
            if let Some(vp) = app.current_tab_mut().video_player.as_mut() {
                vp.set_looping(!vp.looping());
            }
            iced::Task::none()
        }
        Message::VideoVolume(vol) => {
            if let Some(vp) = app.current_tab_mut().video_player.as_mut() {
                vp.set_volume(vol);
            }
            iced::Task::none()
        }
        Message::Seek(secs) => {
            if let Some(vs) = app.current_tab_mut().video_state.as_mut() {
                vs.dragging = true;
                vs.position = secs;
            }
            if let Some(vp) = app.current_tab_mut().video_player.as_mut() {
                vp.set_paused(true);
            }
            iced::Task::none()
        }
        Message::SeekRelease => {
            let tab = app.current_tab_mut();
            if let (Some(vs), Some(vp)) = (tab.video_state.as_mut(), tab.video_player.as_mut()) {
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
            let tab = app.current_tab_mut();
            if let (Some(vs), Some(vp)) = (tab.video_state.as_mut(), tab.video_player.as_mut()) {
                if !vs.dragging {
                    vs.position = vp.position().as_secs_f64();
                }
            }
            iced::Task::none()
        }
        Message::ResponseReceived(resp) => {
            app.current_tab_mut().loading = false;
            app.current_tab_mut().active_response_tab = ResponseTab::Body;
            app.current_tab_mut().is_response_binary = resp.is_binary;

            app.current_tab_mut().response_status = resp.status;
            app.current_tab_mut().response_content_type = resp.content_type.clone();
            app.current_tab_mut().response_time = resp.response_time;
            // app.current_tab_mut().response_bytes = resp.bytes;

            if resp.is_binary && resp.content_type.starts_with("video/") && resp.accepts_range {
                let url = url::Url::parse(&app.current_tab().url).unwrap();

                match iced_video_player::Video::new(&url) {
                    Ok(video) => {
                        app.current_tab_mut().video_player = Some(video);
                        app.current_tab_mut().video_state = Some(VideoState {
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
                        app.current_tab_mut().video_player = None;
                    }
                }
            } else if resp.is_binary && resp.content_type.starts_with("image/") {
                // Create handle directly from the incoming bytes
                app.current_tab_mut().image_handle =
                    Some(iced::widget::image::Handle::from_bytes(resp.bytes.clone()));
            } else {
                app.current_tab_mut().video_player = None;
                app.current_tab_mut().video_state = None;

                app.current_tab_mut().response_headers_content =
                    text_editor::Content::with_text(&resp.headers);
                app.current_tab_mut().response_body_content =
                    text_editor::Content::with_text(&resp.body);
            }

            iced::Task::none()
        }
        Message::ResponseBodyAction(action) => {
            match action {
                text_editor::Action::Edit(_) => {}
                _ => app.current_tab_mut().response_body_content.perform(action),
            };
            iced::Task::none()
        }
        Message::ResponseHeadersAction(action) => {
            match action {
                text_editor::Action::Edit(_) => {}
                _ => app
                    .current_tab_mut()
                    .response_headers_content
                    .perform(action),
            };
            iced::Task::none()
        }
        Message::ResponseTabSelected(response_tab) => {
            app.current_tab_mut().active_response_tab = response_tab;
            iced::Task::none()
        }
        Message::ToggleLayout => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::PrettifyJson => {
            let body_text = app.current_tab().body_content.text();

            iced::Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let json: serde_json::Value =
                            serde_json::from_str(&body_text).map_err(|e| e.to_string())?;

                        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| e.to_string())?
                },
                Message::JsonPrettified,
            )
        }
        Message::JsonPrettified(Ok(pretty)) => {
            let tab = app.current_tab_mut();

            tab.body_content.perform(text_editor::Action::SelectAll);

            tab.body_content
                .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                    std::sync::Arc::new(pretty),
                )));

            iced::Task::none()
        }
        Message::JsonPrettified(Err(err)) => {
            eprintln!("Prettify failed: {err}");
            iced::Task::none()
        }
        Message::CopyToClipboard => {
            if app.current_tab().is_response_binary {
                return iced::Task::none();
            }

            let text = match app.current_tab().active_response_tab {
                ResponseTab::Body => app.current_tab().response_body_content.text(),
                ResponseTab::Headers => app.current_tab().response_headers_content.text(),
            };

            app.current_tab_mut().copied = true;

            iced::Task::batch([
                iced::clipboard::write(text),
                iced::Task::perform(
                    async {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    },
                    |_| Message::ResetCopied,
                ),
            ])
        }
        Message::ResetCopied => {
            app.current_tab_mut().copied = false;
            iced::Task::none()
        }
        Message::QueryParamAdd => {
            app.current_tab_mut().query_params.push(QueryParam::new());
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamRemove(idx) => {
            if idx < app.current_tab().query_params.len() {
                app.current_tab_mut().query_params.remove(idx);
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
            if let Some(field) = app.current_tab_mut().form_data.get_mut(index) {
                field.key = key;
            }
            iced::Task::none()
        }
        Message::FormFieldValueChanged(index, value) => {
            if let Some(field) = app.current_tab_mut().form_data.get_mut(index) {
                field.value = value;
            }
            iced::Task::none()
        }
        Message::FormFieldTypeSelected(idx, form_field_type) => {
            if let Some(field) = app.current_tab_mut().form_data.get_mut(idx) {
                field.field_type = form_field_type;
                field.value.clear();
                field.files.clear();
            }
            iced::Task::none()
        }
        Message::FormFieldToggled(idx) => {
            if let Some(field) = app.current_tab_mut().form_data.get_mut(idx) {
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
            let state = app
                .current_tab()
                .to_saved(&app.json_theme.to_string(), &app.app_theme.to_string());

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
            *app.current_tab_mut() = TabState::from_saved(saved_state);
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
            if let Some(field) = app.current_tab_mut().form_data.get_mut(index) {
                field.files = files;
            }
            iced::Task::none()
        }
        Message::FormFieldRemove(index) => {
            if index < app.current_tab().form_data.len() {
                app.current_tab_mut().form_data.remove(index);
            }
            iced::Task::none()
        }
        Message::FormFieldAdd => {
            app.current_tab_mut().form_data.push(FormField::new());
            iced::Task::none()
        }
        Message::ToggleFindDialog => {
            app.find_dialog_open = !app.find_dialog_open;
            iced::widget::operation::focus("find_input")
        }
        Message::ToggleFindReplaceDialog => {
            app.find_replace_mode = !app.find_replace_mode;
            iced::widget::operation::focus(if app.find_replace_mode {
                "replace_input"
            } else {
                "find_input"
            })
        }
        Message::CloseFindDialog => {
            app.find_dialog_open = false;

            // Clear search highlights
            app.search_match_positions = Vec::new();
            app.current_match_line_col = None;
            app.search_match_length = 0;
            app.current_match = 0;
            app.total_matches = 0;

            iced::Task::none()
        }
        Message::FindTextChanged(find_text) => {
            app.current_match = 0;
            app.total_matches = 0;
            app.current_match_pos = None;
            app.search_match_positions = Vec::new();
            app.current_match_line_col = None;
            app.search_match_length = 0;
            app.find_text = find_text;

            // Automatically find first match when text changes
            if !app.find_text.is_empty() {
                app.find_next();
            }

            iced::Task::none()
        }
        Message::ReplaceTextChanged(replace_text) => {
            app.replace_text = replace_text;
            iced::Task::none()
        }
        Message::ToggleCaseSensitive => {
            app.case_sensitive = !app.case_sensitive;

            // Re-search with new settings
            if !app.find_text.is_empty() {
                app.current_match = 0;
                app.find_next();
            }

            iced::Task::none()
        }
        Message::ToggleWholeWord => {
            app.whole_word = !app.whole_word;

            // Re-search with new settings
            if !app.find_text.is_empty() {
                app.current_match = 0;
                app.find_next();
            }

            iced::Task::none()
        }
        Message::FindNext => {
            app.find_next();

            iced::Task::none()
        }
        Message::FindPrevious => iced::Task::none(),
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
            if let Event::Window(window_event) = &event {
                match window_event {
                    iced::window::Event::Unfocused => {
                        println!("window was unfocused");
                        return iced::Task::none();
                    }
                    _ => {}
                }
            }
            if let Event::Keyboard(key_event) = event {
                match key_event {
                    KeyEvent::KeyPressed { key, modifiers, .. } if modifiers.control() => {
                        if let Key::Character(c) = &key {
                            if c.as_str() == "l" {
                                return iced::widget::operation::focus(
                                    app.current_tab().url_id.clone(),
                                )
                                .chain(
                                    iced::widget::operation::select_all(
                                        app.current_tab().url_id.clone(),
                                    ),
                                );
                            } else if c.as_str() == "f" {
                                return iced::Task::done(Message::ToggleFindDialog);
                            } else if c.as_str() == "h" {
                                return iced::Task::done(Message::ToggleFindReplaceDialog);
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
                        let tasks = vec![
                            iced::widget::operation::is_focused("find_input").then(|f| {
                                if f {
                                    iced::Task::done(Message::FindNext)
                                } else {
                                    iced::Task::none()
                                }
                            }),
                            iced::widget::operation::is_focused("replace_input").then(|f| {
                                if f {
                                    iced::Task::done(Message::Replace)
                                } else {
                                    iced::Task::none()
                                }
                            }),
                            iced::widget::operation::is_focused(app.current_tab().url_id.clone())
                                .then(|f| {
                                    if f {
                                        iced::Task::done(Message::SendRequest)
                                    } else {
                                        iced::Task::none()
                                    }
                                }),
                        ];
                        return iced::Task::batch(tasks);
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
        Message::RequestTabSelected(request_tab) => {
            app.current_tab_mut().active_request_tab = request_tab;
            iced::Task::none()
        }
        Message::ApiKeyPositionChanged(position) => {
            app.current_tab_mut().api_key_position = position;
            iced::Task::none()
        }
        Message::ApiKeyNameChanged(key) => {
            app.current_tab_mut().api_key_name = key;
            iced::Task::none()
        }
        Message::ApiKeyChanged(key) => {
            app.current_tab_mut().api_key = key;
            iced::Task::none()
        }
        Message::HeaderAdd => {
            app.current_tab_mut().headers.push(RequestHeaders::new());
            iced::Task::none()
        }
        Message::HeaderRemove(id) => {
            if id < app.current_tab().headers.len() {
                app.current_tab_mut().headers.remove(id);
            }
            iced::Task::none()
        }
        Message::HeaderKeyChanged(id, key) => {
            if let Some(header) = app.current_tab_mut().headers.get_mut(id) {
                header.key = key;
            }
            iced::Task::none()
        }
        Message::HeaderValueChanged(id, value) => {
            if let Some(header) = app.current_tab_mut().headers.get_mut(id) {
                header.value = value;
            }
            iced::Task::none()
        }
        Message::HeaderToggled(id) => {
            if let Some(header) = app.current_tab_mut().headers.get_mut(id) {
                header.enabled = !header.enabled;
            }
            iced::Task::none()
        }
        Message::WsConnect => {
            let url = app.current_tab().url.clone();

            // Validate URL
            if !url.starts_with("ws://") && !url.starts_with("wss://") {
                app.add_ws_system_message("Error: WebSocket URL must start with ws:// or wss://");
                return iced::Task::none();
            }

            app.current_tab_mut().loading = true;
            app.current_tab_mut().ws_connection_id += 1;
            app.add_ws_system_message(&format!("Connecting to {}...", url));

            iced::Task::none()
        }

        Message::WsDisconnect => {
            app.current_tab_mut().ws_connection_id = 0;
            app.current_tab_mut().ws_connection = None;
            app.current_tab_mut().ws_connected = false;
            app.current_tab_mut().loading = false;
            app.add_ws_system_message("Disconnected");
            iced::Task::none()
        }

        Message::WsEvent(event) => {
            match event {
                WsEvent::Connected(connection) => {
                    app.current_tab_mut().loading = false;
                    app.current_tab_mut().ws_connected = true;
                    app.current_tab_mut().ws_connection = Some(connection);
                    app.add_ws_system_message("Connected successfully");
                }
                WsEvent::Disconnected(reason) => {
                    app.current_tab_mut().ws_connection = None;
                    app.current_tab_mut().loading = false;
                    app.current_tab_mut().ws_connected = false;
                    app.add_ws_system_message(&format!("Disconnected: {}", reason));
                }
                WsEvent::MessageReceived(content) => {
                    app.add_ws_received_message(&content);
                }
                WsEvent::Error(error) => {
                    app.current_tab_mut().loading = false;
                    app.add_ws_system_message(&format!("Error: {}", error));
                }
            }
            iced::Task::none()
        }

        Message::WsMessageInputChanged(text) => {
            app.current_tab_mut().ws_input = text;
            iced::Task::none()
        }

        Message::WsSendMessage => {
            // Note: Sending requires a different approach - see next section
            let message = app.current_tab().ws_input.clone();

            if !message.is_empty() {
                app.add_ws_sent_message(&message);
                app.current_tab_mut().ws_input.clear();

                if let Some(conn) = app.current_tab_mut().ws_connection.as_mut() {
                    conn.send(message);
                }
            }

            iced::Task::none()
        }

        Message::WsClearMessages => {
            app.current_tab_mut().ws_messages.clear();
            iced::Task::none()
        }

        Message::WsToggleAutoScroll => {
            app.current_tab_mut().ws_auto_scroll = !app.current_tab().ws_auto_scroll;
            iced::Task::none()
        }
    }
}

fn view(app: &CrabiPie) -> Element<'_, Message> {
    let main_content = column![
        app.render_title_row(),
        app.render_tabs(),
        rule::horizontal(1.0),
        if app.current_tab().request_type == RequestType::WebSocket {
            app.render_websocket_panel()
        } else {
            app.render_active_tab_content()
        }
    ]
    .height(Length::Fill);

    let main_content: Element<'_, Message> = if app.find_dialog_open {
        let overlay = container(app.view_find_replace())
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill);
        // .style(|_theme: &Theme| container::Appearance {
        //     background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
        //     ..Default::default()
        // });

        iced::widget::stack![main_content, overlay].into()
    } else {
        main_content.into()
    };

    container(main_content)
        .padding(10)
        .height(Length::Fill)
        .into()
}

fn main() -> iced::Result {
    let icon_bytes = include_bytes!("../CrabIce.ico");
    iced::application(CrabiPie::new, update, view)
        .theme(|app: &CrabiPie| app.app_theme.clone())
        .subscription(|app| app.subscription())
        .title(|app: &CrabiPie| app.title())
        .window(iced::window::Settings {
            size: iced::Size::new(1500.0, 800.0),
            icon: iced::window::icon::from_file_data(icon_bytes, None).ok(), //rather than adding
            //iamge dependency , just let the function determine the file type in runtime
            ..Default::default()
        })
        .run()
}

const BODY_DEFAULT: &str = r#"{
  "title": "foo",
  "body": "bar",
  "userId": 1,
  "foo": "bar"
}"#;

#[derive(Debug, Hash, Clone)]
struct WebSocketRecipe {
    ws_connection_id: usize,
    url: String,
}

impl iced::advanced::subscription::Recipe for WebSocketRecipe {
    type Output = Message;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        use std::hash::Hash;
        self.ws_connection_id.hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: std::pin::Pin<
            Box<dyn futures::Stream<Item = iced::advanced::subscription::Event> + Send>,
        >,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = Self::Output> + Send>> {
        let url = self.url.clone();

        Box::pin(websocket_stream(url))
    }
}

// Create the actual stream function
fn websocket_stream(url: String) -> impl futures::Stream<Item = Message> {
    use futures::sink::SinkExt;
    use futures::stream::StreamExt;

    futures::stream::unfold((WebSocketState::Disconnected, 0 as u8), move |state| {
        let url = url.clone();
        async move {
            match state {
                (WebSocketState::Disconnected, retry_count) => {
                    // Stop retrying after 3 attempts
                    if retry_count >= 3 {
                        return Some((
                                Message::WsEvent(WsEvent::Error(
                                    "Failed to connect after 3 attempts. Please check the URL and try again.".to_string()
                                )),
                                (WebSocketState::Failed, retry_count)
                            ));
                    }

                    // Try to connect
                    match CrabiPie::connect_ws(&url).await {
                        Ok(websocket) => {
                            let (sender, receiver) = mpsc::channel(100);
                            let connection = WsConnection(sender);

                            Some((
                                Message::WsEvent(WsEvent::Connected(connection)),
                                (
                                    WebSocketState::Connected {
                                        websocket,
                                        receiver,
                                    },
                                    0,
                                ),
                            ))
                        }
                        Err(e) => {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            Some((
                                Message::WsEvent(WsEvent::Error(format!(
                                    "Connection attempt {} failed: {}",
                                    retry_count + 1,
                                    e
                                ))),
                                (WebSocketState::Disconnected, retry_count + 1),
                            ))
                        }
                    }
                }
                (WebSocketState::Failed, retry_count) => {
                    // Stop the stream - no more retries
                    None
                }
                (
                    WebSocketState::Connected {
                        mut websocket,
                        mut receiver,
                    },
                    _,
                ) => {
                    tokio::select! {
                        result = websocket.next() => {
                            match result {
                                Some(Ok(reqwest_websocket::Message::Text(text))) => {
                                    Some((
                                        Message::WsEvent(WsEvent::MessageReceived(text)),
                                        (WebSocketState::Connected { websocket, receiver }, 0)
                                    ))
                                }
                                Some(Err(e)) => {
                                    Some((
                                        Message::WsEvent(WsEvent::Disconnected(format!("{}", e))),
                                        (WebSocketState::Failed, 0) // Don't retry on disconnection
                                    ))
                                }
                                None => {
                                    Some((
                                        Message::WsEvent(WsEvent::Disconnected("Connection closed".to_string())),
                                        (WebSocketState::Failed, 0)
                                    ))
                                }
                                _ => {
                                    Some((
                                        Message::WsEvent(WsEvent::MessageReceived("[Other]".to_string())),
                                        (WebSocketState::Connected { websocket, receiver }, 0)
                                    ))
                                }
                            }
                        }

                        Some(message) = receiver.next() => { //ui related
                            match websocket.send(reqwest_websocket::Message::Text(message)).await {
                                Ok(_) => Some((
                                    Message::NoOp,
                                    (WebSocketState::Connected { websocket, receiver }, 0)
                                )),
                                Err(e) => {
                                    Some((
                                        Message::WsEvent(WsEvent::Disconnected(format!("Send error: {}", e))),
                                        (WebSocketState::Failed, 0)
                                    ))
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

// State enum for the stream
enum WebSocketState {
    Disconnected,
    Connected {
        websocket: reqwest_websocket::WebSocket,
        receiver: mpsc::Receiver<String>,
    },
    Failed,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
enum RequestType {
    HTTP,
    WebSocket,
    GraphQL,
    GRPC,
}

impl RequestType {
    const ALL: [RequestType; 4] = [
        RequestType::HTTP,
        RequestType::WebSocket,
        RequestType::GraphQL,
        RequestType::GRPC,
    ];
}

impl std::fmt::Display for RequestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

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
    ApiKey,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "No Auth"),
            AuthType::Bearer => write!(f, "Bearer Token"),
            AuthType::ApiKey => write!(f, "Api Key"),
        }
    }
}

impl AuthType {
    const ALL: [AuthType; 3] = [AuthType::None, AuthType::Bearer, AuthType::ApiKey];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
enum ApiKeyPosition {
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
    const ALL: [ApiKeyPosition; 2] = [ApiKeyPosition::Header, ApiKeyPosition::QueryParams];
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestHeaders {
    key: String,
    value: String,
    enabled: bool,
}

impl RequestHeaders {
    fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }

    fn default() -> Vec<RequestHeaders> {
        vec![
            Self {
                key: "User-Agent".to_string(),
                value: "CrabIce".to_string(),
                enabled: true,
            },
            Self {
                key: "Connection".to_string(),
                value: "keep-alive".to_string(),
                enabled: true,
            },
            Self {
                key: "Accept-Encoding".to_string(),
                value: "gzip, deflate, br".to_string(),
                enabled: true,
            },
        ]
    }
}

static HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
    reqwest::Client::builder()
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("failed to build http client")
});

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
enum RequestTab {
    Body,
    Headers,
    Auth,
    Query,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ResponseTab {
    Body,
    Headers,
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected(WsConnection),
    Disconnected(String),
    MessageReceived(String),
    Error(String),
}

pub enum WsCommand {
    Connect(String), // URL
    Disconnect,
    SendMessage(String),
}

#[derive(Debug, Clone)]
struct WsMessage {
    timestamp: String,
    direction: MessageDirection,
    content: String,
}

#[derive(Debug, Clone, PartialEq)]
enum MessageDirection {
    Sent,
    Received,
    System,
}

#[derive(Debug, Clone)]
struct WsMessageDisplay {
    timestamp: String,
    direction: MessageDirection,
    content: String,
}

#[derive(Debug, Clone)]
pub struct WsConnection(mpsc::Sender<String>);

impl WsConnection {
    pub fn send(&mut self, message: String) {
        self.0
            .try_send(message)
            .expect("Send message through WebSocket");
    }
}

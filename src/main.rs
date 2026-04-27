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
        space, text, text_editor, text_editor::Content, text_input, tooltip,
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
    TabBodyLoaded {
        id: usize,
        saved: Option<SavedState>,
    },
    RequestTabLoad(usize), // trigger load for tab index

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
    ClearResponseText,

    // GraphQL
    GraphqlQueryAction(text_editor::Action),
    GraphqlVariablesAction(text_editor::Action),
    GraphqlOperationChanged(String),
    FetchGraphqlSchema,
    GraphqlSchemaFetched(Result<GraphqlSchema, String>),
    GraphqlFieldToggled(String),
    GraphqlTypeToggled(String),
    GraphqlArgToggled(String),
    GraphqlSearchChanged(String),
    GraphqlCollapseAll,

    // Global Cookie auth
    CookieJarOpen,
    CookieJarClose,
    CookieJarAdd(String),
    CookieJarRemove(String, usize), // domain, index
    CookieJarToggled(String, usize),
    CookieJarNameChanged(String, usize, String),
    CookieJarValueChanged(String, usize, String),
    CookieJarDomainChanged(String),
    CookieJarClearDomain(String),

    // Query params
    QueryParamAdd,
    QueryParamRemove(usize),
    QueryParamKeyChanged(usize, String),
    QueryParamValueChanged(usize, String),
    QueryParamToggled(usize),

    // streaming response
    StreamChunk(String),
    StreamDone,

    // WebSocket messages
    WsConnect,
    WsDisconnect,
    WsEvent(WsEvent),
    WsMessageInputChanged(String),
    WsSendMessage,
    WsClearMessages,
    WsToggleAutoScroll,
    WsMessageEditorAction(text_editor::Action),
    WsMessageTypeSelected(WsMessageType),
    WsBinaryMessageTypeSelected(WsBinaryMessageType),

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
    ViewRawForm,
    ViewFormattedForm,
    FormRawAction(text_editor::Action),

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

    // Saving state
    StateLoaded(Option<SessionState>, Vec<TabMetadata>),
    SaveComplete,

    // Sidebar
    ToggleSidebar,
    SidebarItemSelected(usize),
    CollectionLoaded(Option<Collection>),
    CollectionFolderAdd(Option<usize>), // parent id, None = root
    CollectionItemToggleExpand(usize),
    CollectionRequestOpen(usize),
    CollectionItemRename(usize),
    CollectionItemRenameInput(String),
    CollectionItemRenameConfirm(usize),
    CollectionItemRenameCancel,
    CollectionItemDelete(usize),
    CollectionItemDuplicate(usize),
    // Save modal
    OpenSaveModal,
    SaveModalNameChanged(String),
    SaveModalFolderSelected(Option<usize>),
    SaveModalConfirm,
    SaveModalCancel,
    CollectionSaved,

    EventOccurred(Event),
}

struct CrabiPie {
    // Tab managemen
    tabs: Vec<TabLoadState>,
    active_tab: usize,
    next_tab_id: usize,

    // Global UI state (shared across all tabs)
    json_theme: json_highlighter::JsonThemeWrapper,
    app_theme: iced::Theme,
    svg_rotation: f32,

    // Global cookie jar
    cookie_jar_open: bool,
    cookie_jar_new_domain: String,
    cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
    cookie_jar_error: Option<String>,

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

    // collection
    sidebar_open: bool,
    sidebar_selected_id: Option<usize>,
    collection: Collection,
    next_collection_id: usize,
    sidebar_editing_id: Option<usize>,
    sidebar_editing_name: String,
    // Save to collection modal
    save_modal_open: bool,
    save_modal_name: String,
    save_modal_folder_id: Option<usize>,
}

struct TabState {
    metadata: TabMetadata,
    id: usize,
    title: String,

    // Request configuration
    url_id: iced::widget::Id,
    url: String,
    request_type: RequestType,
    method: HttpMethod,
    headers: Vec<RequestHeaders>,
    body_content: text_editor::Content,
    form_view_type: FormViewType,
    auth_type: AuthType,
    bearer_token: String,
    api_key_name: String,
    api_key: String,
    api_key_position: ApiKeyPosition,
    content_type: ContentType,
    query_params: Vec<QueryParam>,
    form_data: Vec<FormField>,
    raw_form_content: text_editor::Content,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    // GraphQL Editors ---
    graphql_query: text_editor::Content,
    graphql_variables: text_editor::Content,
    graphql_operation: String,
    graphql_schema: Option<GraphqlSchema>,
    graphql_schema_loading: bool,
    graphql_schema_error: Option<String>,
    graphql_expanded_types: std::collections::HashSet<String>,
    graphql_search: String,
    graphql_selected_paths: std::collections::HashSet<String>,
    manually_selected_paths: std::collections::HashSet<String>,

    // response stream
    stream_buffer: String,
    is_streaming: bool,

    // WebSocket-specific fields
    ws_connected: bool,
    ws_connection: Option<WsConnection>,
    ws_auto_scroll: bool,
    ws_input: String,
    ws_messages_content: text_editor::Content,
    ws_count_sent: usize,
    ws_count_received: usize,
    ws_message_type: WsMessageType,
    ws_binary_message_type: WsBinaryMessageType,

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
    dirty: bool,
}

impl TabState {
    fn new(id: usize) -> Self {
        Self {
            metadata: TabMetadata {
                id,
                title: format!("Request {}", id + 1),
                url: "https://jsonplaceholder.typicode.com/posts".to_string(),
                method: HttpMethod::GET,
                request_type: RequestType::HTTP,
            },
            id,
            title: format!("Request {}", id + 1),
            url_id: iced::widget::Id::unique(),
            url: "https://jsonplaceholder.typicode.com/posts".to_string(),
            request_type: RequestType::HTTP,
            method: HttpMethod::GET,
            headers: RequestHeaders::default(),
            body_content: text_editor::Content::with_text(BODY_DEFAULT),
            form_view_type: FormViewType::Formatted,
            auth_type: AuthType::None,
            bearer_token: String::new(),
            api_key_name: String::new(),
            api_key: String::new(),
            api_key_position: ApiKeyPosition::Header,
            content_type: ContentType::Json,
            query_params: vec![QueryParam::new()],
            form_data: vec![FormField::new()],
            raw_form_content: text_editor::Content::with_text(""),
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
            ws_input: String::new(),
            ws_auto_scroll: true,
            ws_connection: None,
            ws_connection_id: 0,
            ws_messages_content: text_editor::Content::new(),
            ws_count_sent: 0,
            ws_count_received: 0,
            ws_message_type: WsMessageType::Text,
            ws_binary_message_type: WsBinaryMessageType::Base64,
            is_streaming: false,
            stream_buffer: String::new(),
            graphql_query: text_editor::Content::new(),
            graphql_variables: text_editor::Content::with_text("{}"),
            graphql_operation: String::new(),
            graphql_schema: None,
            graphql_schema_loading: false,
            graphql_schema_error: None,
            graphql_expanded_types: std::collections::HashSet::new(),
            graphql_search: String::new(),
            graphql_selected_paths: std::collections::HashSet::new(),
            manually_selected_paths: std::collections::HashSet::new(),
            dirty: false,
        }
    }

    fn from_saved(saved: SavedState) -> Self {
        Self {
            metadata: TabMetadata {
                id: saved.id,
                title: saved.title.clone(),
                url: saved.url.clone(),
                method: saved.method,
                request_type: saved.request_type,
            },
            id: saved.id,
            title: saved.title,
            url_id: iced::widget::Id::unique(),
            url: saved.url,
            request_type: saved.request_type,
            method: saved.method,
            headers: saved.headers,
            body_content: text_editor::Content::with_text(&saved.body),
            form_view_type: saved.form_view_type,
            auth_type: saved.auth_type,
            bearer_token: saved.bearer_token,
            api_key_name: saved.api_key_name,
            api_key: saved.api_key,
            api_key_position: saved.api_key_position,
            content_type: saved.content_type,
            query_params: saved.query_params,
            form_data: saved.form_data,
            raw_form_content: text_editor::Content::with_text(&saved.raw_form_content),
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
            active_request_tab: saved.active_request_tab,
            active_response_tab: saved.active_response_tab,
            copied: false,
            ws_connected: false,
            ws_input: String::new(),
            ws_auto_scroll: true,
            ws_connection: None,
            ws_connection_id: 0,
            ws_messages_content: text_editor::Content::new(),
            ws_count_sent: 0,
            ws_count_received: 0,
            ws_message_type: saved.ws_message_type,
            ws_binary_message_type: saved.ws_binary_message_type,
            is_streaming: false,
            stream_buffer: String::new(),
            graphql_query: text_editor::Content::with_text(&saved.graphql_query),
            graphql_variables: text_editor::Content::with_text(&saved.graphql_variables),
            graphql_operation: saved.graphql_operation,
            graphql_schema: saved.graphql_schema,
            graphql_schema_loading: false,
            graphql_schema_error: saved.graphql_schema_error,
            graphql_expanded_types: saved.graphql_expanded_types,
            graphql_search: String::new(),
            graphql_selected_paths: saved.graphql_selected_paths,
            manually_selected_paths: saved.manually_selected_paths,
            dirty: false,
        }
    }

    fn to_saved(&self, json_theme: &str, app_theme: &str) -> SavedState {
        SavedState {
            id: self.id,
            title: self.title.clone(),
            url: self.url.clone(),
            request_type: self.request_type.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
            body: self.body_content.text(),
            form_view_type: self.form_view_type,
            auth_type: self.auth_type.clone(),
            bearer_token: self.bearer_token.clone(),
            api_key_name: self.api_key_name.clone(),
            api_key: self.api_key.clone(),
            api_key_position: self.api_key_position,
            content_type: self.content_type.clone(),
            query_params: self.query_params.clone(),
            form_data: self.form_data.clone(),
            raw_form_content: self.raw_form_content.text(),
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
            ws_message_type: self.ws_message_type,
            ws_binary_message_type: self.ws_binary_message_type,
            graphql_query: self.graphql_query.text(),
            graphql_variables: self.graphql_variables.text(),
            graphql_operation: self.graphql_operation.clone(),
            graphql_schema: self.graphql_schema.clone(),
            graphql_schema_error: self.graphql_schema_error.clone(),
            graphql_expanded_types: self.graphql_expanded_types.clone(),
            graphql_selected_paths: self.graphql_selected_paths.clone(),
            manually_selected_paths: self.manually_selected_paths.clone(),

            active_request_tab: self.active_request_tab,
            active_response_tab: self.active_response_tab,
        }
    }

    fn form_data_to_raw(form_data: &[FormField]) -> String {
        form_data
            .iter()
            .filter_map(|f| {
                if f.key.is_empty() && f.value.is_empty() || f.field_type == FormFieldType::File {
                    None
                } else {
                    let mut line = format!("{}: {}", f.key, f.value);
                    if !f.enabled {
                        line = format!("# {}", line);
                    }
                    Some(line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn raw_to_form_data(raw: &str) -> Vec<FormField> {
        raw.lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }

                let (enabled, content) = if let Some(rest) = line.strip_prefix('#') {
                    (false, rest)
                } else {
                    (true, line)
                };

                let (key, value) = content.split_once(':')?;

                Some(FormField {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                    enabled,
                    field_type: FormFieldType::Text,
                    files: vec![],
                })
            })
            .collect()
    }
}

impl CrabiPie {
    fn new() -> (Self, iced::Task<Message>) {
        let app = Self {
            tabs: vec![TabLoadState::Loaded(Box::new(TabState::new(0)))],
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
            sidebar_open: false,
            sidebar_selected_id: None,
            collection: Collection::new(),
            next_collection_id: 1,
            sidebar_editing_id: None,
            sidebar_editing_name: String::new(),
            save_modal_open: false,
            save_modal_name: String::new(),
            save_modal_folder_id: None,
            cookie_jar_open: false,
            cookie_jar_new_domain: String::new(),
            cookie_jar: std::collections::HashMap::new(),
            cookie_jar_error: None,
        };

        let task = iced::Task::batch([
            iced::Task::perform(load_app_state(), |(session, metadata)| {
                Message::StateLoaded(session, metadata)
            }),
            iced::Task::perform(load_collection(), Message::CollectionLoaded),
        ]);

        (app, task)
    }

    fn title(&self) -> String {
        "CrabiPie".to_string()
    }

    fn current_tab(&self) -> Option<&TabState> {
        let slot = self.tabs.get(self.active_tab)?;
        match slot {
            TabLoadState::Loaded(state) => Some(state),
            _ => None,
        }
    }

    fn current_tab_mut(&mut self) -> Option<&mut TabState> {
        let slot = self.tabs.get_mut(self.active_tab)?;
        match slot {
            TabLoadState::Loaded(state) => {
                state.dirty = true;
                Some(state)
            }
            _ => None,
        }
    }

    fn add_tab(&mut self) {
        let new_tab = TabLoadState::Loaded(Box::new(TabState::new(self.next_tab_id)));
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
            Space::new().into()
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
            Space::new().into()
        };

        let match_info: Element<'_, Message> = if !self.find_text.is_empty() {
            text(format!(
                "{} / {} matches",
                self.current_match, self.total_matches,
            ))
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            Space::new().into()
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

        let text = self
            .current_tab()
            .map(|t| t.response_body_content.text())
            .unwrap_or_default();
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

        let text = self
            .current_tab()
            .map(|t| t.response_body_content.text())
            .unwrap_or_default();
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
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
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

        let editor = text_editor(&tab.ws_messages_content)
            .on_action(Message::WsMessageEditorAction)
            .highlight_with::<json_highlighter::LogHighlighter>((), |color, _theme| {
                iced::advanced::text::highlighter::Format {
                    color: Some(*color),
                    font: None,
                }
            })
            .height(Length::Fill)
            .style(|theme: &iced::Theme, status| {
                let mut style = text_editor::Catalog::style(
                    theme,
                    &<iced::Theme as text_editor::Catalog>::default(),
                    status,
                );
                style.border.width = 0.0;
                // style.background = iced::Background::Color(iced::Color::TRANSPARENT);
                style
            });

        let messages_area = container(editor)
            .height(Length::Fill)
            .padding(10)
            .style(|th| container::Style {
                background: Some(iced::Background::Color(th.palette().background)),
                border: Border {
                    width: 1.0,
                    color: th.extended_palette().background.weak.color,
                    radius: 5.0.into(),
                },
                ..Default::default()
            });

        let mut input_row = iced::widget::Row::new().spacing(10);

        input_row = input_row.push(
            pick_list(
                &WsMessageType::ALL[..],
                Some(tab.ws_message_type),
                Message::WsMessageTypeSelected,
            )
            .padding(8),
        );

        if tab.ws_message_type == WsMessageType::Binary {
            input_row = input_row.push(
                pick_list(
                    &WsBinaryMessageType::ALL[..],
                    Some(tab.ws_binary_message_type),
                    Message::WsBinaryMessageTypeSelected,
                )
                .padding(8),
            );
        }

        // Message input area
        input_row = input_row.push(
            text_input("Type message...", &tab.ws_input)
                .on_input(Message::WsMessageInputChanged)
                .on_submit(Message::WsSendMessage)
                .padding(8)
                .width(Length::Fill),
        );

        input_row = input_row.push(
            button(text("Send").size(14))
                .on_press_maybe(if is_connected && !tab.ws_input.is_empty() {
                    Some(Message::WsSendMessage)
                } else {
                    None
                })
                .padding(10)
                .style(button::primary),
        );

        // Update stats to use the counters
        let stats = text(format!(
            "Sent: {} | Received: {}",
            tab.ws_count_sent, tab.ws_count_received
        ))
        .size(11)
        .color(iced::Color::from_rgb(0.5, 0.5, 0.5));

        column![
            connection_row,
            messages_area,
            container(stats).padding(5).center_x(Length::Fill),
            input_row,
        ]
        .spacing(10)
        .into()
    }

    fn render_graphql_tab(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let is_connected = tab.ws_connected;

        // ── left: schema tree ────────────────
        let schema_panel: Element<'_, Message> = if tab.graphql_schema_loading {
            container(text("Loading schema...").size(14))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        } else if let Some(err) = &tab.graphql_schema_error {
            container(text(err).style(|_| text::Style {
                color: Some(iced::Color::from_rgb(1.0, 0.3, 0.3)),
            }))
            .padding(10)
            .into()
        } else if let Some(schema) = &tab.graphql_schema {
            // --- Search + Collapse Bar ---
            let top_bar = row![
                text_input("Search fields", &tab.graphql_search)
                    .on_input(Message::GraphqlSearchChanged)
                    .width(Length::Fill)
                    .padding(5),
                button(text("󰡍").size(16))
                    .style(button::text)
                    .padding(5)
                    .on_press(Message::GraphqlCollapseAll),
                button(text("↻").size(16))
                    .style(button::text)
                    .padding(5)
                    .on_press(Message::FetchGraphqlSchema),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            // --- Root Type Row ---
            let root_type_name = &schema.query_type.name;
            let root_expanded = tab.graphql_expanded_types.contains(root_type_name);

            let root_row = row![
                button(text(if root_expanded { "▼" } else { "▶" }).size(10))
                    .style(button::text)
                    .on_press(Message::GraphqlTypeToggled(root_type_name.clone())),
                text(root_type_name).size(16),
            ]
            .align_y(Alignment::Center);

            let mut scroll_content = Column::new().spacing(0); // Set spacing to 0 for tight rows
            scroll_content = scroll_content.push(
                // container(root_row).padding(Padding::new(0.0).top(8)), // Space before root
                root_row,
            );

            if root_expanded {
                if let Some(root_type) = schema
                    .types
                    .iter()
                    .find(|t| t.name.as_ref() == Some(root_type_name))
                {
                    let tree_rows = render_schema_tree(
                        &schema.types,
                        root_type,
                        &tab.graphql_expanded_types,
                        &tab.graphql_selected_paths,
                        &tab.graphql_search,
                        1, // Indentation level
                        "",
                    );
                    for row in tree_rows {
                        scroll_content = scroll_content.push(row);
                    }
                }
            }

            column![
                top_bar,
                scrollable(scroll_content)
                    .direction(scrollable::Direction::Both {
                        vertical: scrollable::Scrollbar::default()
                            .width(12.0) // Make the bar wide enough to see
                            .spacing(10.0) // Force 10px of "dead zone" between text and bar
                            .scroller_width(8.0),
                        horizontal: scrollable::Scrollbar::default()
                            .width(12.0)
                            .spacing(10.0)
                            .scroller_width(8.0),
                    })
                    .height(Length::Fill)
                    .width(Length::Fill)
            ]
            .spacing(4) // Tight spacing between bar and content
            .height(Length::Fill)
            .into()
        } else {
            // ... (Empty State - unchanged)
            container(
                button("Fetch Schema")
                    .style(button::text)
                    .on_press(Message::FetchGraphqlSchema),
            )
            .into()
        };

        // ── right: query + variables ──────────
        let operation_row = row![
            text("Operation:").size(12),
            text_input("optional", &tab.graphql_operation)
                .on_input(Message::GraphqlOperationChanged)
                .width(Length::FillPortion(1)),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let right_panel = column![
            operation_row,
            text("Query:").size(12),
            text_editor(&tab.graphql_query)
                .on_action(Message::GraphqlQueryAction)
                .height(Length::FillPortion(3)),
            text("Variables (JSON):").size(12),
            text_editor(&tab.graphql_variables)
                .on_action(Message::GraphqlVariablesAction)
                .height(Length::FillPortion(2)),
        ]
        .spacing(8)
        .height(Length::Fill);

        // ── main layout ──────────────────────
        row![
            container(schema_panel)
                .width(Length::FillPortion(1))
                .height(Length::Fill),
            rule::vertical(1),
            container(right_panel)
                .width(Length::FillPortion(2))
                .height(Length::Fill)
        ]
        .spacing(5)
        .height(Length::Fill)
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

    fn add_to_log(&mut self, prefix: &str, message: &str) {
        let Some(tab) = self.current_tab_mut() else {
            return;
        };
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let formatted = format!("[{}] {} {}\n", timestamp, prefix, message);
        tab.ws_messages_content
            .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
        tab.ws_messages_content
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                std::sync::Arc::new(formatted),
            )));
    }

    pub fn add_ws_received_message(&mut self, content: &str) {
        self.add_to_log("←", content);
        if let Some(tab) = self.current_tab_mut() {
            tab.ws_count_received += 1;
        }
    }

    pub fn add_ws_system_message(&mut self, content: &str) {
        self.add_to_log("•", content);
    }

    // Call this in your WsSendMessage logic
    pub fn add_ws_sent_message(&mut self, content: &str) {
        self.add_to_log("→", content);
        if let Some(tab) = self.current_tab_mut() {
            tab.ws_count_sent += 1;
        }
    }

    fn save_task(&self) -> iced::Task<Message> {
        let json_theme_str = self.json_theme.to_string();
        let app_theme_str = self.app_theme.to_string();

        let session = SessionState {
            active_tab: self.active_tab,
            json_theme: json_theme_str.clone(),
            app_theme: app_theme_str.clone(),
            next_tab_id: self.next_tab_id,
            cookie_jar: self.cookie_jar.clone(),
        };

        let metadata: Vec<TabMetadata> = self.tabs.iter().map(|t| t.metadata().clone()).collect();

        let dirty_tabs: Vec<SavedState> = self
            .tabs
            .iter()
            .filter_map(|t| match t {
                TabLoadState::Loaded(s) if s.dirty => {
                    Some(s.to_saved(&json_theme_str, &app_theme_str))
                }
                _ => None,
            })
            .collect();
        println!("Saving {} dirty tabs", dirty_tabs.len());

        iced::Task::perform(
            async move {
                SessionState::save(&session).await;
                TabMetadata::save_all(&metadata).await;
                for saved in dirty_tabs {
                    saved.save().await;
                }
            },
            |_| Message::SaveComplete,
        )
    }

    fn collection_save_task(&self) -> iced::Task<Message> {
        let collection = self.collection.clone();
        iced::Task::perform(save_collection(collection), |_| Message::CollectionSaved)
    }

    fn next_collection_id(&mut self) -> usize {
        let id = self.next_collection_id;
        self.next_collection_id += 1;
        id
    }

    // Find and remove an item by id, returning it
    fn collection_remove_item(
        items: &mut Vec<CollectionItem>,
        id: usize,
    ) -> Option<CollectionItem> {
        if let Some(pos) = items.iter().position(|i| i.id() == id) {
            return Some(items.remove(pos));
        }
        for item in items.iter_mut() {
            if let CollectionItem::Folder(f) = item {
                if let Some(found) = Self::collection_remove_item(&mut f.children, id) {
                    return Some(found);
                }
            }
        }
        None
    }

    // Find a request by id across all nested items
    fn collection_find_request(items: &[CollectionItem], id: usize) -> Option<&CollectionRequest> {
        for item in items {
            match item {
                CollectionItem::Request(r) if r.id == id => return Some(r),
                CollectionItem::Folder(f) => {
                    if let Some(found) = Self::collection_find_request(&f.children, id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    // Insert item into a folder by id, or root if None
    fn collection_insert_into(
        items: &mut Vec<CollectionItem>,
        target_folder_id: Option<usize>,
        item: CollectionItem,
    ) -> bool {
        // return true if inserted
        match target_folder_id {
            None => {
                items.push(item);
                true
            }
            Some(folder_id) => {
                for existing in items.iter_mut() {
                    if let CollectionItem::Folder(f) = existing {
                        if f.id == folder_id {
                            f.children.push(item);
                            return true;
                        }
                        if Self::collection_insert_into(
                            &mut f.children,
                            Some(folder_id),
                            item.clone(),
                        ) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }

    fn collection_toggle_expand(items: &mut Vec<CollectionItem>, id: usize) {
        for item in items.iter_mut() {
            if let CollectionItem::Folder(f) = item {
                if f.id == id {
                    f.expanded = !f.expanded;
                    return;
                }
                Self::collection_toggle_expand(&mut f.children, id);
            }
        }
    }

    fn collection_rename_item(items: &mut Vec<CollectionItem>, id: usize, new_name: String) {
        for item in items.iter_mut() {
            match item {
                CollectionItem::Folder(f) if f.id == id => {
                    f.name = new_name;
                    return;
                }
                CollectionItem::Request(r) if r.id == id => {
                    r.name = new_name;
                    return;
                }
                CollectionItem::Folder(f) => {
                    Self::collection_rename_item(&mut f.children, id, new_name.clone());
                }
                _ => {}
            }
        }
    }

    fn collection_duplicate_item(items: &mut Vec<CollectionItem>, id: usize, new_id: usize) {
        // find position and parent
        for i in 0..items.len() {
            match &items[i] {
                CollectionItem::Request(r) if r.id == id => {
                    let mut cloned = r.clone();
                    cloned.id = new_id;
                    cloned.name = format!("{} (copy)", cloned.name);
                    items.insert(i + 1, CollectionItem::Request(cloned));
                    return;
                }
                CollectionItem::Folder(f) if f.id == id => {
                    let mut cloned = f.clone();
                    cloned.id = new_id;
                    cloned.name = format!("{} (copy)", cloned.name);
                    items.insert(i + 1, CollectionItem::Folder(cloned));
                    return;
                }
                CollectionItem::Folder(_) => {
                    if let CollectionItem::Folder(f) = &mut items[i] {
                        Self::collection_duplicate_item(&mut f.children, id, new_id);
                    }
                }
                _ => {}
            }
        }
    }

    fn render_sidebar(&self) -> Element<'_, Message> {
        // Header
        let header = row![
            text("Collection").size(14),
            space::horizontal(),
            tooltip(
                button(text("+📁").shaping(text::Shaping::Advanced).size(12))
                    .style(button::text)
                    .on_press(Message::CollectionFolderAdd(None)),
                "New folder at root",
                tooltip::Position::Bottom
            ),
            tooltip(
                button(text("💾").shaping(text::Shaping::Advanced).size(12))
                    .style(button::text)
                    .on_press(Message::OpenSaveModal),
                "Save current tab",
                tooltip::Position::Bottom
            ),
        ]
        .align_y(Alignment::Center)
        .spacing(4);

        // Render items recursively
        let items: Element<'_, Message> = if self.collection.items.is_empty() {
            container(
                text("No saved requests.\nClick 💾 to save current tab.")
                    .size(12)
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .padding(10)
            .into()
        } else {
            column(
                self.collection
                    .items
                    .iter()
                    .map(|item| self.render_collection_item(item, 0))
                    .collect::<Vec<_>>(),
            )
            .spacing(2)
            .into()
        };

        let content = column![
            header,
            rule::horizontal(1.0),
            scrollable(items).height(Length::Fill),
        ]
        .spacing(8);

        container(content)
            .width(Length::Fixed(240.0))
            .height(Length::Fill)
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.extended_palette().background.weak.color,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .padding(8)
            .into()
    }

    fn render_collection_item<'a>(
        &'a self,
        item: &'a CollectionItem,
        depth: u16,
    ) -> Element<'a, Message> {
        let indent = depth as f32 * 14.0;
        let is_selected = self.sidebar_selected_id == Some(item.id());

        match item {
            CollectionItem::Folder(folder) => {
                let arrow = if folder.expanded { "▾" } else { "▸" };

                let name_or_input: Element<'_, Message> =
                    if self.sidebar_editing_id == Some(folder.id) {
                        text_input("Folder name...", &self.sidebar_editing_name)
                            .id("sidebar_rename")
                            .on_input(Message::CollectionItemRenameInput)
                            .on_submit(Message::CollectionItemRenameConfirm(folder.id))
                            .size(13)
                            .into()
                    } else {
                        let row_btn = button(
                            row![
                                text(arrow).size(12),
                                text("📁").shaping(text::Shaping::Advanced).size(12),
                                text(&folder.name).size(13),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .style(move |theme: &iced::Theme, status| {
                            if is_selected {
                                button::Style {
                                    background: Some(iced::Background::Color(
                                        theme.extended_palette().primary.weak.color,
                                    )),
                                    text_color: theme.extended_palette().primary.weak.text,
                                    border: Border::default(),
                                    shadow: iced::Shadow::default(),
                                    snap: false,
                                }
                            } else {
                                button::text(theme, status)
                            }
                        })
                        .width(Length::Fill)
                        .on_press(Message::SidebarItemSelected(folder.id));

                        iced_aw::ContextMenu::new(row_btn, move || {
                            let context_items = column![
                                button(
                                    row![text("📁+").size(12), text(" Add Subfolder").size(13)]
                                        .spacing(4)
                                )
                                .style(button::text)
                                .width(Length::Fill)
                                .on_press(Message::CollectionFolderAdd(Some(folder.id))),
                                button(
                                    row![
                                        text("✏️").shaping(text::Shaping::Advanced).size(12),
                                        text(" Rename").size(13)
                                    ]
                                    .spacing(4)
                                )
                                .style(button::text)
                                .width(Length::Fill)
                                .on_press(Message::CollectionItemRename(folder.id)),
                                button(
                                    row![
                                        text("🗑").shaping(text::Shaping::Advanced).size(12),
                                        text(" Delete").size(13)
                                    ]
                                    .spacing(4)
                                )
                                .style(button::text)
                                .width(Length::Fill)
                                .on_press(Message::CollectionItemDelete(folder.id)),
                            ]
                            .padding(4);

                            container(context_items)
                                .style(|theme: &iced::Theme| container::Style {
                                    background: Some(iced::Background::Color(
                                        theme.palette().background,
                                    )),
                                    border: Border {
                                        width: 1.0,
                                        color: theme.extended_palette().background.weak.color,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                })
                                .width(Length::Fixed(160.0))
                                .into()
                        })
                        .into()
                    };

                let row_content = row![space::horizontal().width(indent), name_or_input,]
                    .align_y(Alignment::Center);

                if folder.expanded {
                    let children: Vec<Element<'_, Message>> = folder
                        .children
                        .iter()
                        .map(|child| self.render_collection_item(child, depth + 1))
                        .collect();

                    column![row_content, column(children).spacing(2)]
                        .spacing(2)
                        .into()
                } else {
                    row_content.into()
                }
            }

            CollectionItem::Request(req) => {
                let method_color = match req.method {
                    HttpMethod::GET => iced::Color::from_rgb(0.27, 0.73, 0.27),
                    HttpMethod::POST => iced::Color::from_rgb(0.98, 0.65, 0.14),
                    HttpMethod::PUT => iced::Color::from_rgb(0.14, 0.59, 0.98),
                    HttpMethod::DELETE => iced::Color::from_rgb(0.95, 0.26, 0.21),
                    HttpMethod::PATCH => iced::Color::from_rgb(0.61, 0.15, 0.69),
                    _ => iced::Color::from_rgb(0.5, 0.5, 0.5),
                };

                let name_or_input: Element<'_, Message> = if self.sidebar_editing_id == Some(req.id)
                {
                    text_input("Request name...", &self.sidebar_editing_name)
                        .id("sidebar_rename")
                        .on_input(Message::CollectionItemRenameInput)
                        .on_submit(Message::CollectionItemRenameConfirm(req.id))
                        .size(13)
                        .into()
                } else {
                    let row_btn = button(
                        row![
                            text(req.method.to_string()).size(10).color(method_color),
                            text(&req.name).size(13),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .style(move |theme: &iced::Theme, status| {
                        if is_selected {
                            button::Style {
                                background: Some(iced::Background::Color(
                                    theme.extended_palette().primary.weak.color,
                                )),
                                text_color: theme.extended_palette().primary.weak.text,
                                border: Border::default(),
                                shadow: iced::Shadow::default(),
                                snap: false,
                            }
                        } else {
                            button::text(theme, status)
                        }
                    })
                    .width(Length::Fill)
                    .on_press(Message::SidebarItemSelected(req.id));

                    let req_id = req.id;
                    iced_aw::ContextMenu::new(row_btn, move || {
                        let context_items = column![
                            button(
                                row![text("↗").size(12), text(" Open in new tab").size(13)]
                                    .spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionRequestOpen(req_id)),
                            button(
                                row![
                                    text("✏️").shaping(text::Shaping::Advanced).size(12),
                                    text(" Rename").size(13)
                                ]
                                .spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionItemRename(req_id)),
                            button(
                                row![text("⧉").size(12), text(" Duplicate").size(13)].spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionItemDuplicate(req_id)),
                            button(
                                row![
                                    text("🗑").shaping(text::Shaping::Advanced).size(12),
                                    text(" Delete").size(13)
                                ]
                                .spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionItemDelete(req_id)),
                        ]
                        .padding(4);

                        container(context_items)
                            .style(|theme: &iced::Theme| container::Style {
                                background: Some(iced::Background::Color(
                                    theme.palette().background,
                                )),
                                border: Border {
                                    width: 1.0,
                                    color: theme.extended_palette().background.weak.color,
                                    radius: 4.0.into(),
                                },
                                ..Default::default()
                            })
                            .width(Length::Fixed(160.0))
                            .into()
                    })
                    .into()
                };

                row![space::horizontal().width(indent), name_or_input,]
                    .align_y(Alignment::Center)
                    .into()
            }
        }
    }

    fn render_save_modal(&self) -> Element<'_, Message> {
        // Build folder list for picker — flatten all folders
        let mut folder_options: Vec<(Option<usize>, String)> = vec![(None, "Root".to_string())];

        fn collect_folders(
            items: &[CollectionItem],
            depth: usize,
            out: &mut Vec<(Option<usize>, String)>,
        ) {
            for item in items {
                if let CollectionItem::Folder(f) = item {
                    let indent = "  ".repeat(depth);
                    out.push((Some(f.id), format!("{}📁 {}", indent, f.name)));
                    collect_folders(&f.children, depth + 1, out);
                }
            }
        }
        collect_folders(&self.collection.items, 0, &mut folder_options);

        let folder_labels: Vec<String> = folder_options.iter().map(|(_, l)| l.clone()).collect();

        let selected_label = folder_options
            .iter()
            .find(|(id, _)| *id == self.save_modal_folder_id)
            .map(|(_, l)| l.clone())
            .unwrap_or("Root".to_string());

        let modal_content = column![
            text("Save to Collection").size(16),
            rule::horizontal(1.0),
            column![
                text("Name").size(12),
                text_input("Request name...", &self.save_modal_name)
                    .on_input(Message::SaveModalNameChanged)
                    .on_submit(Message::SaveModalConfirm)
                    .padding(8),
            ]
            .spacing(4),
            column![
                text("Save into folder").size(12),
                pick_list(folder_labels, Some(selected_label), move |selected| {
                    let folder_id = folder_options
                        .iter()
                        .find(|(_, l)| l == &selected)
                        .map(|(id, _)| *id)
                        .unwrap_or(None);
                    Message::SaveModalFolderSelected(folder_id)
                },)
                .padding(8)
                .width(Length::Fill),
            ]
            .spacing(4),
            row![
                space::horizontal(),
                button("Cancel")
                    .style(button::secondary)
                    .on_press(Message::SaveModalCancel)
                    .padding(8),
                button("Save")
                    .style(button::primary)
                    .on_press(Message::SaveModalConfirm)
                    .padding(8),
            ]
            .spacing(8),
        ]
        .spacing(12);

        container(modal_content)
            .width(Length::Fixed(340.0))
            .padding(20)
            .style(|theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(theme.palette().background)),
                border: Border {
                    width: 1.0,
                    color: theme.palette().primary,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn format_duration(dur: std::time::Duration) -> String {
        let secs = dur.as_secs_f64();
        if secs < 1.0 {
            format!("{:.1}ms", secs * 1000.0)
        } else if secs < 60.0 {
            format!("{:.2}s", secs)
        } else if secs < 3600.0 {
            let m = secs as u64 / 60;
            let s = secs as u64 % 60;
            format!("{}m {}s", m, s)
        } else {
            let h = secs as u64 / 3600;
            let m = (secs as u64 % 3600) / 60;
            format!("{}h {}m", h, m)
        }
    }

    pub fn build_query(&self) -> String {
        let Some(tab) = self.current_tab() else {
            return String::from("Loading...");
        };

        let mut buf = String::from("query {\n");

        if let Some(schema) = &tab.graphql_schema {
            // Find the "Query" type (or whatever your query_type is named)
            if let Some(root_type) = schema
                .types
                .iter()
                .find(|t| t.name.as_deref() == Some("Query"))
            {
                buf.push_str(&generate_selection_set(
                    &schema.types,
                    root_type,
                    &tab.graphql_selected_paths,
                    1,       // Start at indent level 1
                    "Query", // Starting path
                ));
            }
        }

        buf.push('}');
        buf
    }
}

enum TabLoadState {
    Unloaded(TabMetadata),
    Loading(TabMetadata), // background task running
    Loaded(Box<TabState>),
}
impl TabLoadState {
    fn id(&self) -> usize {
        self.metadata().id
    }

    fn metadata(&self) -> &TabMetadata {
        match self {
            TabLoadState::Unloaded(m) => m,
            TabLoadState::Loading(m) => m,
            TabLoadState::Loaded(s) => &s.metadata,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TabMetadata {
    id: usize,
    title: String,
    url: String,
    method: HttpMethod,
    request_type: RequestType,
}
impl TabMetadata {
    async fn save_metadata(tabs: Vec<TabMetadata>) {
        let path = state_dir().join("tabs/metadata.json");
        if let Ok(json) = serde_json::to_string(&tabs) {
            tokio::fs::write(path, json).await.ok();
        }
    }

async fn load_all() -> Vec<TabMetadata> {
    let path = state_dir().join("tabs/metadata.json");
    let bytes = tokio::fs::read(&path).await;
    let result: Vec<TabMetadata> = bytes.ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    println!("metadata ids: {:?}", result.iter().map(|m| m.id).collect::<Vec<_>>());
    result
}

    async fn save_all(tabs: &[TabMetadata]) {
        let path = state_dir().join("tabs/metadata.json");
        if let Ok(json) = serde_json::to_vec(tabs) {
            tokio::fs::write(path, json).await.ok();
        }
    }
}

// Minimal session file (no tabs)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionState {
    active_tab: usize,
    json_theme: String,
    app_theme: String,
    next_tab_id: usize,
    cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
}

impl SessionState {
    async fn load() -> Option<SessionState> {
        let bytes = tokio::fs::read(state_file_path()).await.ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    async fn save(&self) {
        if let Ok(json) = serde_json::to_vec(self) {
            tokio::fs::write(state_file_path(), json).await.ok();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedState {
    id: usize,
    title: String,
    url: String,
    request_type: RequestType,
    method: HttpMethod,
    headers: Vec<RequestHeaders>,
    body: String,
    form_view_type: FormViewType,
    auth_type: AuthType,
    bearer_token: String,
    api_key_name: String,
    api_key: String,
    api_key_position: ApiKeyPosition,
    content_type: ContentType,
    query_params: Vec<QueryParam>,
    form_data: Vec<FormField>,
    raw_form_content: String,
    json_theme: String,
    app_theme: String,

    ws_message_type: WsMessageType,
    ws_binary_message_type: WsBinaryMessageType,

    // Response (only when NOT binary)
    response_status: Option<String>,
    response_headers: Option<String>,
    response_body: Option<String>,

    // GraphQL
    graphql_query: String,
    graphql_variables: String,
    graphql_operation: String,
    graphql_schema: Option<GraphqlSchema>,
    graphql_schema_error: Option<String>,
    graphql_expanded_types: std::collections::HashSet<String>,
    graphql_selected_paths: std::collections::HashSet<String>,
    manually_selected_paths: std::collections::HashSet<String>,

    active_request_tab: RequestTab,
    active_response_tab: ResponseTab,
}

impl Default for SavedState {
    fn default() -> Self {
        let base = "https://jsonplaceholder.typicode.com/posts".to_string();
        Self {
            id: 0,
            title: "Request-1".into(),
            url: base.clone(),
            request_type: RequestType::HTTP,
            method: HttpMethod::GET,
            headers: vec![RequestHeaders::new()],
            body: String::new(),
            form_view_type: FormViewType::Formatted,
            auth_type: AuthType::None,
            api_key_position: ApiKeyPosition::Header,
            bearer_token: String::new(),
            api_key_name: String::new(),
            api_key: String::new(),
            content_type: ContentType::Json,
            query_params: vec![QueryParam::new()],
            form_data: vec![FormField::new()],
            raw_form_content: String::new(),
            json_theme: String::new(),
            app_theme: String::new(),
            response_status: None,
            response_headers: None,
            response_body: None,
            ws_message_type: WsMessageType::Text,
            ws_binary_message_type: WsBinaryMessageType::Base64,
            graphql_query: String::new(),
            graphql_variables: String::new(),
            graphql_operation: String::new(),
            graphql_schema: None,
            graphql_schema_error: None,
            graphql_expanded_types: std::collections::HashSet::new(),
            graphql_selected_paths: std::collections::HashSet::new(),
            manually_selected_paths: std::collections::HashSet::new(),
            active_request_tab: RequestTab::Query,
            active_response_tab: ResponseTab::Body,
        }
    }
}

impl SavedState {
    async fn save(&self) {
        tokio::fs::create_dir_all(state_dir().join("tabs"))
            .await
            .ok();

        let path = state_dir().join(format!("tabs/{}.bin", self.id));
        if let Ok(json) = serde_json::to_vec(&self) {
            // gzip compress
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            use std::io::Write;
            if encoder.write_all(&json).is_ok() {
                if let Ok(compressed) = encoder.finish() {
                    tokio::fs::write(path, compressed).await.ok();
                }
            }
        }
    }

    async fn load(id: usize) -> Option<SavedState> {
        let path = state_dir().join(format!("tabs/{}.bin", id));
        let compressed = tokio::fs::read(path).await.ok()?;
        let mut decoder = flate2::read::GzDecoder::new(compressed.as_slice());
        let mut json = Vec::new();
        use std::io::Read;
        decoder.read_to_end(&mut json).ok()?;
        serde_json::from_slice::<SavedState>(&json).ok()
    }
}

impl CrabiPie {
    fn view_tab_content(&self) -> Element<'_, Message> {
        match &self.tabs[self.active_tab] {
            TabLoadState::Loaded(_) => self.render_active_tab_content(),
            TabLoadState::Loading(_) | TabLoadState::Unloaded(_) => {
                iced::widget::container(iced::widget::text("Loading..."))
                    .center(iced::Fill)
                    .into()
            }
        }
    }

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
            let tab_title = tab.metadata().title.clone();
            let tab_button = button(
                row![text(tab_title), button_or_space]
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
            button(text(if self.sidebar_open { "◀" } else { "▶" }).size(14))
                .style(button::text)
                .on_press(Message::ToggleSidebar),
            text("CrabiPie HTTP Client").size(16),
            space::horizontal(),
            text("App theme"),
            pick_list(
                &iced::Theme::ALL[..],
                Some(&self.app_theme),
                Message::AppThemeChanged,
            ),
            button(text("📂").shaping(text::Shaping::Advanced).size(14))
                .style(button::text)
                .on_press(Message::LoadRequest),
            button(text("💾").shaping(text::Shaping::Advanced).size(14))
                .style(button::text)
                .on_press(Message::SaveRequest)
        ]
        .spacing(10)
        .into()
    }

    fn render_request_row(&self) -> Element<'_, Message> {
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
                text("⏹ Cancel")
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

    fn render_request_section(&self) -> Element<'_, Message> {
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

    fn render_body_tab(&self) -> Element<'_, Message> {
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
                Message::PrettifyJson
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
                text_editor(&tab.body_content)
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
                    text_editor(&tab.raw_form_content)
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

    fn render_form_data(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let is_url_encoded = matches!(tab.content_type, ContentType::XWWWFormUrlEncoded);

        let mut fields_col = Column::new().spacing(10);

        for (idx, field) in tab.form_data.iter().enumerate() {
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
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let mut params_col = Column::new().spacing(10);

        for (idx, param) in tab.query_params.iter().enumerate() {
            let checkbox =
                checkbox(param.enabled).on_toggle(move |_| Message::QueryParamToggled(idx));

            let key_input = text_input("key", &param.key)
                .on_input(move |key| Message::QueryParamKeyChanged(idx, key))
                .width(200);

            let value_input = text_input("value", &param.value)
                .on_input(move |val| Message::QueryParamValueChanged(idx, val))
                .width(300);

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
            .align_y(Alignment::Center);

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

    fn build_query_string(&self) -> String {
        let Some(tab) = self.current_tab() else {
            return "Loading...".into();
        };
        let params: Vec<String> = tab
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
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let mut headers_col = Column::new().spacing(10);

        for (idx, header) in tab.headers.iter().enumerate() {
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
            AuthType::ApiKey => row![
                column![text("Key:"), text("Value:"), text("Add to:"),]
                    .align_x(Alignment::Center)
                    .spacing(10),
                column![
                    text_input("", &tab.api_key_name)
                        .on_input(Message::ApiKeyNameChanged)
                        .width(Length::Fill),
                    text_input("", &tab.api_key)
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

    fn render_cookie_jar_modal(&self) -> Element<'_, Message> {
        if !self.cookie_jar_open {
            return Space::new().into();
        }

        let content: Element<'_, Message> = if self.cookie_jar.is_empty() {
            text("No cookies stored yet.").into()
        } else {
            let domain_sections: Vec<Element<'_, Message>> = self
                .cookie_jar
                .iter()
                .map(|(domain, cookies)| {
                    let domain_header: Element<'_, Message> = row![
                        text(domain.clone()),
                        space::horizontal(),
                        tooltip(
                            button(text("➕").shaping(text::Shaping::Advanced))
                                .on_press(Message::CookieJarAdd(domain.clone())),
                            "Add",
                            tooltip::Position::Bottom
                        ),
                        tooltip(
                            button(text("🧹").shaping(text::Shaping::Advanced))
                                .on_press(Message::CookieJarClearDomain(domain.clone())),
                            "Clear",
                            tooltip::Position::Bottom
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into();

                    let col_headers: Element<'_, Message> = row![
                        text("").width(24), // checkbox space
                        text("Name").size(11).width(Length::FillPortion(2)),
                        text("Value").size(11).width(Length::FillPortion(3)),
                        text("Expires").size(11).width(Length::FillPortion(2)),
                        text("").width(30), // delete btn space
                    ]
                    .spacing(8)
                    .into();

                    let cookie_rows: Vec<Element<'_, Message>> = cookies
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let d1 = domain.clone();
                            let d2 = domain.clone();
                            let d3 = domain.clone();
                            let d4 = domain.clone();
                            row![
                                checkbox(c.enabled)
                                    .on_toggle(move |_| Message::CookieJarToggled(d1.clone(), i)),
                                text_input("", &c.name)
                                    .on_input(move |v| Message::CookieJarNameChanged(
                                        d2.clone(),
                                        i,
                                        v
                                    ))
                                    .width(Length::FillPortion(2)),
                                text_input("", &c.value)
                                    .on_input(move |v| Message::CookieJarValueChanged(
                                        d3.clone(),
                                        i,
                                        v
                                    ))
                                    .width(Length::FillPortion(3)),
                                text(c.expires.as_deref().unwrap_or("session")).size(11),
                                button(text("❌").shaping(text::Shaping::Advanced))
                                    .style(button::text)
                                    .on_press(Message::CookieJarRemove(d4.clone(), i)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .into()
                        })
                        .collect();

                    column![
                        domain_header,
                        col_headers,
                        Column::with_children(cookie_rows).spacing(6),
                    ]
                    .spacing(6)
                    .into()
                })
                .collect();

            scrollable(Column::with_children(domain_sections).spacing(16))
                .height(Length::Fixed(300.0))
                .into()
        };

        let add_row: Element<'_, Message> = row![
            text_input("domain (e.g. api.example.com)", &self.cookie_jar_new_domain)
                .on_input(Message::CookieJarDomainChanged)
                .width(Length::Fill),
            button("+ Add Cookie")
                .on_press(Message::CookieJarAdd(self.cookie_jar_new_domain.clone())),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        let error_el: Element<'_, Message> = match &self.cookie_jar_error {
            Some(err) => text(err.clone())
                .style(|theme| text::Style {
                    color: Some(iced::Color::from_rgb(1.0, 0.3, 0.3)),
                })
                .into(),
            None => Space::new().into(),
        };

        let modal_body = column![
            column![
                row![
                    text("Cookie Jar"),
                    space::horizontal(),
                    button("✕")
                        .on_press(Message::CookieJarClose)
                        .style(button::text),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                rule::horizontal(1),
            ],
            content,
            column![error_el, add_row]
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
        .style(|theme| container::Style {
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

        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let status_view: Element<'_, Message> = if tab.loading {
            text("Loading...").into()
        } else if !tab.response_status.is_empty() {
            text(&tab.response_status)
                .color(status_color(&tab.response_status))
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

        if let Some(resp_time) = tab.response_time {
            header_row = header_row.push(
                text(format!("⏱️ {}", Self::format_duration(resp_time)))
                    .shaping(text::Shaping::Advanced),
            );
        }
        if tab.is_response_binary {
            header_row = header_row.push(
                text(format!(
                    "🗃️ {:.2} KB",
                    tab.response_bytes.len() as f32 / 1024.0
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
        if !tab.response_body_content.is_empty() || !tab.response_headers_content.is_empty() {
            header_row = header_row.push(tooltip(
                button(text(if tab.copied { "✅" } else { "📋" }).shaping(text::Shaping::Advanced))
                    .on_press(Message::CopyToClipboard)
                    .style(button::text),
                if tab.copied {
                    "Copied"
                } else {
                    "Copy to Clipboard"
                },
                tooltip::Position::Bottom,
            ));
        }
        header_row = header_row.push(tooltip(
            button(text("🧹").shaping(text::Shaping::Advanced))
                .on_press(Message::ClearResponseText)
                .style(button::text),
            "Clear",
            tooltip::Position::Bottom,
        ));

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
                .set_active_tab(&tab.active_response_tab)
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
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        if tab.is_response_binary {
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

            if tab.response_content_type.starts_with("image/") {
                if let Some(handle) = &tab.image_handle {
                    body_column = body_column.push(
                        scrollable(
                            iced::widget::image(handle.clone())
                                .content_fit(iced::ContentFit::Contain),
                        )
                        .height(Length::Fill)
                        .width(Length::Fill),
                    );
                }
            } else if tab.response_content_type.starts_with("video/") {
                // Video playback
                if let Some(video) = &tab.video_player {
                    let vs = tab.video_state.as_ref().unwrap();
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
                        tab.response_filename
                    ))
                    .shaping(text::Shaping::Advanced)
                    .style(|_| text::Style {
                        color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                    }),
                );
                body_column =
                    body_column.push(text(format!("Size: {} bytes", tab.response_bytes.len())));
            }
            body_column.into()
        } else {
            let content: Element<'_, Message> = if tab.response_body_content.is_empty() {
                space().into()
            } else {
                text_editor(&tab.response_body_content)
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
                    .wrapping(iced::advanced::text::Wrapping::Glyph)
                    .style(Self::get_editor_style)
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

        style.border.width = 0.0;

        style
    }

    fn render_response_headers(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let content: Element<'_, Message> = if tab.response_headers_content.is_empty() {
            space().into()
        } else {
            text_editor(&tab.response_headers_content)
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

        scrollable(content)
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .height(Length::FillPortion(1))
            .into()
    }

    fn loading_overlay(&self) -> Option<Element<'_, Message>> {
        let Some(tab) = self.current_tab() else {
            return Some(iced::widget::text("Loading...").into());
        };

        if !tab.loading {
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
        if let Some(tab) = self.current_tab_mut() {
            if let Some(param) = tab.query_params.get_mut(idx) {
                f(param);
            }
        }
        self.rebuild_url();
    }

    fn rebuild_url(&mut self) {
        use reqwest::Url;

        let Some(tab) = self.current_tab() else {
            return;
        };

        let current = tab.url.clone();
        let mut url = Url::parse(&current).expect("invalid URL");

        // wipe existing query completely
        url.set_query(None);

        {
            let mut pairs = url.query_pairs_mut();
            for p in &tab.query_params {
                if p.enabled && !p.key.is_empty() {
                    pairs.append_pair(&p.key, &p.value);
                }
            }
        }

        if let Some(tab) = self.current_tab_mut() {
            tab.url = url.to_string();
        }
    }

    fn parse_url_query(&mut self) {
        let Some(tab) = self.current_tab_mut() else {
            return;
        };

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

    fn build_request(&self) -> Option<(reqwest::RequestBuilder, String)> {
        let tab = self.current_tab()?;
        let mut url = tab.url.clone();

        // ── headers ──────────────────────────
        let mut header_map: reqwest::header::HeaderMap = tab
            .headers
            .iter()
            .filter(|h| h.enabled)
            .filter_map(|h| {
                let name = reqwest::header::HeaderName::from_bytes(h.key.trim().as_bytes()).ok()?;
                let value = reqwest::header::HeaderValue::from_str(h.value.trim()).ok()?;
                Some((name, value))
            })
            .collect();

        // ── auth ──────────────────────────────
        match tab.auth_type {
            AuthType::Bearer => {
                if !tab.bearer_token.is_empty() {
                    if let Ok(hv) = reqwest::header::HeaderValue::from_str(&format!(
                        "Bearer {}",
                        tab.bearer_token
                    )) {
                        header_map.insert(reqwest::header::AUTHORIZATION, hv);
                    }
                }
            }
            AuthType::ApiKey => {
                if !tab.api_key.is_empty() && !tab.api_key_name.is_empty() {
                    if tab.api_key_position == ApiKeyPosition::Header {
                        if let (Ok(hn), Ok(hv)) = (
                            reqwest::header::HeaderName::try_from(&tab.api_key_name),
                            reqwest::header::HeaderValue::from_str(&tab.api_key),
                        ) {
                            header_map.insert(hn, hv);
                        }
                    } else if let Ok(mut parsed) = url::Url::parse(&url) {
                        parsed
                            .query_pairs_mut()
                            .append_pair(&tab.api_key_name, &tab.api_key);
                        url = parsed.to_string();
                    }
                }
            }
            AuthType::None => {}
        }

        // ── cookie jar ───────────────────────
        if let Some(domain) = extract_domain(&url) {
            if let Some(cookies) = self.cookie_jar.get(&domain) {
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
        let client = &HTTP_CLIENT;
        if tab.request_type == RequestType::GraphQL {
            let variables: serde_json::Value = serde_json::from_str(&tab.graphql_variables.text())
                .unwrap_or(serde_json::Value::Object(Default::default()));

            let body = serde_json::json!({
                "query": tab.graphql_query.text(),
                "variables": variables,
                "operationName": if tab.graphql_operation.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(tab.graphql_operation.clone())
                }
            });

            return Some((
                client
                    .post(&url)
                    .body(body.to_string())
                    .header("Content-Type", "application/json")
                    .headers(header_map),
                url,
            ));
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
                        .body(tab.body_content.text())
                        .header("Content-Type", "application/json"),
                    ContentType::XWWWFormUrlEncoded => {
                        let params: Vec<_> = tab
                            .form_data
                            .iter()
                            .filter(|f| {
                                f.enabled
                                    && !f.key.is_empty()
                                    && f.field_type == FormFieldType::Text
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
                                                let part = reqwest::multipart::Part::bytes(fc)
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

        Some((builder.headers(header_map), url))
    }

    fn send_request(&mut self) -> iced::Task<Message> {
        let Some((request, _url)) = self.build_request() else {
            return iced::Task::none();
        };
        let Some(tab) = self.current_tab() else {
            return iced::Task::none();
        };
        let cancel_flag = tab.cancel_flag.clone();
        tab.cancel_flag.store(false, Ordering::Relaxed);
        if let Some(mut_tab) = self.current_tab_mut() {
            mut_tab.response_time = None;
            mut_tab.stream_buffer = String::new();
        }

        iced::Task::run(
            async_stream::stream! {
                let start_time = tokio::time::Instant::now();
                let resp = match request.send().await {
                    Ok(r)  => r,
                    Err(e) => {
                        let msg = if e.is_timeout() { "Request timed out".into() }
                                  else { format!("Request failed: {e}") };
                        yield Message::ResponseReceived(HttpResponse {
                            status: "Error".to_string(),
                            body: msg,
                            response_time: Some(start_time.elapsed()),
                            ..Default::default()
                        });
                        return;
                    }
                };

                // ── emit status + headers immediately ───────────────────────────
                let status = format!(
                    "{} {}",
                    resp.status().as_u16(),
                    resp.status().canonical_reason().unwrap_or("")
                );

                let hm = resp.headers().clone();

                // Cookies
                let set_cookies: Vec<String> = hm.get_all("set-cookie")
                    .iter()
                    .filter_map(|v| v.to_str().ok().map(|s| s.to_string()))
                    .collect();

                // Headers all
                let headers_text = format!("{:#?}", hm);

                // Content Type
                let ct = hm.get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("").to_string();

                // Binary / video: fall back to old buffered path
                let is_binary = ct.starts_with("image/")
                    || ct.starts_with("application/pdf")
                    || ct.starts_with("application/octet-stream")
                    || ct.starts_with("video/")
                    || ct.starts_with("audio/");

                if is_binary {
                    let accepts_range = hm.get("accept-ranges").and_then(|h| h.to_str().ok()).is_some();
                    let filename = hm.get("content-disposition")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.split("filename=").nth(1)
                            .map(|f| f.trim_matches(|c| c == '"' || c == '\'').to_string()))
                        .unwrap_or_else(|| _url.split('/').last().unwrap_or("download").to_string());

                    if accepts_range && ct.starts_with("video/") {
                        yield Message::ResponseReceived(HttpResponse {
                            status, headers: headers_text, is_binary: true,
                            filename, content_type: ct,
                            response_time: Some(start_time.elapsed()),
                            accepts_range: true, ..Default::default()
                        });
                        return;
                    }

                    let (body, bytes) = match resp.bytes().await {
                        Ok(b)  => (format!("Binary file ({} bytes)\n\nContent-Type: {}", b.len(), ct), b.to_vec()),
                        Err(e) => (format!("Error reading binary data: {e}"), vec![]),
                    };
                    yield Message::ResponseReceived(HttpResponse {
                        status, headers: headers_text, body, is_binary: true,
                        filename, bytes, content_type: ct,
                        response_time: Some(start_time.elapsed()),
                        set_cookies,
                        accepts_range,
                    });
                    return;
                }

                // ── TEXT: stream chunks ─────────────────────────────────────────
                // Emit a "headers ready" snapshot so the UI can show status immediately
                yield Message::ResponseReceived(HttpResponse {
                    status: status.clone(),
                    headers: headers_text,
                    body: String::new(),
                    content_type: ct.clone(),
                    response_time: Some(start_time.elapsed()),
                    set_cookies,
                    ..Default::default()
                });

            use futures_util::StreamExt;
            let mut byte_stream = resp.bytes_stream();
            let mut buf: Vec<u8> = Vec::new();

            while let Some(chunk_result) = byte_stream.next().await {
                if cancel_flag.load(Ordering::Relaxed) {
                    yield Message::StreamChunk("…[cancelled]".to_string());
                    break;
                }
                match chunk_result {
                    Ok(bytes) => {
                        buf.extend_from_slice(bytes.as_ref());
                        match std::str::from_utf8(buf.as_slice()) {
                            Ok(s) => {
                                yield Message::StreamChunk(s.to_string());
                                buf.clear();
                            }
                            Err(e) => {
                                let valid_up_to = e.valid_up_to();
                                if valid_up_to > 0 {
                                    let s = unsafe {
                                        std::str::from_utf8_unchecked(&buf.as_slice()[..valid_up_to])
                                    }.to_string();
                                    let remaining = buf.as_slice()[valid_up_to..].to_vec();
                                    buf = remaining;
                                    yield Message::StreamChunk(s);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Message::StreamChunk(format!("\n[stream error: {e}]"));
                        break;
                    }
                }
            }

            yield Message::StreamDone;
                },
            std::convert::identity, // stream already yields Message
        )
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let mut subscriptions = vec![self.svg_rotation_subscription(), self.event_subscription()];

        if let Some(tab) = self.current_tab()
            && tab.request_type == RequestType::WebSocket
            && tab.ws_connection_id > 0
        {
            let url = tab.url.clone();
            subscriptions.push(Self::websocket_subscription(tab.ws_connection_id, url));
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
        if let Some(tab) = self.current_tab()
            && tab.loading
        {
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
    if app.sidebar_editing_id.is_some() {
        let is_rename_message = matches!(
            &message,
            Message::CollectionItemRenameInput(_)
                | Message::CollectionItemRenameConfirm(_)
                | Message::CollectionItemRenameCancel
                | Message::EventOccurred(_)
        );
        if !is_rename_message {
            let id = app.sidebar_editing_id.unwrap();
            let new_name = app.sidebar_editing_name.trim().to_string();
            if !new_name.is_empty() {
                CrabiPie::collection_rename_item(&mut app.collection.items, id, new_name);
            }
            app.sidebar_editing_id = None;
            app.sidebar_editing_name = String::new();
        }
    }
    match message {
        Message::NoOp => iced::Task::none(),
        Message::TabSelected(index) => {
            app.active_tab = index;
            match &app.tabs[index] {
                TabLoadState::Loaded(_) => iced::Task::none(), // instant
                TabLoadState::Unloaded(meta) => {
                    let id = meta.id;
                    app.tabs[index] = TabLoadState::Loading(meta.clone());
                    iced::Task::perform(SavedState::load(id), move |s| Message::TabBodyLoaded {
                        id,
                        saved: s,
                    })
                }
                TabLoadState::Loading(_) => iced::Task::none(), // already in flight
            }
        }
        Message::AddNewTab => {
            let new_id = app.next_tab_id;
            app.next_tab_id += 1;
            app.tabs
                .push(TabLoadState::Loaded(Box::new(TabState::new(new_id))));
            app.active_tab = app.tabs.len() - 1;
            iced::Task::none()
        }
        Message::TabBodyLoaded { id, saved } => {
            // find the slot and hydrate it
            if let Some(slot) = app.tabs.iter_mut().find(|t| t.id() == id) {
                *slot = match saved {
                    Some(s) => TabLoadState::Loaded(Box::new(TabState::from_saved(s))),
                    None => TabLoadState::Unloaded(slot.metadata().clone()), // fallback
                };
            }

            // after active tab done, schedule background loading for others
            // proximity order: active±1, active±2, ...
            let active = app.active_tab;
            let unloaded: Vec<usize> = proximity_order(active, app.tabs.len())
                .into_iter()
                .filter(|&i| matches!(app.tabs[i], TabLoadState::Unloaded(_)))
                .collect();

            if let Some(&next_idx) = unloaded.first() {
                if let TabLoadState::Unloaded(meta) = &app.tabs[next_idx] {
                    let id = meta.id;
                    app.tabs[next_idx] = TabLoadState::Loading(meta.clone());
                    // low priority — runs in background while UI is live
                    return iced::Task::perform(SavedState::load(id), move |s| {
                        Message::TabBodyLoaded { id, saved: s }
                    });
                }
            }
            iced::Task::none()
        }
        Message::RequestTabLoad(index) => match app.tabs.get(index) {
            Some(TabLoadState::Unloaded(meta)) => {
                let id = meta.id;
                app.tabs[index] = TabLoadState::Loading(meta.clone());
                iced::Task::perform(SavedState::load(id), move |s| Message::TabBodyLoaded {
                    id,
                    saved: s,
                })
            }
            _ => iced::Task::none(),
        },
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
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            match req_type {
                RequestType::HTTP | RequestType::GraphQL => {
                    tab.request_type = req_type;
                }
                RequestType::WebSocket => {
                    tab.request_type = req_type;
                    tab.ws_connected = false;
                    tab.ws_input = String::new();
                    tab.ws_auto_scroll = true;
                    tab.url = "wss://echo.websocket.org".to_string();
                }
                _ => {
                    tab.response_body_content =
                        text_editor::Content::with_text("Ops! Sorry. Not implemented yet!");
                }
            }
            iced::Task::none()
        }
        Message::MethodSelected(method) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.method = method;
            iced::Task::none()
        }
        Message::UrlChanged(url) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.url = url;
            app.parse_url_query();
            iced::Task::none()
        }
        Message::SendRequest => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if !tab.loading && !tab.url.trim().is_empty() {
                tab.loading = true;
                app.send_request()
            } else {
                iced::Task::none()
            }
        }
        Message::BodyAction(action) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.body_content.perform(action);
            iced::Task::none()
        }
        Message::AuthTypeSelected(auth_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.auth_type = auth_type;
            iced::Task::none()
        }
        Message::BearerTokenChanged(token) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.bearer_token = token;
            iced::Task::none()
        }
        Message::ContentTypeSelected(content_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.content_type = content_type;
            iced::Task::none()
        }
        Message::CancelRequest => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.cancel_flag.store(true, Ordering::Relaxed);
            tab.loading = false;
            tab.response_body_content =
                text_editor::Content::with_text("Request cancelled by user");
            tab.response_status = "Cancelled".to_string();
            iced::Task::none()
        }
        Message::SaveBinaryResponse => {
            let Some(tab) = app.current_tab() else {
                return iced::Task::none();
            };
            if !tab.is_response_binary {
                return iced::Task::none();
            }

            let file_name = tab.response_filename.clone();
            let response_bytes = tab.response_bytes.clone();

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
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            match result {
                Ok(filename) => {
                    tab.response_body_content = text_editor::Content::with_text(&format!(
                        "File saved successfully: {}",
                        filename
                    ))
                }
                Err(error) => {
                    tab.response_body_content =
                        text_editor::Content::with_text(&format!("Error saving file: {}", error))
                }
            }
            iced::Task::none()
        }
        Message::ClearResponseText => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.response_body_content = text_editor::Content::with_text("");
            tab.response_headers_content = text_editor::Content::with_text("");
            iced::Task::none()
        }
        Message::GraphqlQueryAction(action) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            // text_editor::Content::perform handles the editing logic
            tab.graphql_query.perform(action);
            iced::Task::none()
        }

        Message::GraphqlVariablesAction(action) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_variables.perform(action);
            iced::Task::none()
        }

        Message::GraphqlOperationChanged(val) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_operation = val;
            iced::Task::none()
        }

        Message::FetchGraphqlSchema => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            let url = tab.url.clone();
            if url.is_empty() {
                return iced::Task::none();
            }

            tab.graphql_schema_loading = true;
            tab.graphql_schema_error = None;

            iced::Task::perform(
                async move {
                    let body = serde_json::json!({ "query": INTROSPECTION_QUERY });
                    let resp = HTTP_CLIENT
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(body.to_string())
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    let text = resp.text().await.map_err(|e| e.to_string())?;

                    // This function should now return your new GraphqlSchema struct
                    parse_graphql_schema(&text)
                },
                Message::GraphqlSchemaFetched,
            )
        }

        Message::GraphqlSchemaFetched(result) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_schema_loading = false;
            match result {
                Ok(schema) => {
                    tab.graphql_schema = Some(schema);
                    tab.graphql_schema_error = None;
                    // Optional: Auto-expand the root query type
                    // tab.graphql_expanded_types.insert(schema.query_type.name.clone());
                }
                Err(e) => {
                    tab.graphql_schema_error = Some(e);
                }
            }
            iced::Task::none()
        }

        Message::GraphqlFieldToggled(path) | Message::GraphqlArgToggled(path) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            let selected = &mut tab.graphql_selected_paths;
            let manual = &mut tab.manually_selected_paths;

            if selected.contains(&path) {
                // --- TOGGLE OFF ---
                // 4. Recursive uncheck (Downwards)
                let child_prefix = format!("{}.", path);
                selected.retain(|p| p != &path && !p.starts_with(&child_prefix));
                manual.retain(|p| p != &path && !p.starts_with(&child_prefix));

                // 3. Walk backwards (Upwards)
                let mut parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
                while parts.len() > 1 {
                    parts.pop();
                    let parent_path = parts.join(".");

                    let sibling_prefix = format!("{}.", parent_path);
                    let has_active_children =
                        selected.iter().any(|p| p.starts_with(&sibling_prefix));

                    if !has_active_children && !manual.contains(&parent_path) {
                        selected.remove(&parent_path);
                    } else {
                        break;
                    }
                }
            } else {
                // --- TOGGLE ON ---
                manual.insert(path.clone()); // Track this as a manual click

                // 1. Bubbling check (Upwards)
                let parts: Vec<&str> = path.split('.').collect();
                let mut current_path = String::new();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        current_path.push('.');
                    }
                    current_path.push_str(part);
                    selected.insert(current_path.clone());
                }
            }
            let query = app.build_query();
            if let Some(tab) = app.current_tab_mut() {
                tab.graphql_query = text_editor::Content::with_text(&query);
            }
            iced::Task::none()
        }

        Message::GraphqlTypeToggled(name) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            let expanded = &mut tab.graphql_expanded_types;
            if !expanded.remove(&name) {
                expanded.insert(name);
            }
            iced::Task::none()
        }

        Message::GraphqlSearchChanged(val) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_search = val;
            iced::Task::none()
        }
        Message::GraphqlCollapseAll => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            // Clear the set to collapse everything
            tab.graphql_expanded_types.clear();
            iced::Task::none()
        }
        Message::CookieJarOpen => {
            app.cookie_jar_open = true;
            iced::Task::none()
        }
        Message::CookieJarClose => {
            for cookies in app.cookie_jar.values_mut() {
                cookies.retain(|c| !c.name.trim().is_empty());
            }
            app.cookie_jar.retain(|_, cookies| !cookies.is_empty());
            app.cookie_jar_open = false;
            iced::Task::none()
        }
        Message::CookieJarAdd(domain) => {
            if domain.trim().is_empty() {
                app.cookie_jar_error = Some("Domain cannot be empty".to_string());
            } else {
                app.cookie_jar.entry(domain).or_default().push(CookieEntry {
                    name: String::new(),
                    value: String::new(),
                    enabled: true,
                    domain: String::new(),
                    path: "/".to_string(),
                    expires: None,
                });
                app.cookie_jar_new_domain = String::new();
                app.cookie_jar_error = None;
            }
            iced::Task::none()
        }
        Message::CookieJarRemove(domain, idx) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if idx < cookies.len() {
                    cookies.remove(idx);
                }
                if cookies.is_empty() {
                    app.cookie_jar.remove(&domain);
                }
            }
            iced::Task::none()
        }
        Message::CookieJarToggled(domain, idx) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if let Some(c) = cookies.get_mut(idx) {
                    c.enabled = !c.enabled;
                }
            }
            iced::Task::none()
        }
        Message::CookieJarNameChanged(domain, idx, val) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if let Some(c) = cookies.get_mut(idx) {
                    c.name = val;
                }
            }
            iced::Task::none()
        }
        Message::CookieJarValueChanged(domain, idx, val) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if let Some(c) = cookies.get_mut(idx) {
                    c.value = val;
                }
            }
            iced::Task::none()
        }
        Message::CookieJarDomainChanged(val) => {
            app.cookie_jar_new_domain = val;
            app.cookie_jar_error = None;
            iced::Task::none()
        }
        Message::CookieJarClearDomain(domain) => {
            app.cookie_jar.remove(&domain);
            iced::Task::none()
        }
        Message::Tick => {
            app.svg_rotation = (app.svg_rotation + 4.0) % 360.0;
            iced::Task::none()
        }
        Message::TogglePause => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_paused(!vp.paused());
                }
            }
            iced::Task::none()
        }
        Message::ToggleLoop => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_looping(!vp.looping());
                }
            }
            iced::Task::none()
        }
        Message::VideoVolume(vol) => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_volume(vol);
                }
            }
            iced::Task::none()
        }
        Message::Seek(secs) => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vs) = tab.video_state.as_mut() {
                    vs.dragging = true;
                    vs.position = secs;
                }
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_paused(true);
                }
            }
            iced::Task::none()
        }
        Message::SeekRelease => {
            if let Some(tab) = app.current_tab_mut() {
                if let (Some(vs), Some(vp)) = (tab.video_state.as_mut(), tab.video_player.as_mut())
                {
                    vs.dragging = false;
                    vp.seek(std::time::Duration::from_secs_f64(vs.position), false)
                        .expect("seek");
                    vp.set_paused(false);
                }
            }
            iced::Task::none()
        }
        Message::EndOfStream => {
            println!("end of stream");
            iced::Task::none()
        }
        Message::NewFrame => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let (Some(vs), Some(vp)) = (tab.video_state.as_mut(), tab.video_player.as_mut()) {
                if !vs.dragging {
                    vs.position = vp.position().as_secs_f64();
                }
            }
            iced::Task::none()
        }
        Message::ResponseReceived(resp) => {
            let url = {
                let Some(tab) = app.current_tab_mut() else {
                    return iced::Task::none();
                };

                tab.loading = false;
                tab.is_streaming = true;
                tab.active_response_tab = ResponseTab::Body;
                tab.is_response_binary = resp.is_binary;
                tab.response_status = resp.status;
                tab.response_content_type = resp.content_type.clone();
                tab.response_time = resp.response_time;

                let url = tab.url.clone();

                if resp.is_binary && resp.content_type.starts_with("video/") && resp.accepts_range {
                    let parsed_url = url::Url::parse(&url).unwrap();
                    match iced_video_player::Video::new(&parsed_url) {
                        Ok(video) => {
                            tab.video_player = Some(video);
                            tab.video_state = Some(VideoState {
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
                            tab.video_player = None;
                        }
                    }
                } else if resp.is_binary && resp.content_type.starts_with("image/") {
                    tab.image_handle =
                        Some(iced::widget::image::Handle::from_bytes(resp.bytes.clone()));
                } else {
                    tab.video_player = None;
                    tab.video_state = None;
                    tab.response_headers_content = text_editor::Content::with_text(&resp.headers);
                    tab.response_body_content = text_editor::Content::with_text(&resp.body);
                }

                url
            };

            if let Some(domain) = extract_domain(&url) {
                for raw in &resp.set_cookies {
                    if let Some(cookie) = parse_set_cookie(raw) {
                        let jar = app.cookie_jar.entry(domain.clone()).or_default();
                        if let Some(existing) = jar.iter_mut().find(|c| c.name == cookie.name) {
                            *existing = cookie;
                        } else {
                            jar.push(cookie);
                        }
                    }
                }
            }

            iced::Task::none()
        }
        Message::ResponseBodyAction(action) => {
            if let Some(tab) = app.current_tab_mut() {
                match action {
                    text_editor::Action::Edit(_) => {}
                    _ => tab.response_body_content.perform(action),
                }
            }
            iced::Task::none()
        }
        Message::ResponseHeadersAction(action) => {
            if let Some(tab) = app.current_tab_mut() {
                match action {
                    text_editor::Action::Edit(_) => {}
                    _ => tab.response_headers_content.perform(action),
                }
            }
            iced::Task::none()
        }
        Message::ResponseTabSelected(response_tab) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.active_response_tab = response_tab;
            iced::Task::none()
        }
        Message::PrettifyJson => {
            let Some(tab) = app.current_tab() else {
                return iced::Task::none();
            };
            let body_text = tab.body_content.text();

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
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

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
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if tab.is_response_binary {
                return iced::Task::none();
            }
            let text = match tab.active_response_tab {
                ResponseTab::Body => tab.response_body_content.text(),
                ResponseTab::Headers => tab.response_headers_content.text(),
            };
            tab.copied = true;
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
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.copied = false;
            iced::Task::none()
        }
        Message::QueryParamAdd => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.query_params.push(QueryParam::new());
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamRemove(idx) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if idx < tab.query_params.len() {
                tab.query_params.remove(idx);
            }
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamKeyChanged(idx, key) => {
            app.update_query(idx, |p| p.key = key);
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamValueChanged(idx, value) => {
            app.update_query(idx, |p| p.value = value);
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamToggled(idx) => {
            app.update_query(idx, |p| p.enabled = !p.enabled);
            app.rebuild_url();
            iced::Task::none()
        }
        Message::FormFieldKeyChanged(index, key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            if let Some(field) = tab.form_data.get_mut(index) {
                field.key = key;
            }
            iced::Task::none()
        }
        Message::FormFieldValueChanged(index, value) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(index) {
                field.value = value;
            }
            iced::Task::none()
        }
        Message::FormFieldTypeSelected(idx, form_field_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(idx) {
                field.field_type = form_field_type;
                field.value.clear();
                field.files.clear();
            }
            iced::Task::none()
        }
        Message::FormFieldToggled(idx) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(idx) {
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
            let Some(tab) = app.current_tab() else {
                return iced::Task::none();
            };
            let state = tab.to_saved(&app.json_theme.to_string(), &app.app_theme.to_string());

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
            if let Some(slot) = app.tabs.get_mut(app.active_tab) {
                *slot = TabLoadState::Loaded(Box::new(TabState::from_saved(saved_state)));
            }
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
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(index) {
                field.files = files;
            }
            iced::Task::none()
        }
        Message::FormFieldRemove(index) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if index < tab.form_data.len() {
                tab.form_data.remove(index);
            }
            iced::Task::none()
        }
        Message::FormFieldAdd => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.form_data.push(FormField::new());
            iced::Task::none()
        }
        Message::ViewRawForm => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            if matches!(
                tab.content_type,
                ContentType::FormData | ContentType::XWWWFormUrlEncoded
            ) {
                let raw = TabState::form_data_to_raw(&tab.form_data);
                tab.raw_form_content = text_editor::Content::with_text(&raw);
                tab.form_view_type = FormViewType::Raw;
            }

            iced::Task::none()
        }
        Message::ViewFormattedForm => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            if matches!(
                tab.content_type,
                ContentType::FormData | ContentType::XWWWFormUrlEncoded
            ) {
                let raw = tab.raw_form_content.text();

                tab.form_data = TabState::raw_to_form_data(&raw);
                tab.form_view_type = FormViewType::Formatted;
            }

            iced::Task::none()
        }
        Message::FormRawAction(action) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.raw_form_content.perform(action);

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
                    iced::window::Event::CloseRequested => {
                        // Save first, then close
                        return app.save_task().chain(iced::exit());
                    }
                    iced::window::Event::Unfocused => {
                        println!("window was unfocused");
                        return iced::Task::none();
                    }
                    _ => {}
                }
            }
            if let Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Left | iced::mouse::Button::Right,
            )) = &event
            {
                if let Some(editing_id) = app.sidebar_editing_id {
                    return iced::widget::operation::is_focused("sidebar_rename").then(
                        move |focused| {
                            if !focused {
                                iced::Task::done(Message::CollectionItemRenameConfirm(editing_id))
                            } else {
                                iced::Task::none()
                            }
                        },
                    );
                }
            }
            if let Event::Keyboard(key_event) = event {
                let Some(tab) = app.current_tab() else {
                    return iced::Task::none();
                };
                match key_event {
                    KeyEvent::KeyPressed { key, modifiers, .. } if modifiers.control() => {
                        if let Key::Character(c) = &key {
                            if c.as_str() == "l" {
                                return iced::widget::operation::focus(tab.url_id.clone()).chain(
                                    iced::widget::operation::select_all(tab.url_id.clone()),
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
                            iced::widget::operation::is_focused(tab.url_id.clone()).then(|f| {
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
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Escape),
                        ..
                    } => {
                        if app.sidebar_editing_id.is_some() {
                            app.sidebar_editing_id = None;
                            app.sidebar_editing_name = String::new();
                            return iced::Task::none();
                        }
                        if app.save_modal_open {
                            app.save_modal_open = false;
                            app.save_modal_name = String::new();
                            app.save_modal_folder_id = None;
                            return iced::Task::none();
                        }
                    }
                    _ => {}
                }
            }
            iced::Task::none()
        }
        Message::RequestTabSelected(request_tab) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.active_request_tab = request_tab;
            iced::Task::none()
        }
        Message::ApiKeyPositionChanged(position) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.api_key_position = position;
            iced::Task::none()
        }
        Message::ApiKeyNameChanged(key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.api_key_name = key;
            iced::Task::none()
        }
        Message::ApiKeyChanged(key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.api_key = key;
            iced::Task::none()
        }
        Message::HeaderAdd => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.headers.push(RequestHeaders::new());
            iced::Task::none()
        }
        Message::HeaderRemove(id) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if id < tab.headers.len() {
                tab.headers.remove(id);
            }
            iced::Task::none()
        }
        Message::HeaderKeyChanged(id, key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            if let Some(header) = tab.headers.get_mut(id) {
                header.key = key;
            }
            iced::Task::none()
        }
        Message::HeaderValueChanged(id, value) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(header) = tab.headers.get_mut(id) {
                header.value = value;
            }
            iced::Task::none()
        }
        Message::HeaderToggled(id) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(header) = tab.headers.get_mut(id) {
                header.enabled = !header.enabled;
            }
            iced::Task::none()
        }
        Message::StreamChunk(chunk) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.stream_buffer.push_str(&chunk);
            // Re-build the editor content from the full buffer each chunk.
            // For very large responses you can throttle this, but it's fine for typical APIs.
            let current = tab.stream_buffer.clone();
            tab.response_body_content = text_editor::Content::with_text(&current);
            iced::Task::none()
        }
        Message::StreamDone => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.is_streaming = false;
            tab.loading = false;
            // Optionally pretty-print JSON now that we have the full body
            let body = tab.stream_buffer.clone();
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Ok(pretty) = serde_json::to_string_pretty(&j) {
                    tab.response_body_content = text_editor::Content::with_text(&pretty);
                }
            }

            iced::Task::none()
        }
        Message::WsConnect => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            let url = tab.url.clone();

            // Validate URL
            if !url.starts_with("ws://") && !url.starts_with("wss://") {
                app.add_ws_system_message("Error: WebSocket URL must start with ws:// or wss://");
                return iced::Task::none();
            }

            tab.loading = true;
            tab.ws_connection_id += 1;
            app.add_ws_system_message(&format!("Connecting to {}...", url));

            iced::Task::none()
        }
        Message::WsDisconnect => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.ws_connection_id = 0;
            tab.ws_connection = None;
            tab.ws_connected = false;
            tab.loading = false;
            app.add_ws_system_message("Disconnected");
            iced::Task::none()
        }
        Message::WsEvent(event) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            match event {
                WsEvent::Connected(connection) => {
                    tab.loading = false;
                    tab.ws_connected = true;
                    tab.ws_connection = Some(connection);
                    app.add_ws_system_message("Connected successfully");
                }
                WsEvent::Disconnected(reason) => {
                    tab.ws_connection = None;
                    tab.loading = false;
                    tab.ws_connected = false;
                    app.add_ws_system_message(&format!("Disconnected: {}", reason));
                }
                WsEvent::MessageReceived(content) => {
                    app.add_ws_received_message(&content);
                }
                WsEvent::Error(error) => {
                    tab.loading = false;
                    app.add_ws_system_message(&format!("Error: {}", error));
                }
            }
            iced::Task::none()
        }
        Message::WsMessageInputChanged(text) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.ws_input = text;
            iced::Task::none()
        }
        Message::WsSendMessage => {
            // 1. Isolate the mutable borrow of 'app' (via 'tab') in a block
            let message = {
                let Some(tab) = app.current_tab_mut() else {
                    return iced::Task::none();
                };

                let msg = tab.ws_input.clone();
                if msg.is_empty() {
                    return iced::Task::none();
                }

                tab.ws_input.clear();

                // If you need to send the message, do it while you have the tab
                if let Some(conn) = tab.ws_connection.as_mut() {
                    let _ = conn.send(msg.clone());
                }

                msg // Return the msg so we can use it outside the block
            }; // <--- 'tab' goes out of scope here, 'app' is no longer borrowed

            // 2. Now 'app' is free to be borrowed again here
            app.add_ws_sent_message(&message);

            iced::Task::none()
        }
        Message::WsClearMessages => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.ws_messages_content = text_editor::Content::new();
            tab.ws_count_sent = 0;
            tab.ws_count_received = 0;

            iced::Task::none()
        }
        Message::WsToggleAutoScroll => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_auto_scroll = !tab.ws_auto_scroll;
            iced::Task::none()
        }
        Message::WsMessageEditorAction(action) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_messages_content.perform(action);
            iced::Task::none()
        }
        Message::WsMessageTypeSelected(msg_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_message_type = msg_type;
            iced::Task::none()
        }
        Message::WsBinaryMessageTypeSelected(msg_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_binary_message_type = msg_type;
            iced::Task::none()
        }
        Message::StateLoaded(maybe_session, metadata) => {
            if let Some(session) = maybe_session {
                app.json_theme = json_theme_from_str(&session.json_theme);
                app.app_theme = theme_from_str(&session.app_theme);
                app.next_tab_id = session.next_tab_id;
                app.cookie_jar = session.cookie_jar;
                app.active_tab = session.active_tab.min(metadata.len().saturating_sub(1));
            }

            // build tab list — all Unloaded first
            app.tabs = metadata
                .into_iter()
                .map(|m| TabLoadState::Unloaded(m))
                .collect();

            // no saved tabs — create a fresh one
            if app.tabs.is_empty() {
                app.tabs
                    .push(TabLoadState::Loaded(Box::new(TabState::new(0))));
                app.next_tab_id = 1;
                app.active_tab = 0;
                return iced::Task::none();
            }

            // load active tab immediately
            let active = app.active_tab;
            if let Some(TabLoadState::Unloaded(meta)) = app.tabs.get(active) {
                let id = meta.id;
                app.tabs[active] = TabLoadState::Loading(meta.clone());
                return iced::Task::perform(SavedState::load(id), move |s| {
                    Message::TabBodyLoaded { id, saved: s }
                });
            }
            iced::Task::none()
        }
        Message::SaveComplete => {
            println!("SaveComplete received");
            iced::Task::none()
        }
        Message::CollectionLoaded(maybe_collection) => {
            if let Some(collection) = maybe_collection {
                app.collection = collection;
            }
            iced::Task::none()
        }
        Message::ToggleSidebar => {
            app.sidebar_open = !app.sidebar_open;
            iced::Task::none()
        }
        Message::SidebarItemSelected(id) => {
            app.sidebar_selected_id = Some(id);
            iced::Task::none()
        }
        Message::CollectionFolderAdd(parent_id) => {
            let id = app.next_collection_id();
            let folder = CollectionItem::Folder(CollectionFolder {
                id,
                name: "New Folder".to_string(),
                expanded: true,
                children: Vec::new(),
            });
            CrabiPie::collection_insert_into(&mut app.collection.items, parent_id, folder);
            // immediately enter rename mode
            app.sidebar_editing_id = Some(id);
            app.sidebar_editing_name = "New Folder".to_string();
            app.collection_save_task()
        }
        Message::CollectionItemToggleExpand(id) => {
            CrabiPie::collection_toggle_expand(&mut app.collection.items, id);
            iced::Task::none()
        }
        Message::CollectionRequestOpen(id) => {
            app.sidebar_selected_id = Some(id);
            if let Some(req) = CrabiPie::collection_find_request(&app.collection.items, id) {
                let saved = req.saved_state.clone();
                let new_tab = TabLoadState::Loaded(Box::new(TabState::from_saved(saved)));
                app.tabs.push(new_tab);
                app.active_tab = app.tabs.len() - 1;
                app.next_tab_id += 1;
            }
            iced::Task::none()
        }
        Message::CollectionItemRename(id) => {
            // find current name
            fn find_name(items: &[CollectionItem], id: usize) -> Option<String> {
                for item in items {
                    if item.id() == id {
                        return Some(item.name().to_string());
                    }
                    if let CollectionItem::Folder(f) = item {
                        if let Some(n) = find_name(&f.children, id) {
                            return Some(n);
                        }
                    }
                }
                None
            }
            if let Some(name) = find_name(&app.collection.items, id) {
                app.sidebar_editing_id = Some(id);
                app.sidebar_editing_name = name;
            }
            iced::Task::none()
        }
        Message::CollectionItemRenameInput(text) => {
            app.sidebar_editing_name = text;
            iced::Task::none()
        }
        Message::CollectionItemRenameConfirm(id) => {
            let new_name = app.sidebar_editing_name.trim().to_string();
            if !new_name.is_empty() {
                CrabiPie::collection_rename_item(&mut app.collection.items, id, new_name);
            }
            app.sidebar_editing_id = None;
            app.sidebar_editing_name = String::new();
            app.collection_save_task()
        }
        Message::CollectionItemRenameCancel => {
            app.sidebar_editing_id = None;
            app.sidebar_editing_name = String::new();
            iced::Task::none()
        }
        Message::CollectionItemDelete(id) => {
            CrabiPie::collection_remove_item(&mut app.collection.items, id);
            app.collection_save_task()
        }
        Message::CollectionItemDuplicate(id) => {
            let new_id = app.next_collection_id();
            CrabiPie::collection_duplicate_item(&mut app.collection.items, id, new_id);
            app.collection_save_task()
        }
        Message::OpenSaveModal => {
            // Scope the borrow so it ends immediately after we get the title
            let title = {
                let Some(tab) = app.current_tab() else {
                    return iced::Task::none();
                };
                tab.title.clone()
            }; // The borrow of 'app' ends right here because 'tab' is gone

            // Now 'app' is free to be modified mutably
            app.save_modal_open = true;
            app.save_modal_name = title;
            app.save_modal_folder_id = None;

            iced::Task::none()
        }
        Message::SaveModalNameChanged(name) => {
            app.save_modal_name = name;
            iced::Task::none()
        }
        Message::SaveModalFolderSelected(folder_id) => {
            app.save_modal_folder_id = folder_id;
            iced::Task::none()
        }
        Message::SaveModalConfirm => {
            // 1. Get the data from the tab first (Immutable borrow phase)
            let (method, saved_state) = {
                let Some(tab) = app.current_tab() else {
                    return iced::Task::none();
                };
                (
                    tab.method.clone(),
                    tab.to_saved(&app.json_theme.to_string(), &app.app_theme.to_string()),
                )
            }; // The immutable borrow of 'app' ends here

            // 2. Now 'app' is free! You can now borrow it mutably.
            let id = app.next_collection_id(); // Works now!

            let req = CollectionItem::Request(CollectionRequest {
                id,
                name: app.save_modal_name.trim().to_string(),
                method,
                saved_state,
            });

            let folder_id = app.save_modal_folder_id;

            // Perform the insertion
            CrabiPie::collection_insert_into(&mut app.collection.items, folder_id, req);

            // 3. Cleanup
            app.save_modal_open = false;
            app.save_modal_name = String::new();
            app.save_modal_folder_id = None;

            app.collection_save_task()
        }
        Message::SaveModalCancel => {
            app.save_modal_open = false;
            app.save_modal_name = String::new();
            app.save_modal_folder_id = None;
            iced::Task::none()
        }
        Message::CollectionSaved => iced::Task::none(),
    }
}

fn view(app: &CrabiPie) -> Element<'_, Message> {
    let Some(tab) = app.current_tab() else {
        return iced::widget::text("Loading...").into();
    };
    let main_content = column![
        app.render_title_row(),
        app.render_tabs(),
        rule::horizontal(1.0),
        if tab.request_type == RequestType::WebSocket {
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
            .height(Length::Fill);
        iced::widget::stack![main_content, overlay].into()
    } else {
        main_content.into()
    };

    // Wrap with sidebar if open
    let body: Element<'_, Message> = if app.sidebar_open {
        row![app.render_sidebar(), rule::vertical(1.0), main_content,]
            .height(Length::Fill)
            .into()
    } else {
        main_content
    };

    // Save modal overlay
    let body: Element<'_, Message> = if app.save_modal_open {
        let overlay = container(app.render_save_modal())
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(
                    0.0, 0.0, 0.0, 0.5,
                ))),
                ..Default::default()
            });
        iced::widget::stack![body, overlay].into()
    } else {
        body
    };

    let body: Element<'_, Message> = if app.cookie_jar_open {
        iced::widget::stack![body, app.render_cookie_jar_modal()].into()
    } else {
        body
    };

    container(body).padding(10).height(Length::Fill).into()
}

fn main() -> iced::Result {
    let icon_bytes = include_bytes!("../CrabiPie.ico");
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
        .exit_on_close_request(false)
        .run()
}

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
enum FormViewType {
    Raw,
    Formatted,
}

impl std::fmt::Display for FormViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "View {:?}", self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
enum FormFieldType {
    Text,
    File,
}

impl std::fmt::Display for FormFieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
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

#[derive(Debug, Clone, Default)]
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
    set_cookies: Vec<String>,
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
                value: "CrabiPie".to_string(),
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
    GraphQL,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
enum WsMessageType {
    Text,
    Json,
    Binary,
    HTML,
}

impl std::fmt::Display for WsMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl WsMessageType {
    const ALL: [WsMessageType; 4] = [
        WsMessageType::Text,
        WsMessageType::Json,
        WsMessageType::Binary,
        WsMessageType::HTML,
    ];
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
enum WsBinaryMessageType {
    Base64,
    HexaDecimal,
}

impl std::fmt::Display for WsBinaryMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl WsBinaryMessageType {
    const ALL: [WsBinaryMessageType; 2] = [
        WsBinaryMessageType::Base64,
        WsBinaryMessageType::HexaDecimal,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppPersistedState {
    tabs: Vec<SavedState>,
    active_tab: usize,
    json_theme: String,
    app_theme: String,
    next_tab_id: usize,
    cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
}

impl AppPersistedState {
    async fn migrate_if_needed() {
        let old_path = state_dir().join("session.json");
        // if tabs key exists in old json → split into per-file format → delete old
    }
}

fn state_file_path() -> std::path::PathBuf {
    state_dir().join("session.json")
}

fn state_dir() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = base.join(".crabipie");
    std::fs::create_dir_all(&dir).ok();
    dir
}

async fn save_app_state(state: AppPersistedState) {
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        tokio::fs::write(state_file_path(), json).await.ok();
    }
}

async fn load_app_state() -> (Option<SessionState>, Vec<TabMetadata>) {
    let session = SessionState::load().await;
    let metadata = TabMetadata::load_all().await;
    (session, metadata)
}
fn theme_from_str(s: &str) -> iced::Theme {
    match s {
        "Light" => iced::Theme::Light,
        "Dark" => iced::Theme::Dark,
        "Dracula" => iced::Theme::Dracula,
        "Nord" => iced::Theme::Nord,
        "Solarized Light" => iced::Theme::SolarizedLight,
        "Solarized Dark" => iced::Theme::SolarizedDark,
        "Gruvbox Light" => iced::Theme::GruvboxLight,
        "Gruvbox Dark" => iced::Theme::GruvboxDark,
        "Catppuccin Latte" => iced::Theme::CatppuccinLatte,
        "Catppuccin Frappe" => iced::Theme::CatppuccinFrappe,
        "Catppuccin Macchiato" => iced::Theme::CatppuccinMacchiato,
        "Catppuccin Mocha" => iced::Theme::CatppuccinMocha,
        "Tokyo Night" => iced::Theme::TokyoNight,
        "Tokyo Night Storm" => iced::Theme::TokyoNightStorm,
        "Tokyo Night Light" => iced::Theme::TokyoNightLight,
        "Kanagawa Wave" => iced::Theme::KanagawaWave,
        "Kanagawa Dragon" => iced::Theme::KanagawaDragon,
        "Kanagawa Lotus" => iced::Theme::KanagawaLotus,
        "Moonfly" => iced::Theme::Moonfly,
        "Nightfly" => iced::Theme::Nightfly,
        "Oxocarbon" => iced::Theme::Oxocarbon,
        "Ferra" => iced::Theme::Ferra,
        _ => iced::Theme::CatppuccinMocha, // default fallback
    }
}

fn json_theme_from_str(s: &str) -> json_highlighter::JsonThemeWrapper {
    match s {
        "Base 16 Eighties" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Eighties)
        }
        "Base 16 Mocha" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Mocha)
        }
        "Base 16 Ocean" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::Base16Ocean)
        }
        "Solarized Dark" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::SolarizedDark)
        }
        "Inspired GitHub" => {
            json_highlighter::JsonThemeWrapper::Builtin(iced::highlighter::Theme::InspiredGitHub)
        }
        "Default Dark" => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::DEFAULT_DARK,
        ),
        "Default Light" => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::DEFAULT_LIGHT,
        ),
        "VSCode Dark" => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::VSCODE_DARK,
        ),
        _ => json_highlighter::JsonThemeWrapper::Custom(
            json_highlighter::CustomJsonTheme::VSCODE_DARK,
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Collection {
    name: String,
    items: Vec<CollectionItem>,
}

impl Collection {
    fn new() -> Self {
        Self {
            name: "My Collection".to_string(),
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CollectionItem {
    Folder(CollectionFolder),
    Request(CollectionRequest),
}

impl CollectionItem {
    fn id(&self) -> usize {
        match self {
            CollectionItem::Folder(f) => f.id,
            CollectionItem::Request(r) => r.id,
        }
    }

    fn name(&self) -> &str {
        match self {
            CollectionItem::Folder(f) => &f.name,
            CollectionItem::Request(r) => &r.name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub enabled: bool,
    pub domain: String,
    pub path: String,
    pub expires: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CollectionFolder {
    id: usize,
    name: String,
    expanded: bool,
    children: Vec<CollectionItem>, // enables infinite nesting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CollectionRequest {
    id: usize,
    name: String,
    method: HttpMethod,
    saved_state: SavedState,
}

fn collection_file_path() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = base.join(".crabipie");
    std::fs::create_dir_all(&dir).ok();
    dir.join("collection.json")
}

async fn save_collection(collection: Collection) {
    if let Ok(json) = serde_json::to_string_pretty(&collection) {
        tokio::fs::write(collection_file_path(), json).await.ok();
    }
}

async fn load_collection() -> Option<Collection> {
    let bytes = tokio::fs::read(collection_file_path()).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

fn parse_set_cookie(raw: &str) -> Option<CookieEntry> {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IntrospectionResponse {
    data: SchemaWrapper,
}

#[derive(Debug, Deserialize)]
struct SchemaWrapper {
    #[serde(rename = "__schema")]
    schema: GraphqlSchema,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GraphqlSchema {
    query_type: TypeRef,
    types: Vec<GraphqlType>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TypeRef {
    name: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlType {
    name: Option<String>,
    kind: String,
    fields: Option<Vec<GraphqlField>>,
    input_fields: Option<Vec<GraphqlField>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GraphqlField {
    name: String,
    // The "type" field is an object, not a string
    #[serde(rename = "type")]
    field_type: TypeDetail,

    #[serde(default)]
    args: Vec<GraphqlArg>,

    #[serde(default)]
    is_selected: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GraphqlArg {
    name: String,
    #[serde(rename = "type")]
    arg_type: TypeDetail,

    #[serde(default)] // Default to false when parsing JSON
    pub is_selected: bool,
}

impl std::fmt::Display for GraphqlArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.arg_type)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TypeDetail {
    name: Option<String>,
    kind: String,
    // This allows for [String!]! logic
    of_type: Option<Box<TypeDetail>>,
}

impl TypeDetail {
    /// Recursively finds the actual name of the type,
    /// bypassing NON_NULL and LIST wrappers.
    pub fn get_base_name(&self) -> Option<&String> {
        if let Some(ref name) = self.name {
            Some(name)
        } else if let Some(ref inner) = self.of_type {
            inner.get_base_name()
        } else {
            None
        }
    }
}

impl std::fmt::Display for TypeDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind.as_str() {
            "NON_NULL" => {
                if let Some(inner) = &self.of_type {
                    write!(f, "{}!", inner)
                } else {
                    write!(f, "Unknown!")
                }
            }
            "LIST" => {
                if let Some(inner) = &self.of_type {
                    write!(f, "[{}]", inner)
                } else {
                    write!(f, "[Unknown]")
                }
            }
            // For SCALAR, OBJECT, etc., just print the name
            _ => write!(f, "{}", self.name.as_deref().unwrap_or("Unknown")),
        }
    }
}

fn parse_graphql_schema(json: &str) -> Result<GraphqlSchema, String> {
    let wrapper: IntrospectionResponse =
        serde_json::from_str(json).map_err(|e| format!("JSON Deserialization Error: {}", e))?;

    let mut schema = wrapper.data.schema;

    schema.types.retain(|t| {
        t.name
            .as_ref()
            .map(|name| !name.starts_with("__"))
            .unwrap_or(true)
    });

    Ok(schema)
}

fn render_schema_tree<'a>(
    types: &'a [GraphqlType],
    current_type: &'a GraphqlType,
    expanded: &std::collections::HashSet<String>,
    selected_paths: &std::collections::HashSet<String>,
    search: &str,
    depth: u16,
    path: &str,
) -> Vec<Element<'a, Message>> {
    let mut rows: Vec<Element<'a, Message>> = vec![];

    let fields_to_render = current_type
        .fields
        .as_ref()
        .or(current_type.input_fields.as_ref());

    let Some(fields) = fields_to_render else {
        return rows;
    };

    let current_type_name = current_type.name.as_deref().unwrap_or("Unknown");

    let indent_width = depth as f32 * 16.0;

    for field in fields {
        // Search filter
        if !search.is_empty() && !field.name.to_lowercase().contains(&search.to_lowercase()) {
            continue;
        }

        let base_type_name = field
            .field_type
            .get_base_name()
            .map(|s| s.as_str())
            .unwrap_or("Unknown");

        let field_path = if path.is_empty() {
            field.name.clone()
        } else {
            format!("{}.{}", path, field.name)
        };

        // Check expandability
        let target_type = types
            .iter()
            .find(|t| t.name.as_deref() == Some(base_type_name));

        let has_type_children = target_type.map_or(false, |t| {
            t.fields.as_ref().map_or(false, |f| !f.is_empty())
                || t.input_fields.as_ref().map_or(false, |f| !f.is_empty())
        });

        let has_args = !field.args.is_empty();
        let is_expandable = has_type_children || has_args;
        let field_path_clone = field_path.clone();
        let is_expanded = expanded.contains(&field_path);

        // UI Components
        let expander_width = 20.0;

        let expand_btn: Element<'a, Message> = if is_expandable {
            container(
                button(text(if is_expanded { "▼" } else { "▶" }).size(10))
                    .style(button::text)
                    .padding(0)
                    .on_press(Message::GraphqlTypeToggled(field_path_clone.clone())),
            )
            .width(expander_width)
            .center_x(expander_width)
            .into()
        } else {
            Space::new().width(expander_width).into()
        };

        let type_label =
            text(format!("{}", field.field_type))
                .size(13)
                .style(|theme: &iced::Theme| text::Style {
                    color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5)), // Grey out the types
                });

        let is_selected = selected_paths.contains(&field_path);
        let field_path_cloned = field_path.clone();
        let field_row = row![
            Space::new().width(indent_width),
            expand_btn,
            checkbox(is_selected)
                .on_toggle(move |_| Message::GraphqlFieldToggled(field_path_cloned.clone())),
            text(field.name.clone()).size(14),
            type_label,
        ]
        .spacing(4)
        .padding(Padding {
            top: 2.0,
            ..Default::default()
        })
        .align_y(Alignment::Center);

        rows.push(field_row.into());

        // Only recurse if expanded
        if is_expanded {
            // A. Render Arguments AND their nested fields
            for arg in &field.args {
                let arg_path = format!("{}.{}", field_path, arg.name);
                let arg_path_cloned = arg_path.clone();
                let is_arg_selected = selected_paths.contains(&arg_path);
                let is_arg_expanded = expanded.contains(&arg_path);

                // Find if the argument type has sub-fields (is it an Input Object?)
                let arg_base_name = arg
                    .arg_type
                    .get_base_name()
                    .map(|s| s.as_str())
                    .unwrap_or("");

                let arg_target_type = types
                    .iter()
                    .find(|t| t.name.as_deref() == Some(arg_base_name));

                // An argument is expandable if it has input_fields
                let is_arg_expandable = arg_target_type.map_or(false, |t| t.input_fields.is_some());

                // --- Render the Argument Row ---
                let arg_expander: Element<'a, Message> = if is_arg_expandable {
                    button(text(if is_arg_expanded { "▼" } else { "▶" }).size(10))
                        .style(button::text)
                        .padding(0.0)
                        .on_press(Message::GraphqlTypeToggled(arg_path.clone()))
                        .into()
                } else {
                    Space::new().width(20.0).into()
                };

                let t_name = current_type_name.to_string();
                let f_name = field.name.clone();
                let a_name = arg.name.clone();
                // --- Render the Argument Row ---
                let arg_row = row![
                    Space::new().width(indent_width + 16.0),
                    container(arg_expander).width(20.0).center_x(20.0),
                    checkbox(is_arg_selected)
                        .on_toggle(move |_| Message::GraphqlArgToggled(arg_path_cloned.clone())),
                    text(format!("arg: {}", arg.name))
                        .size(13)
                        .style(|_| text::Style {
                            color: Some(iced::Color::from_rgb(0.9, 0.5, 0.1))
                        }),
                    text(format!("({})", arg.arg_type))
                        .size(14)
                        .style(|_| text::Style {
                            color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5))
                        }),
                ]
                .spacing(4)
                .padding(Padding {
                    top: 2.0,
                    ..Default::default()
                })
                .align_y(Alignment::Center);

                rows.push(arg_row.into());

                // --- Recurse into Argument Children (the 'code', 'eq' part) ---
                if is_arg_expanded && is_arg_expandable {
                    if let Some(arg_type_obj) = arg_target_type {
                        let arg_children = render_schema_tree(
                            types,
                            arg_type_obj,
                            expanded,
                            selected_paths,
                            search,
                            depth + 2, // Move children further right
                            &arg_path,
                        );
                        rows.extend(arg_children);
                    }
                }
            }

            // B. Recurse into child fields (Return Types)
            if let Some(child_type_obj) = target_type {
                let children = render_schema_tree(
                    types,
                    child_type_obj,
                    expanded,
                    selected_paths,
                    search,
                    depth + 1,
                    &field_path,
                );
                rows.extend(children);
            }
        }
    }

    rows
}

fn generate_selection_set(
    all_types: &[GraphqlType],
    current_type: &GraphqlType,
    selected_paths: &std::collections::HashSet<String>,
    indent: usize,
    current_path: &str,
) -> String {
    let mut output = String::new();
    let spaces = "  ".repeat(indent);

    let fields = current_type
        .fields
        .as_ref()
        .or(current_type.input_fields.as_ref());

    if let Some(fields) = fields {
        for field in fields {
            let field_path = format!("{}.{}", current_path, field.name);

            // ONLY proceed if this field is in the HashSet
            if selected_paths.contains(&field_path) {
                output.push_str(&format!("{}{}", spaces, field.name));

                // --- 1. Handle Arguments ---
                let active_args: Vec<_> = field
                    .args
                    .iter()
                    .filter(|a| selected_paths.contains(&format!("{}.arg:{}", field_path, a.name)))
                    .collect();

                if !active_args.is_empty() {
                    output.push('(');
                    let arg_strings: Vec<String> = active_args
                        .iter()
                        .map(|a| format!("{}: TODO_VAL", a.name)) // We'll talk about values next!
                        .collect();
                    output.push_str(&arg_strings.join(", "));
                    output.push(')');
                }

                // --- 2. Handle Sub-selection (Nested Objects) ---
                let base_name = field.field_type.get_base_name();
                let sub_type = all_types
                    .iter()
                    .find(|t| t.name.as_deref() == base_name.map(|s| s.as_str()));

                // Check if any children of this field are selected
                let has_selected_children = selected_paths
                    .iter()
                    .any(|p| p.starts_with(&format!("{}.", field_path)) && !p.contains(".arg:"));

                if let Some(st) = sub_type {
                    if has_selected_children {
                        output.push_str(" {\n");
                        output.push_str(&generate_selection_set(
                            all_types,
                            st,
                            selected_paths,
                            indent + 1,
                            &field_path,
                        ));
                        output.push_str(&format!("{}}}\n", spaces));
                    } else {
                        output.push('\n');
                    }
                } else {
                    output.push('\n');
                }
            }
        }
    }
    output
}

fn proximity_order(active: usize, len: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..len).collect();
    indices.sort_by_key(|&i| i.abs_diff(active));
    indices
}

const BODY_DEFAULT: &str = r#"{
  "title": "foo",
  "body": "bar",
  "userId": 1,
  "foo": "bar"
}"#;

const RAW_FORM_PLACEHOLDER: &str = r#"Rows are separated by newline.
Keys and values are separated by :
Prepend # to the rows that you want to add but keep it disabled.
"#;

const INTROSPECTION_QUERY: &str = r#"
{
  __schema {
    queryType { name }
    types {
      name
      kind
      fields(includeDeprecated: false) {
        name
        type {
          name
          kind
          ofType { name kind ofType { name kind } }
        }
        args {
          name
          type { name kind ofType { name kind } }
        }
      }
      inputFields {
        name
        type {
          name
          kind
          ofType { name kind ofType { name kind } }
        }
      }
    }
  }
}
"#;

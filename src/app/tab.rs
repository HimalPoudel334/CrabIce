//! Per-tab state: everything one request tab owns (request config, response, editors, media).

use iced::widget::text_editor;

use crate::domain::auth::{ApiKeyPosition, AuthType};
use crate::domain::graphql::GraphqlSchema;
use crate::domain::http::{
    BODY_DEFAULT,
    ContentType,
    FormField,
    FormFieldType,
    HttpMethod,
    QueryParam,
    RAW_FORM_PLACEHOLDER,
    RequestHeaders,
    RequestType,
};
use crate::domain::persistence::{SavedState, TabMetadata};
use crate::domain::view_state::{FormViewType, HeaderViewType, RequestTab, ResponseTab};
use crate::domain::websocket::{WsBinaryMessageType, WsConnection, WsMessageType};
use crate::domain::pdf_viewer;

pub(crate) struct TabState {
    pub(crate) metadata: TabMetadata,
    pub(crate) id: usize,
    pub(crate) title: String,

    // Request configuration
    pub(crate) url_id: iced::widget::Id,
    pub(crate) url: String,
    pub(crate) request_type: RequestType,
    pub(crate) method: HttpMethod,
    pub(crate) headers: Vec<RequestHeaders>,
    pub(crate) header_view_type: HeaderViewType,
    pub(crate) raw_headers_content: std::sync::Arc<str>,
    pub(crate) request_body: std::sync::Arc<str>,
    pub(crate) form_view_type: FormViewType,
    pub(crate) auth_type: AuthType,
    pub(crate) bearer_token: String,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) api_key_name: String,
    pub(crate) api_key: String,
    pub(crate) api_key_position: ApiKeyPosition,
    pub(crate) content_type: ContentType,
    pub(crate) query_params: Vec<QueryParam>,
    pub(crate) form_data: Vec<FormField>,
    pub(crate) raw_form_content: std::sync::Arc<str>,
    pub(crate) cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    // GraphQL Editors ---
    pub(crate) graphql_query: std::sync::Arc<str>,
    pub(crate) graphql_variables: std::sync::Arc<str>,
    pub(crate) graphql_operation: String,
    pub(crate) graphql_schema: Option<GraphqlSchema>,
    pub(crate) graphql_schema_loading: bool,
    pub(crate) graphql_schema_error: Option<String>,
    pub(crate) graphql_expanded_types: std::collections::HashSet<String>,
    pub(crate) graphql_search: String,
    pub(crate) graphql_selected_paths: std::collections::HashSet<String>,
    pub(crate) manually_selected_paths: std::collections::HashSet<String>,

    // response stream
    pub(crate) stream_buffer: String,
    pub(crate) is_streaming: bool,

    // WebSocket-specific fields
    pub(crate) ws_connected: bool,
    pub(crate) ws_connection: Option<WsConnection>,
    pub(crate) ws_auto_scroll: bool,
    pub(crate) ws_input: String,
    pub(crate) ws_messages_content: text_editor::Content,
    pub(crate) ws_count_sent: usize,
    pub(crate) ws_count_received: usize,
    pub(crate) ws_message_type: WsMessageType,
    pub(crate) ws_binary_message_type: WsBinaryMessageType,

    //image handle
    pub(crate) image_handle: Option<iced::widget::image::Handle>,

    // Pdf response
    pub(crate) pdf_viewer: Option<pdf_viewer::PdfViewer>,
    pub(crate) pdf_current_image: Option<iced::advanced::image::Handle>,

    // Video response
    pub(crate) video_player: Option<iced_video_player::Video>,
    pub(crate) video_state: Option<VideoState>,

    // Response data
    pub(crate) response_status: String,
    pub(crate) response_body: std::sync::Arc<str>,
    pub(crate) response_headers: std::sync::Arc<str>,
    pub(crate) is_response_binary: bool,
    pub(crate) response_filename: String,
    pub(crate) response_bytes: Vec<u8>,
    pub(crate) response_content_type: String,
    pub(crate) response_time: Option<std::time::Duration>,

    // Tab-specific UI state
    pub(crate) loading: bool,
    pub(crate) active_request_tab: RequestTab,
    pub(crate) active_response_tab: ResponseTab,
    pub(crate) copied: bool,
    pub(crate) ws_connection_id: usize,
    pub(crate) dirty: bool,

    // Which environment this tab resolves {{vars}} against. Environments
    // themselves are still global/shared; only the *selection* is per-tab.
    pub(crate) active_environment_id: Option<usize>,
    // Guards against retrying an auth-refresh-triggered resend more than
    // once per manual send, so a still-401ing token can't loop forever.
    pub(crate) auth_retry_used: bool,
}

impl TabState {
    pub(crate) fn new(id: usize, default_environment_id: Option<usize>) -> Self {
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
            header_view_type: HeaderViewType::Formatted,
            raw_headers_content: std::sync::Arc::from(RAW_FORM_PLACEHOLDER),
            request_body: std::sync::Arc::from(BODY_DEFAULT),
            form_view_type: FormViewType::Formatted,
            auth_type: AuthType::None,
            bearer_token: String::new(),
            api_key_name: String::new(),
            username: String::new(),
            password: String::new(),
            api_key: String::new(),
            api_key_position: ApiKeyPosition::Header,
            content_type: ContentType::Json,
            query_params: vec![QueryParam::new()],
            form_data: vec![FormField::new()],
            raw_form_content: std::sync::Arc::from(""),
            image_handle: None,
            pdf_viewer: None,
            pdf_current_image: None,
            video_player: None,
            video_state: None,
            cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            response_status: String::new(),
            response_headers: std::sync::Arc::from(""),
            response_body: std::sync::Arc::from(""),
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
            graphql_query: std::sync::Arc::from(""),
            graphql_variables: std::sync::Arc::from("{}"),
            graphql_operation: String::new(),
            graphql_schema: None,
            graphql_schema_loading: false,
            graphql_schema_error: None,
            graphql_expanded_types: std::collections::HashSet::new(),
            graphql_search: String::new(),
            graphql_selected_paths: std::collections::HashSet::new(),
            manually_selected_paths: std::collections::HashSet::new(),
            dirty: false,
            active_environment_id: default_environment_id,
            auth_retry_used: false,
        }
    }

    pub(crate) fn from_saved(saved: SavedState) -> Self {
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
            header_view_type: saved.header_view_type,
            raw_headers_content: std::sync::Arc::from(saved.raw_headers_content.as_str()),
            request_body: std::sync::Arc::from(saved.body.as_str()),
            form_view_type: saved.form_view_type,
            auth_type: saved.auth_type,
            bearer_token: saved.bearer_token,
            username: saved.username,
            password: saved.password,
            api_key_name: saved.api_key_name,
            api_key: saved.api_key,
            api_key_position: saved.api_key_position,
            content_type: saved.content_type,
            query_params: saved.query_params,
            form_data: saved.form_data,
            raw_form_content: std::sync::Arc::from(saved.raw_form_content.as_str()),
            image_handle: None,
            video_player: None,
            video_state: None,
            cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            response_status: saved.response_status.unwrap_or_default(),
            response_headers: std::sync::Arc::from(
                saved.response_headers.unwrap_or_default().as_str(),
            ),

            response_body: std::sync::Arc::from(saved.response_body.unwrap_or_default().as_str()),
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
            graphql_query: std::sync::Arc::from(saved.graphql_query.as_str()),
            graphql_variables: std::sync::Arc::from(saved.graphql_variables.as_str()),
            graphql_operation: saved.graphql_operation,
            graphql_schema: saved.graphql_schema,
            graphql_schema_loading: false,
            graphql_schema_error: saved.graphql_schema_error,
            graphql_expanded_types: saved.graphql_expanded_types,
            graphql_search: String::new(),
            graphql_selected_paths: saved.graphql_selected_paths,
            manually_selected_paths: saved.manually_selected_paths,
            dirty: false,
            pdf_viewer: None,
            pdf_current_image: None,
            active_environment_id: saved.active_environment_id,
            auth_retry_used: false,
        }
    }

    pub(crate) fn to_saved(&self, json_theme: &str, app_theme: &str) -> SavedState {
        SavedState {
            id: self.id,
            title: self.title.clone(),
            url: self.url.clone(),
            request_type: self.request_type.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
            header_view_type: self.header_view_type,
            raw_headers_content: self.raw_headers_content.to_string(),
            body: self.request_body.to_string(),
            form_view_type: self.form_view_type,
            auth_type: self.auth_type.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            bearer_token: self.bearer_token.clone(),
            api_key_name: self.api_key_name.clone(),
            api_key: self.api_key.clone(),
            api_key_position: self.api_key_position,
            content_type: self.content_type.clone(),
            query_params: self.query_params.clone(),
            form_data: self.form_data.clone(),
            raw_form_content: self.raw_form_content.to_string(),
            json_theme: json_theme.to_string(),
            app_theme: app_theme.to_string(),
            response_status: if self.response_status.is_empty() {
                None
            } else {
                Some(self.response_status.clone())
            },
            response_headers: Some(self.response_body.to_string()),
            response_body: Some(self.response_body.to_string()),
            ws_message_type: self.ws_message_type,
            ws_binary_message_type: self.ws_binary_message_type,
            graphql_query: self.graphql_query.to_string(),
            graphql_variables: self.graphql_variables.to_string(),
            graphql_operation: self.graphql_operation.clone(),
            graphql_schema: self.graphql_schema.clone(),
            graphql_schema_error: self.graphql_schema_error.clone(),
            graphql_expanded_types: self.graphql_expanded_types.clone(),
            graphql_selected_paths: self.graphql_selected_paths.clone(),
            manually_selected_paths: self.manually_selected_paths.clone(),

            active_request_tab: self.active_request_tab,
            active_response_tab: self.active_response_tab,
            active_environment_id: self.active_environment_id,
        }
    }

    pub(crate) fn headers_to_raw(headers: &[RequestHeaders]) -> String {
        headers
            .iter()
            .filter_map(|h| {
                if h.key.is_empty() && h.value.is_empty() {
                    None
                } else {
                    let mut line = format!("{}: {}", h.key, h.value);

                    if !h.enabled {
                        line = format!("# {}", line);
                    }

                    Some(line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn raw_to_headers(raw: &str) -> Vec<RequestHeaders> {
        raw.lines()
            .filter_map(|line| {
                let line = line.trim();

                if line.is_empty() {
                    return None;
                }

                let (enabled, line) = if let Some(rest) = line.strip_prefix('#') {
                    (false, rest.trim())
                } else {
                    (true, line)
                };

                let (key, value) = line.split_once(':')?;

                Some(RequestHeaders {
                    enabled,
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                })
            })
            .collect()
    }

    pub(crate) fn form_data_to_raw(form_data: &[FormField]) -> String {
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

    pub(crate) fn raw_to_form_data(raw: &str) -> Vec<FormField> {
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

pub(crate) enum TabLoadState {
    Unloaded(TabMetadata),
    Loading(TabMetadata), // background task running
    Loaded(Box<TabState>),
}

impl TabLoadState {
    pub(crate) fn id(&self) -> usize {
        self.metadata().id
    }

    pub(crate) fn metadata(&self) -> &TabMetadata {
        match self {
            TabLoadState::Unloaded(m) => m,
            TabLoadState::Loading(m) => m,
            TabLoadState::Loaded(s) => &s.metadata,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VideoState {
    pub(crate) playing: bool,
    pub(crate) position: f64,
    pub(crate) duration: f64,
    pub(crate) dragging: bool,
    pub(crate) volume: f64,
    pub(crate) buffering: bool,
}

pub(crate) fn proximity_order(active: usize, len: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..len).collect();
    indices.sort_by_key(|&i| i.abs_diff(active));
    indices
}

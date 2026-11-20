//! On-disk state: session, tab metadata, per-tab saved state.

use serde::{Deserialize, Serialize};

use crate::domain::auth::{ApiKeyPosition, AuthType};
use crate::domain::cookies::CookieEntry;
use crate::domain::environment::Environment;
use crate::domain::graphql::GraphqlSchema;
use crate::domain::http::{
    ContentType,
    FormField,
    HttpMethod,
    QueryParam,
    RequestHeaders,
    RequestType,
};
use crate::domain::view_state::{FormViewType, HeaderViewType, RequestTab, ResponseTab};
use crate::domain::websocket::{WsBinaryMessageType, WsMessageType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TabMetadata {
    pub(crate) id: usize,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) method: HttpMethod,
    pub(crate) request_type: RequestType,
}

impl TabMetadata {
    pub(crate) async fn save_metadata(tabs: Vec<TabMetadata>) {
        let path = state_dir().join("tabs/metadata.json");
        if let Ok(json) = serde_json::to_string(&tabs) {
            tokio::fs::write(path, json).await.ok();
        }
    }

    pub(crate) async fn load_all() -> Vec<TabMetadata> {
        let path = state_dir().join("tabs/metadata.json");
        let bytes = tokio::fs::read(&path).await;
        let result: Vec<TabMetadata> = bytes
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        println!(
            "metadata ids: {:?}",
            result.iter().map(|m| m.id).collect::<Vec<_>>()
        );
        result
    }

    pub(crate) async fn save_all(tabs: &[TabMetadata]) {
        let path = state_dir().join("tabs/metadata.json");
        if let Ok(json) = serde_json::to_vec(tabs) {
            tokio::fs::write(path, json).await.ok();
        }
    }
}

// Minimal session file (no tabs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionState {
    pub(crate) active_tab: usize,
    pub(crate) json_theme: String,
    pub(crate) app_theme: String,
    pub(crate) next_tab_id: usize,
    pub(crate) cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
    #[serde(default)]
    pub(crate) environments: Vec<Environment>,
    #[serde(default)]
    pub(crate) active_environment_id: Option<usize>,
    #[serde(default)]
    pub(crate) next_environment_id: usize,
}

impl SessionState {
    pub(crate) async fn load() -> Option<SessionState> {
        let bytes = tokio::fs::read(state_file_path()).await.ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub(crate) async fn save(&self) {
        if let Ok(json) = serde_json::to_vec(self) {
            tokio::fs::write(state_file_path(), json).await.ok();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedState {
    pub(crate) id: usize,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) request_type: RequestType,
    pub(crate) method: HttpMethod,
    pub(crate) headers: Vec<RequestHeaders>,
    pub(crate) header_view_type: HeaderViewType,
    pub(crate) raw_headers_content: String,
    pub(crate) body: String,
    pub(crate) form_view_type: FormViewType,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) auth_type: AuthType,
    pub(crate) bearer_token: String,
    pub(crate) api_key_name: String,
    pub(crate) api_key: String,
    pub(crate) api_key_position: ApiKeyPosition,
    pub(crate) content_type: ContentType,
    pub(crate) query_params: Vec<QueryParam>,
    pub(crate) form_data: Vec<FormField>,
    pub(crate) raw_form_content: String,
    pub(crate) json_theme: String,
    pub(crate) app_theme: String,

    pub(crate) ws_message_type: WsMessageType,
    pub(crate) ws_binary_message_type: WsBinaryMessageType,

    // Response (only when NOT binary)
    pub(crate) response_status: Option<String>,
    pub(crate) response_headers: Option<String>,
    pub(crate) response_body: Option<String>,

    // GraphQL
    pub(crate) graphql_query: String,
    pub(crate) graphql_variables: String,
    pub(crate) graphql_operation: String,
    pub(crate) graphql_schema: Option<GraphqlSchema>,
    pub(crate) graphql_schema_error: Option<String>,
    pub(crate) graphql_expanded_types: std::collections::HashSet<String>,
    pub(crate) graphql_selected_paths: std::collections::HashSet<String>,
    pub(crate) manually_selected_paths: std::collections::HashSet<String>,

    pub(crate) active_request_tab: RequestTab,
    pub(crate) active_response_tab: ResponseTab,

    #[serde(default)]
    pub(crate) active_environment_id: Option<usize>,
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
            header_view_type: HeaderViewType::Formatted,
            raw_headers_content: String::new(),
            body: String::new(),
            form_view_type: FormViewType::Formatted,
            auth_type: AuthType::None,
            username: String::new(),
            password: String::new(),
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
            active_environment_id: None,
        }
    }
}

impl SavedState {
    pub(crate) async fn save(&self) {
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

    pub(crate) async fn load(id: usize) -> Option<SavedState> {
        let path = state_dir().join(format!("tabs/{}.bin", id));
        let compressed = tokio::fs::read(path).await.ok()?;
        let mut decoder = flate2::read::GzDecoder::new(compressed.as_slice());
        let mut json = Vec::new();
        use std::io::Read;
        decoder.read_to_end(&mut json).ok()?;
        serde_json::from_slice::<SavedState>(&json).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AppPersistedState {
    pub(crate) tabs: Vec<SavedState>,
    pub(crate) active_tab: usize,
    pub(crate) json_theme: String,
    pub(crate) app_theme: String,
    pub(crate) next_tab_id: usize,
    pub(crate) cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
}

impl AppPersistedState {
    pub(crate) async fn migrate_if_needed() {
        let old_path = state_dir().join("session.json");
        // if tabs key exists in old json → split into per-file format → delete old
    }
}

pub(crate) fn state_file_path() -> std::path::PathBuf {
    state_dir().join("session.json")
}

pub(crate) fn state_dir() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = base.join(".crabipie");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub(crate) async fn save_app_state(state: AppPersistedState) {
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        tokio::fs::write(state_file_path(), json).await.ok();
    }
}

pub(crate) async fn load_app_state() -> (Option<SessionState>, Vec<TabMetadata>) {
    let session = SessionState::load().await;
    let metadata = TabMetadata::load_all().await;
    (session, metadata)
}

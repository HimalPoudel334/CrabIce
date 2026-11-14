use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPersistedState {
    pub tabs: Vec<SavedState>,
    pub active_tab: usize,
    pub json_theme: String,
    pub app_theme: String,
    pub next_tab_id: usize,
    pub cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
}

impl AppPersistedState {
    pub async fn migrate_if_needed() {
        let _old_path = state_dir().join("session.json");
        // if tabs key exists in old json → split into per-file format → delete old
    }
}

pub fn state_file_path() -> std::path::PathBuf {
    state_dir().join("session.json")
}

pub fn state_dir() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let dir = base.join(".crabipie");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub async fn save_app_state(state: AppPersistedState) {
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        tokio::fs::write(state_file_path(), json).await.ok();
    }
}

pub async fn load_app_state() -> (Option<SessionState>, Vec<TabMetadata>) {
    let session = SessionState::load().await;
    let metadata = TabMetadata::load_all().await;
    (session, metadata)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabMetadata {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub method: HttpMethod,
    pub request_type: RequestType,
}
impl TabMetadata {
    pub async fn save_metadata(tabs: Vec<TabMetadata>) {
        let path = state_dir().join("tabs/metadata.json");
        if let Ok(json) = serde_json::to_string(&tabs) {
            tokio::fs::write(path, json).await.ok();
        }
    }

    pub async fn load_all() -> Vec<TabMetadata> {
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

    pub async fn save_all(tabs: &[TabMetadata]) {
        let path = state_dir().join("tabs/metadata.json");
        if let Ok(json) = serde_json::to_vec(tabs) {
            tokio::fs::write(path, json).await.ok();
        }
    }
}

// Minimal session file (no tabs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub active_tab: usize,
    pub json_theme: String,
    pub app_theme: String,
    pub next_tab_id: usize,
    pub cookie_jar: std::collections::HashMap<String, Vec<CookieEntry>>,
    #[serde(default)]
    pub environments: Vec<Environment>,
    #[serde(default)]
    pub active_environment_id: Option<usize>,
    #[serde(default)]
    pub next_environment_id: usize,
}

impl SessionState {
    pub async fn load() -> Option<SessionState> {
        let bytes = tokio::fs::read(state_file_path()).await.ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub async fn save(&self) {
        if let Ok(json) = serde_json::to_vec(self) {
            tokio::fs::write(state_file_path(), json).await.ok();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub request_type: RequestType,
    pub method: HttpMethod,
    pub headers: Vec<RequestHeaders>,
    pub header_view_type: HeaderViewType,
    pub raw_headers_content: String,
    pub body: String,
    pub form_view_type: FormViewType,
    pub username: String,
    pub password: String,
    pub auth_type: AuthType,
    pub bearer_token: String,
    pub api_key_name: String,
    pub api_key: String,
    pub api_key_position: ApiKeyPosition,
    pub content_type: ContentType,
    pub query_params: Vec<QueryParam>,
    pub form_data: Vec<FormField>,
    pub raw_form_content: String,
    pub json_theme: String,
    pub app_theme: String,

    pub ws_message_type: WsMessageType,
    pub ws_binary_message_type: WsBinaryMessageType,

    // Response (only when NOT binary)
    pub response_status: Option<String>,
    pub response_headers: Option<String>,
    pub response_body: Option<String>,

    // GraphQL
    pub graphql_query: String,
    pub graphql_variables: String,
    pub graphql_operation: String,
    pub graphql_schema: Option<GraphqlSchema>,
    pub graphql_schema_error: Option<String>,
    pub graphql_expanded_types: std::collections::HashSet<String>,
    pub graphql_selected_paths: std::collections::HashSet<String>,
    pub manually_selected_paths: std::collections::HashSet<String>,

    pub active_request_tab: RequestTab,
    pub active_response_tab: ResponseTab,

    #[serde(default)]
    pub active_environment_id: Option<usize>,
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
    pub async fn save(&self) {
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

    pub async fn load(id: usize) -> Option<SavedState> {
        let path = state_dir().join(format!("tabs/{}.bin", id));
        let compressed = tokio::fs::read(path).await.ok()?;
        let mut decoder = flate2::read::GzDecoder::new(compressed.as_slice());
        let mut json = Vec::new();
        use std::io::Read;
        decoder.read_to_end(&mut json).ok()?;
        serde_json::from_slice::<SavedState>(&json).ok()
    }
}


/// Panel sizes of the native shell. Kept in its own file so the iced app's
/// session.json is never touched by it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiLayout {
    pub sidebar_open: bool,
    pub sidebar_width: f32,
    pub split: f32,
    pub gql_tree_width: f32,
    pub gql_query_frac: f32,
}

impl Default for UiLayout {
    fn default() -> Self {
        Self { sidebar_open: false, sidebar_width: 240.0, split: 0.5, gql_tree_width: 250.0, gql_query_frac: 0.6 }
    }
}

impl UiLayout {
    pub fn path() -> std::path::PathBuf {
        state_dir().join("native_ui.json")
    }

    pub fn load() -> Option<Self> {
        serde_json::from_slice(&std::fs::read(Self::path()).ok()?).ok()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            std::fs::write(Self::path(), json).ok();
        }
    }
}

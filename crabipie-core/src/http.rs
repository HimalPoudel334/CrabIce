use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct HttpResponse {
    pub status: String,
    pub headers: String,
    pub accepts_range: bool,
    pub is_streaming: bool,
    pub body: String,
    pub is_binary: bool,
    pub filename: String,
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub response_time: Option<tokio::time::Duration>,
    pub set_cookies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VideoState {
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub dragging: bool,
    pub volume: f64,
    pub buffering: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParam {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl QueryParam {
    pub fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestHeaders {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl RequestHeaders {
    pub fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }

    pub fn default() -> Vec<RequestHeaders> {
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

pub static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("failed to build http client")
});

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RequestTab {
    Body,
    Headers,
    Auth,
    Query,
    GraphQL,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResponseTab {
    Body,
    Headers,
}

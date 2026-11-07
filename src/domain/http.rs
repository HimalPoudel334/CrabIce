//! HTTP request/response data types and the shared client.

use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum RequestType {
    HTTP,
    WebSocket,
    GraphQL,
    GRPC,
}

impl RequestType {
    pub(crate) const ALL: [RequestType; 4] = [
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
pub(crate) enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    QUERY,
}

impl HttpMethod {
    pub(crate) const ALL: [HttpMethod; 8] = [
        HttpMethod::GET,
        HttpMethod::POST,
        HttpMethod::PUT,
        HttpMethod::DELETE,
        HttpMethod::PATCH,
        HttpMethod::HEAD,
        HttpMethod::OPTIONS,
        HttpMethod::QUERY,
    ];
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ContentType {
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
    pub(crate) const ALL: [ContentType; 3] = [
        ContentType::Json,
        ContentType::FormData,
        ContentType::XWWWFormUrlEncoded,
    ];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum FormFieldType {
    Text,
    File,
}

impl std::fmt::Display for FormFieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FormFieldType {
    pub(crate) const ALL: [FormFieldType; 2] = [FormFieldType::Text, FormFieldType::File];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FormField {
    pub(crate) enabled: bool,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) files: Vec<String>,
    pub(crate) field_type: FormFieldType,
}

impl FormField {
    pub(crate) fn new() -> Self {
        Self {
            enabled: true,
            key: String::new(),
            value: String::new(),
            files: Vec::new(),
            field_type: FormFieldType::Text,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HttpResponse {
    pub(crate) status: String,
    pub(crate) headers: String,
    pub(crate) accepts_range: bool,
    pub(crate) is_streaming: bool,
    pub(crate) body: String,
    pub(crate) is_binary: bool,
    pub(crate) filename: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) content_type: String,
    pub(crate) response_time: Option<tokio::time::Duration>,
    pub(crate) set_cookies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QueryParam {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) enabled: bool,
}

impl QueryParam {
    pub(crate) fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestHeaders {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) enabled: bool,
}

impl RequestHeaders {
    pub(crate) fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }

    pub(crate) fn default() -> Vec<RequestHeaders> {
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

pub(crate) static HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
    reqwest::Client::builder()
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("failed to build http client")
});

pub(crate) const BODY_DEFAULT: &str = r#"{
  "title": "foo",
  "body": "bar",
  "userId": 1,
  "foo": "bar"
}"#;

pub(crate) const RAW_FORM_PLACEHOLDER: &str = r#"Rows are separated by newline.
Keys and values are separated by :
Prepend # to the rows that you want to add but keep it disabled.
"#;

pub(crate) fn encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        out.push_str(&urlencoding::encode(&rest[..start]));
        match rest[start..].find("}}") {
            Some(rel_end) => {
                let end = start + rel_end + 2;
                out.push_str(&rest[start..end]);
                rest = &rest[end..];
            }
            None => {
                // Unterminated "{{" — treat the remainder as literal text.
                out.push_str(&urlencoding::encode(&rest[start..]));
                rest = "";
            }
        }
    }
    out.push_str(&urlencoding::encode(rest));
    out
}

// Threshold for large responses (e.g., 5 MB)
pub(crate) const LARGE_RESPONSE_THRESHOLD: u64 = 5 * 1024 * 1024;

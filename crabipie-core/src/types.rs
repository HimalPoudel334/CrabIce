use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum RequestType {
    HTTP,
    WebSocket,
    GraphQL,
    GRPC,
}

impl RequestType {
    pub const ALL: [RequestType; 4] = [
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
pub enum HttpMethod {
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
    pub const ALL: [HttpMethod; 8] = [
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
pub enum ContentType {
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
    pub const ALL: [ContentType; 3] = [
        ContentType::Json,
        ContentType::FormData,
        ContentType::XWWWFormUrlEncoded,
    ];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum FormViewType {
    Raw,
    Formatted,
}

impl std::fmt::Display for FormViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "View {:?}", self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum HeaderViewType {
    Formatted,
    Raw,
}

impl std::fmt::Display for HeaderViewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Edit {:?}", self)
    }
}

impl HeaderViewType {
    pub const ALL: [HeaderViewType; 2] = [HeaderViewType::Formatted, HeaderViewType::Raw];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum FormFieldType {
    Text,
    File,
}

impl std::fmt::Display for FormFieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FormFieldType {
    pub const ALL: [FormFieldType; 2] = [FormFieldType::Text, FormFieldType::File];
}

#[derive(Debug, Clone, Copy)]
pub enum JsonTarget {
    RequestBody,
    ResponseBody,
    ResponseHeaders,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub enabled: bool,
    pub key: String,
    pub value: String,
    pub files: Vec<String>,
    pub field_type: FormFieldType,
}

impl FormField {
    pub fn new() -> Self {
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
pub enum AuthType {
    None,
    Basic,
    Bearer,
    ApiKey,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "No Auth"),
            AuthType::Basic => write!(f, "Basic Auth"),
            AuthType::Bearer => write!(f, "Bearer Token"),
            AuthType::ApiKey => write!(f, "Api Key"),
        }
    }
}

impl AuthType {
    pub const ALL: [AuthType; 4] = [
        AuthType::None,
        AuthType::Basic,
        AuthType::Bearer,
        AuthType::ApiKey,
    ];
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum ApiKeyPosition {
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
    pub const ALL: [ApiKeyPosition; 2] = [ApiKeyPosition::Header, ApiKeyPosition::QueryParams];
}

// ─────────────────────────────────────────────────────────────────────────
// Auth auto-refresh (login/refresh-token automation)
// ─────────────────────────────────────────────────────────────────────────

/// How an environment renews its access token automatically.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthRefreshMode {
    /// No auto-refresh configured; bearer/basic fields behave exactly as before.
    #[default]
    None,
    /// No dedicated refresh endpoint exists — "refreshing" means calling the
    /// login endpoint again with the stored username/password.
    ReLogin,
    /// A proper OAuth2-style flow: login returns an access token *and* a
    /// refresh token; renewal calls a separate refresh endpoint with the
    /// refresh token instead of resending the password.
    RefreshToken,
}

impl std::fmt::Display for AuthRefreshMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthRefreshMode::None => write!(f, "Off"),
            AuthRefreshMode::ReLogin => write!(f, "Re-login (username/password)"),
            AuthRefreshMode::RefreshToken => write!(f, "Refresh token"),
        }
    }
}

impl AuthRefreshMode {
    pub const ALL: [AuthRefreshMode; 3] = [
        AuthRefreshMode::None,
        AuthRefreshMode::ReLogin,
        AuthRefreshMode::RefreshToken,
    ];
}

/// HTTP method used for the login / refresh request.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthHttpMethod {
    #[default]
    Post,
    Put,
    Patch,
    /// Only valid with form-urlencoded; fields are sent as the query string.
    Get,
}

impl AuthHttpMethod {
    pub const ALL: [AuthHttpMethod; 4] = [
        AuthHttpMethod::Post,
        AuthHttpMethod::Put,
        AuthHttpMethod::Patch,
        AuthHttpMethod::Get,
    ];

    pub fn to_reqwest(self) -> reqwest::Method {
        match self {
            AuthHttpMethod::Post => reqwest::Method::POST,
            AuthHttpMethod::Put => reqwest::Method::PUT,
            AuthHttpMethod::Patch => reqwest::Method::PATCH,
            AuthHttpMethod::Get => reqwest::Method::GET,
        }
    }
}

impl std::fmt::Display for AuthHttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthHttpMethod::Post => write!(f, "POST"),
            AuthHttpMethod::Put => write!(f, "PUT"),
            AuthHttpMethod::Patch => write!(f, "PATCH"),
            AuthHttpMethod::Get => write!(f, "GET"),
        }
    }
}

/// Body encoding for the login / refresh request.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthBodyFormat {
    #[default]
    Json,
    FormUrlEncoded,
}

impl AuthBodyFormat {
    pub const ALL: [AuthBodyFormat; 2] = [AuthBodyFormat::Json, AuthBodyFormat::FormUrlEncoded];
}

impl std::fmt::Display for AuthBodyFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthBodyFormat::Json => write!(f, "JSON"),
            AuthBodyFormat::FormUrlEncoded => write!(f, "x-www-form-urlencoded"),
        }
    }
}

/// Which of the two auth requests a message/helper refers to.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthRequestKind {
    Login,
    Refresh,
}

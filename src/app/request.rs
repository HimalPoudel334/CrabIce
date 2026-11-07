//! Request pipeline: URL/query sync, building the `reqwest` request, sending it (incl. streaming and auth-refresh retry).

use std::sync::atomic::Ordering;
use iced::futures::SinkExt;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::auth::{ApiKeyPosition, AuthRefreshMode, AuthType, perform_auth_refresh};
use crate::domain::cookies::extract_domain;
use crate::domain::http::{
    ContentType,
    FormFieldType,
    HTTP_CLIENT,
    HttpMethod,
    HttpResponse,
    QueryParam,
    RequestType,
    encode_query_component,
};

impl CrabiPie {
    pub(crate) fn build_query_string(&self) -> String {
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

    pub(crate) fn update_query<F: FnOnce(&mut QueryParam)>(&mut self, idx: usize, f: F) {
        if let Some(tab) = self.current_tab_mut() {
            if let Some(param) = tab.query_params.get_mut(idx) {
                f(param);
            }
        }
        self.rebuild_url();
    }

    pub(crate) fn rebuild_url(&mut self) {
        let Some(tab) = self.current_tab() else {
            return;
        };

        let current = tab.url.clone();
        let base = current.split('?').next().unwrap_or(&current);

        let mut query = String::new();
        for p in &tab.query_params {
            if p.enabled && !p.key.is_empty() {
                if !query.is_empty() {
                    query.push('&');
                }
                query.push_str(&encode_query_component(&p.key));
                query.push('=');
                query.push_str(&encode_query_component(&p.value));
            }
        }

        let new_url = if query.is_empty() {
            base.to_string()
        } else {
            format!("{}?{}", base, query)
        };

        if let Some(tab) = self.current_tab_mut() {
            tab.url = new_url;
        }
    }

    pub(crate) fn parse_url_query(&mut self) {
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

    pub(crate) fn build_request(&self) -> Option<(reqwest::RequestBuilder, String)> {
        let tab = self.current_tab()?;

        let mut url = self.resolve_vars(&tab.url);
        if !matches!(url.split_once("://"), Some(("http" | "https", _))) {
            url = format!("https://{}", url);
        }

        // ── headers ──────────────────────────
        let mut header_map: reqwest::header::HeaderMap = tab
            .headers
            .iter()
            .filter(|h| h.enabled)
            .filter_map(|h| {
                let key = self.resolve_vars(&h.key);
                let value = self.resolve_vars(&h.value);
                let name = reqwest::header::HeaderName::from_bytes(key.trim().as_bytes()).ok()?;
                let value = reqwest::header::HeaderValue::from_str(value.trim()).ok()?;
                Some((name, value))
            })
            .collect();

        // ── auth ──────────────────────────────
        // Auth fields go through resolve_vars too, so e.g. a Bearer Token
        // field of "{{access_token}}" picks up whatever the environment's
        // auth auto-refresh (or a manual paste) last wrote into that var.
        match tab.auth_type {
            AuthType::Basic => {
                let username = self.resolve_vars(&tab.username);
                let password = self.resolve_vars(&tab.password);
                if !username.is_empty() || !password.is_empty() {
                    let credentials = format!("{}:{}", username, password);
                    let encoded = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        credentials,
                    );
                    if let Ok(hv) =
                        reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded))
                    {
                        header_map.insert(reqwest::header::AUTHORIZATION, hv);
                    }
                }
            }
            AuthType::Bearer => {
                let bearer_token = self.resolve_vars(&tab.bearer_token);
                if !bearer_token.is_empty() {
                    if let Ok(hv) =
                        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", bearer_token))
                    {
                        header_map.insert(reqwest::header::AUTHORIZATION, hv);
                    }
                }
            }
            AuthType::ApiKey => {
                let api_key = self.resolve_vars(&tab.api_key);
                let api_key_name = self.resolve_vars(&tab.api_key_name);
                if !api_key.is_empty() && !api_key_name.is_empty() {
                    if tab.api_key_position == ApiKeyPosition::Header {
                        if let (Ok(hn), Ok(hv)) = (
                            reqwest::header::HeaderName::try_from(&api_key_name),
                            reqwest::header::HeaderValue::from_str(&api_key),
                        ) {
                            header_map.insert(hn, hv);
                        }
                    } else if let Ok(mut parsed) = url::Url::parse(&url) {
                        parsed
                            .query_pairs_mut()
                            .append_pair(&api_key_name, &api_key);
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
            let variables: serde_json::Value = serde_json::from_str(&tab.graphql_variables)
                .unwrap_or(serde_json::Value::Object(Default::default()));

            let body = serde_json::json!({
                "query": *tab.graphql_query,
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
                        .body(tab.request_body.to_string())
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
            HttpMethod::HEAD => client.head(&url),
            HttpMethod::OPTIONS => client.request(reqwest::Method::OPTIONS, &url),
            HttpMethod::QUERY => {
                let query_verb = reqwest::Method::from_bytes(b"QUERY").unwrap();
                client.request(query_verb, &url)
            }
        };

        Some((builder.headers(header_map), url))
    }

    /// Entry point for firing a request. Checks whether the active tab's
    /// environment needs a preventive token refresh first; if so, kicks that
    /// off and defers the actual send until it completes (see
    /// `Message::TokenRefreshed`). Otherwise sends immediately.
    pub(crate) fn send_request(&mut self) -> iced::Task<Message> {
        if let Some(task) = self.maybe_start_preventive_auth_refresh() {
            return task;
        }
        self.send_request_now()
    }

    /// If the active tab's environment has auto-refresh enabled and the
    /// stored access token is missing or known-expired, starts a refresh
    /// and returns a task that will re-invoke the send once it completes.
    /// Returns `None` when no refresh is needed (the common case).
    pub(crate) fn maybe_start_preventive_auth_refresh(&mut self) -> Option<iced::Task<Message>> {
        // Scoped so all immutable borrows of `self` end before we touch
        // `self.pending_resend_tab` / build the task below.
        let (tab_id, env_id, env_clone) = {
            let tab = self.current_tab()?;
            if tab.auth_type != AuthType::Bearer {
                return None;
            }
            let tab_id = tab.id;
            let env = self.environment_for_tab(tab)?;
            let cfg = env.auth_config.as_ref()?;
            if !cfg.auto_refresh_enabled || cfg.mode == AuthRefreshMode::None {
                return None;
            }
            let has_token = env
                .get_var(&cfg.access_token_var)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            // 30s buffer so we don't race a token that's about to expire mid-flight.
            if has_token && !env.is_token_expired(30) {
                return None;
            }
            (tab_id, env.id, env.clone())
        };

        self.pending_resend_tab = Some(tab_id);
        let client = HTTP_CLIENT.clone();
        Some(iced::Task::perform(
            async move { perform_auth_refresh(&client, &env_clone).await },
            move |result| Message::TokenRefreshed(env_id, result),
        ))
    }

    /// Called when a response comes back 401 while auto-refresh is
    /// available and this send hasn't already been retried once. Kicks off
    /// a refresh + one automatic resend, same as the preventive path but
    /// triggered reactively instead of by a stale expiry.
    pub(crate) fn maybe_retry_after_auth_failure(&mut self) -> Option<iced::Task<Message>> {
        let (tab_id, env_id, env_clone) = {
            let tab = self.current_tab()?;
            if tab.auth_type != AuthType::Bearer || tab.auth_retry_used {
                return None;
            }
            let tab_id = tab.id;
            let env = self.environment_for_tab(tab)?;
            let cfg = env.auth_config.as_ref()?;
            if !cfg.auto_refresh_enabled || cfg.mode == AuthRefreshMode::None {
                return None;
            }
            (tab_id, env.id, env.clone())
        };

        if let Some(tab_mut) = self.current_tab_mut() {
            tab_mut.auth_retry_used = true;
        }
        self.pending_resend_tab = Some(tab_id);
        let client = HTTP_CLIENT.clone();
        Some(iced::Task::perform(
            async move { perform_auth_refresh(&client, &env_clone).await },
            move |result| Message::TokenRefreshed(env_id, result),
        ))
    }

    pub(crate) fn send_request_now(&mut self) -> iced::Task<Message> {
        let Some((request, _url)) = self.build_request() else {
            return iced::Task::none();
        };
        let Some(tab) = self.current_tab() else {
            return iced::Task::none();
        };

        let is_head = tab.method == HttpMethod::HEAD;
        let cancel_flag = tab.cancel_flag.clone();
        tab.cancel_flag.store(false, Ordering::Relaxed);
        if let Some(mut_tab) = self.current_tab_mut() {
            mut_tab.response_time = None;
            mut_tab.stream_buffer = String::new();
        }

        iced::Task::stream(iced::stream::channel(
            100,
            move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                let start_time = tokio::time::Instant::now();
                let resp = match request.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        let msg = if e.is_timeout() {
                            "Request timed out".into()
                        } else {
                            format!("Request failed: {e}")
                        };
                        let _ = output
                            .send(Message::ResponseReceived(HttpResponse {
                                status: "Error".to_string(),
                                body: msg,
                                response_time: Some(start_time.elapsed()),
                                ..Default::default()
                            }))
                            .await;
                        return;
                    }
                };

                // ── Emit status + headers immediately ───────────────────────────
                let status = format!(
                    "{} {}",
                    resp.status().as_u16(),
                    resp.status().canonical_reason().unwrap_or("")
                );

                let hm = resp.headers().clone();

                // Cookies
                let set_cookies: Vec<String> = hm
                    .get_all("set-cookie")
                    .iter()
                    .filter_map(|v| v.to_str().ok().map(|s| s.to_string()))
                    .collect();

                // Headers all
                let headers_text = format!("{:#?}", hm);

                // ── 1. Header Evaluation & Content Inspection ───────────────────
                let content_type = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_lowercase();

                let is_chunked = resp
                    .headers()
                    .get(reqwest::header::TRANSFER_ENCODING)
                    .and_then(|v| v.to_str().ok())
                    .map_or(false, |v| v.to_lowercase().contains("chunked"));

                let content_length = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());

                let is_explicit_stream = content_type.contains("text/event-stream")
                    || content_type.contains("application/x-ndjson")
                    || content_type.contains("application/json-seq");

                // let is_large_body =
                //     content_length.map_or(false, |len| len > LARGE_RESPONSE_THRESHOLD);

                let should_stream = is_explicit_stream || is_chunked;

                // ── 2. Binary Handling Path ─────────────────────────────────────
                let is_binary = content_type.starts_with("image/")
                    || content_type.starts_with("application/pdf")
                    || content_type.starts_with("application/octet-stream")
                    || content_type.starts_with("video/")
                    || content_type.starts_with("audio/");

                if is_binary {
                    let accepts_range = hm
                        .get("accept-ranges")
                        .and_then(|h| h.to_str().ok())
                        .is_some();
                    let filename = hm
                        .get("content-disposition")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| {
                            s.split("filename=")
                                .nth(1)
                                .map(|f| f.trim_matches(|c| c == '"' || c == '\'').to_string())
                        })
                        .unwrap_or_else(|| {
                            _url.split('/').last().unwrap_or("download").to_string()
                        });

                    if accepts_range && content_type.starts_with("video/") {
                        let _ = output
                            .send(Message::ResponseReceived(HttpResponse {
                                status,
                                headers: headers_text,
                                is_binary: true,
                                filename,
                                content_type,
                                response_time: Some(start_time.elapsed()),
                                accepts_range: true,
                                ..Default::default()
                            }))
                            .await;
                        return;
                    }

                    let (body, bytes) = match resp.bytes().await {
                        Ok(b) => (
                            format!(
                                "Binary file ({} bytes)\n\nContent-Type: {}",
                                b.len(),
                                content_type
                            ),
                            b.to_vec(),
                        ),
                        Err(e) => (format!("Error reading binary data: {e}"), vec![]),
                    };

                    let _ = output
                        .send(Message::ResponseReceived(HttpResponse {
                            status,
                            headers: headers_text,
                            body,
                            is_binary: true,
                            filename,
                            bytes,
                            content_type,
                            response_time: Some(start_time.elapsed()),
                            set_cookies,
                            accepts_range,
                            is_streaming: false,
                        }))
                        .await;
                    return;
                }

                // ── 3. Empty Responses (HEAD, 204, 304) ────────────────────────
                let is_empty_status =
                    resp.status().as_u16() == 204 || resp.status().as_u16() == 304;
                if is_head || is_empty_status {
                    let _ = output
                        .send(Message::ResponseReceived(HttpResponse {
                            status,
                            headers: headers_text,
                            body: String::new(),
                            content_type,
                            response_time: Some(start_time.elapsed()),
                            set_cookies,
                            ..Default::default()
                        }))
                        .await;
                    let _ = output.send(Message::StreamDone).await;
                    return;
                }

                // ── 4. Text Handling: Buffered vs. Streamed ────────────────────
                if !should_stream {
                    // ── BUFFERED PATH (Fast, 1 UI update for normal JSON/HTML)
                    let body = match resp.text().await {
                        Ok(t) => t,
                        Err(e) => format!("Error reading response body: {e}"),
                    };

                    let _ = output
                        .send(Message::ResponseReceived(HttpResponse {
                            status,
                            headers: headers_text,
                            body,
                            content_type,
                            response_time: Some(start_time.elapsed()),
                            set_cookies,
                            ..Default::default()
                        }))
                        .await;
                    let _ = output.send(Message::StreamDone).await;
                } else {
                    // ── STREAMED PATH (Real-time chunks for SSE / chunked / >5MB)
                    let _ = output
                        .send(Message::ResponseReceived(HttpResponse {
                            status,
                            headers: headers_text,
                            body: String::new(),
                            is_streaming: true,
                            content_type,
                            response_time: Some(start_time.elapsed()),
                            set_cookies,
                            ..Default::default()
                        }))
                        .await;

                    use futures_util::StreamExt;
                    let mut byte_stream = resp.bytes_stream();
                    let mut buf: Vec<u8> = Vec::new();

                    while let Some(chunk_result) = byte_stream.next().await {
                        if cancel_flag.load(Ordering::Relaxed) {
                            let _ = output
                                .send(Message::StreamChunk("…[cancelled]".to_string()))
                                .await;
                            break;
                        }
                        match chunk_result {
                            Ok(bytes) => {
                                buf.extend_from_slice(bytes.as_ref());
                                match std::str::from_utf8(buf.as_slice()) {
                                    Ok(s) => {
                                        let _ =
                                            output.send(Message::StreamChunk(s.to_string())).await;
                                        buf.clear();
                                    }
                                    Err(e) => {
                                        let valid_up_to = e.valid_up_to();
                                        if valid_up_to > 0 {
                                            let s = unsafe {
                                                std::str::from_utf8_unchecked(
                                                    &buf.as_slice()[..valid_up_to],
                                                )
                                            }
                                            .to_string();
                                            buf = buf.as_slice()[valid_up_to..].to_vec();
                                            let _ = output.send(Message::StreamChunk(s)).await;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = output
                                    .send(Message::StreamChunk(format!("\n[stream error: {e}]")))
                                    .await;
                                break;
                            }
                        }
                    }

                    let _ = output.send(Message::StreamDone).await;
                }
            },
        ))
    }
}

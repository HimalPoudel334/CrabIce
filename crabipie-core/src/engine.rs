//! Executes a built request and reports progress through a callback, so any
//! UI (or a test) can consume it without depending on a toolkit's task type.
//!
//! Event order, matching the original iced app:
//!   * error / binary / empty-body:  `Response` only (empty-body also `Done`)
//!   * buffered text:                `Response`, `Done`
//!   * streamed (SSE / chunked):     `Response` (is_streaming), `Chunk`*, `Done`

use crate::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ResponseEvent {
    Response(HttpResponse),
    Chunk(String),
    Done,
}

pub async fn execute_request(
    request: reqwest::RequestBuilder,
    url: String,
    is_head: bool,
    cancel_flag: Arc<AtomicBool>,
    mut emit: impl FnMut(ResponseEvent),
) {
    cancel_flag.store(false, Ordering::Relaxed);
    let start_time = tokio::time::Instant::now();

    let resp = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            let msg = if e.is_timeout() {
                "Request timed out".to_string()
            } else {
                format!("Request failed: {e}")
            };
            emit(ResponseEvent::Response(HttpResponse {
                status: "Error".to_string(),
                body: msg,
                response_time: Some(start_time.elapsed()),
                ..Default::default()
            }));
            return;
        }
    };

    // ── status + headers ────────────────────────────────────────────────
    let status = format!(
        "{} {}",
        resp.status().as_u16(),
        resp.status().canonical_reason().unwrap_or("")
    );
    let hm = resp.headers().clone();

    let set_cookies: Vec<String> = hm
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok().map(|s| s.to_string()))
        .collect();

    // "name: value" per line (the iced app showed Rust's debug dump).
    let headers_text = hm
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("<binary>")))
        .collect::<Vec<_>>()
        .join("\n");

    // ── content inspection ──────────────────────────────────────────────
    let content_type = hm
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let is_chunked = hm
        .get(reqwest::header::TRANSFER_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map_or(false, |v| v.to_lowercase().contains("chunked"));

    let is_explicit_stream = content_type.contains("text/event-stream")
        || content_type.contains("application/x-ndjson")
        || content_type.contains("application/json-seq");

    let should_stream = is_explicit_stream || is_chunked;

    // ── binary ──────────────────────────────────────────────────────────
    let is_binary = content_type.starts_with("image/")
        || content_type.starts_with("application/pdf")
        || content_type.starts_with("application/octet-stream")
        || content_type.starts_with("video/")
        || content_type.starts_with("audio/");

    if is_binary {
        let accepts_range = hm.get("accept-ranges").and_then(|h| h.to_str().ok()).is_some();
        let filename = hm
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.split("filename=")
                    .nth(1)
                    .map(|f| f.trim_matches(|c| c == '"' || c == '\'').to_string())
            })
            .unwrap_or_else(|| url.split('/').last().unwrap_or("download").to_string());

        // Seekable video is played straight from the URL, not downloaded.
        if accepts_range && content_type.starts_with("video/") {
            emit(ResponseEvent::Response(HttpResponse {
                status,
                headers: headers_text,
                is_binary: true,
                filename,
                content_type,
                response_time: Some(start_time.elapsed()),
                accepts_range: true,
                ..Default::default()
            }));
            return;
        }

        let (body, bytes) = match resp.bytes().await {
            Ok(b) => (
                format!("Binary file ({} bytes)\n\nContent-Type: {}", b.len(), content_type),
                b.to_vec(),
            ),
            Err(e) => (format!("Error reading binary data: {e}"), vec![]),
        };

        emit(ResponseEvent::Response(HttpResponse {
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
        }));
        return;
    }

    // ── empty responses (HEAD, 204, 304) ────────────────────────────────
    let code = resp.status().as_u16();
    if is_head || code == 204 || code == 304 {
        emit(ResponseEvent::Response(HttpResponse {
            status,
            headers: headers_text,
            body: String::new(),
            content_type,
            response_time: Some(start_time.elapsed()),
            set_cookies,
            ..Default::default()
        }));
        emit(ResponseEvent::Done);
        return;
    }

    // ── text: buffered ──────────────────────────────────────────────────
    if !should_stream {
        let body = match resp.text().await {
            Ok(t) => t,
            Err(e) => format!("Error reading response body: {e}"),
        };
        emit(ResponseEvent::Response(HttpResponse {
            status,
            headers: headers_text,
            body,
            content_type,
            response_time: Some(start_time.elapsed()),
            set_cookies,
            ..Default::default()
        }));
        emit(ResponseEvent::Done);
        return;
    }

    // ── text: streamed ──────────────────────────────────────────────────
    emit(ResponseEvent::Response(HttpResponse {
        status,
        headers: headers_text,
        body: String::new(),
        is_streaming: true,
        content_type,
        response_time: Some(start_time.elapsed()),
        set_cookies,
        ..Default::default()
    }));

    use futures_util::StreamExt;
    let mut byte_stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();

    while let Some(chunk_result) = byte_stream.next().await {
        if cancel_flag.load(Ordering::Relaxed) {
            emit(ResponseEvent::Chunk("…[cancelled]".to_string()));
            break;
        }
        match chunk_result {
            Ok(bytes) => {
                buf.extend_from_slice(bytes.as_ref());
                match std::str::from_utf8(&buf) {
                    Ok(s) => {
                        emit(ResponseEvent::Chunk(s.to_string()));
                        buf.clear();
                    }
                    Err(e) => {
                        // Emit the valid prefix; keep a split multi-byte
                        // character in the buffer for the next chunk.
                        let valid_up_to = e.valid_up_to();
                        if valid_up_to > 0 {
                            let s = String::from_utf8_lossy(&buf[..valid_up_to]).into_owned();
                            buf.drain(..valid_up_to);
                            emit(ResponseEvent::Chunk(s));
                        }
                    }
                }
            }
            Err(e) => {
                emit(ResponseEvent::Chunk(format!("\n[stream error: {e}]")));
                break;
            }
        }
    }

    emit(ResponseEvent::Done);
}

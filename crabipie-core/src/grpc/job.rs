//! What the UI runs for one gRPC action: discovery, or one call (discovery
//! included when no descriptors are loaded yet). Progress comes back as events.

use super::client::{call, call_stream, connect, discover, method_path, parse_target, CallEnd, ConnectOpts};
use super::desc::Pool;
use super::dynmsg;
use crate::{resolve_vars, ApiKeyPosition, AuthType, Environment, SavedState};
use std::sync::Arc;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Clone, Debug)]
pub enum GrpcEvent {
    /// Descriptors loaded through server reflection.
    Pool(Arc<Pool>),
    DiscoveryFailed(String),
    /// One response message, pretty-printed JSON.
    Message(String),
    Done(CallEnd),
    /// The call could not be made or was cut off (not a gRPC error status).
    Failed(String),
    /// The method streams requests: the call is open and accepts `GrpcInput`.
    StreamOpen,
    /// Total request messages sent so far on a streaming call.
    Sent(usize),
    /// A follow-up message could not be sent (bad JSON, unknown field...); the call goes on.
    InputError(String),
}

/// What the UI sends into a running client-streaming or bidirectional call.
#[derive(Clone, Debug)]
pub enum GrpcInput {
    /// Another request message, as JSON.
    Message(String),
    /// Close the request side; the server may keep answering.
    Finish,
}

#[derive(Clone, Debug)]
pub struct GrpcCall {
    pub url: String,
    pub insecure: bool,
    pub metadata: Vec<(String, String)>,
    /// Descriptors from an earlier discovery; None means discover first.
    pub pool: Option<Arc<Pool>>,
    /// "pkg.Service/Method". None = discovery only.
    pub method: Option<String>,
    /// Request message as JSON (blank means `{}`).
    pub body: String,
}

/// Enabled headers plus the auth settings, with `{{variables}}` resolved.
/// Bearer and Basic become `authorization`; an API key goes in only when its
/// position is "header".
pub fn metadata_from_state(tab: &SavedState, env: Option<&Environment>) -> Vec<(String, String)> {
    let rv = |s: &str| resolve_vars(s, env);
    let mut md: Vec<(String, String)> = tab
        .headers
        .iter()
        .filter(|h| h.enabled)
        .map(|h| (rv(&h.key).trim().to_string(), rv(&h.value).trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect();
    let mut set = |name: &str, value: String| {
        md.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        md.push((name.to_string(), value));
    };
    match tab.auth_type {
        AuthType::Basic => {
            let (u, p) = (rv(&tab.username), rv(&tab.password));
            if !u.is_empty() || !p.is_empty() {
                let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, format!("{u}:{p}"));
                set("authorization", format!("Basic {encoded}"));
            }
        }
        AuthType::Bearer => {
            let token = rv(&tab.bearer_token);
            if !token.is_empty() {
                set("authorization", format!("Bearer {token}"));
            }
        }
        AuthType::ApiKey => {
            let (name, value) = (rv(&tab.api_key_name), rv(&tab.api_key));
            if !name.is_empty() && !value.is_empty() && tab.api_key_position == ApiKeyPosition::Header {
                set(&name, value);
            }
        }
        AuthType::None => {}
    }
    md
}

fn request_json(body: &str) -> Result<serde_json::Value, String> {
    let t = body.trim();
    if t.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(t).map_err(|e| format!("the request is not valid JSON: {e}"))
}

/// Runs one action to completion, reporting through `emit`. A client-streaming
/// method gets its one message from the body and the request side is closed.
pub async fn run(req: GrpcCall, emit: impl FnMut(GrpcEvent)) {
    let (tx, rx) = unbounded_channel();
    drop(tx);
    run_with(req, emit, rx).await
}

/// Like `run`, with a channel for follow-up messages and `Finish` on streaming calls.
/// Dropping the returned future cancels everything.
pub async fn run_with(req: GrpcCall, mut emit: impl FnMut(GrpcEvent), inputs: UnboundedReceiver<GrpcInput>) {
    // The work reports into a channel so that several parts of it can emit at once.
    let (etx, mut erx) = unbounded_channel::<GrpcEvent>();
    let job = work(req, etx.clone(), inputs);
    tokio::pin!(job);
    loop {
        tokio::select! {
            _ = &mut job => break,
            Some(ev) = erx.recv() => emit(ev),
        }
    }
    drop(etx);
    while let Ok(ev) = erx.try_recv() {
        emit(ev);
    }
}

async fn work(req: GrpcCall, tx: UnboundedSender<GrpcEvent>, mut inputs: UnboundedReceiver<GrpcInput>) {
    let emit = |e: GrpcEvent| {
        let _ = tx.send(e);
    };
    let discovery_only = req.method.is_none();
    let fail = |e: String| if discovery_only { GrpcEvent::DiscoveryFailed(e) } else { GrpcEvent::Failed(e) };

    let target = match parse_target(&req.url) {
        Ok(t) => t,
        Err(e) => return emit(fail(e)),
    };
    let opts = ConnectOpts { insecure: req.insecure, ..ConnectOpts::default() };
    let conn = match connect(&target, &opts).await {
        Ok(c) => c,
        Err(e) => return emit(fail(e)),
    };
    let pool = match req.pool.clone() {
        Some(p) => p,
        None => match discover(&conn, &req.metadata).await {
            Ok(p) => {
                let p = Arc::new(p);
                emit(GrpcEvent::Pool(p.clone()));
                p
            }
            Err(e) => return emit(fail(e)),
        },
    };
    let Some(method) = req.method.as_deref() else { return };
    let Some((svc, name)) = method.split_once('/') else {
        return emit(GrpcEvent::Failed(format!("\"{method}\" is not a service/method name")));
    };
    let Some(m) = pool.find_method(svc, name) else {
        return emit(GrpcEvent::Failed(format!("the server has no method {method}; try Load services again")));
    };
    let value = match request_json(&req.body) {
        Ok(v) => v,
        Err(e) => return emit(GrpcEvent::Failed(e)),
    };
    let bytes = match dynmsg::encode(&pool, &m.input, &value) {
        Ok(b) => b,
        Err(e) => return emit(GrpcEvent::Failed(format!("request: {e}"))),
    };
    let output = m.output.clone();
    let path = method_path(svc, name);
    let on_msg = |msg: Vec<u8>| match dynmsg::decode(&pool, &output, &msg) {
        Ok(v) => emit(GrpcEvent::Message(serde_json::to_string_pretty(&v).unwrap_or_default())),
        Err(e) => emit(GrpcEvent::Message(format!("(could not decode a response message: {e})"))),
    };

    let result = if m.client_streaming {
        // The body is the first message; more arrive through `inputs` until Finish.
        let (otx, orx) = unbounded_channel::<Vec<u8>>();
        let _ = otx.send(bytes);
        emit(GrpcEvent::StreamOpen);
        emit(GrpcEvent::Sent(1));
        let mut sent = 1usize;
        let mut otx = Some(otx);
        let mut inputs_open = true;
        let call_fut = call_stream(&conn, &path, &req.metadata, orx, on_msg);
        tokio::pin!(call_fut);
        loop {
            tokio::select! {
                res = &mut call_fut => break res,
                input = inputs.recv(), if inputs_open => match input {
                    Some(GrpcInput::Message(text)) => {
                        let encoded = request_json(&text).and_then(|v| dynmsg::encode(&pool, &m.input, &v).map_err(|e| format!("request: {e}")));
                        match (encoded, otx.as_ref()) {
                            (Ok(b), Some(o)) => {
                                if o.send(b).is_ok() {
                                    sent += 1;
                                    emit(GrpcEvent::Sent(sent));
                                }
                            }
                            (Err(e), _) => emit(GrpcEvent::InputError(e)),
                            (Ok(_), None) => {}
                        }
                    }
                    // Finish, or the UI went away: close our side of the stream
                    Some(GrpcInput::Finish) | None => {
                        otx = None;
                        inputs_open = false;
                    }
                },
            }
        }
    } else {
        call(&conn, &path, &req.metadata, &bytes, on_msg).await
    };
    match result {
        Ok(end) => emit(GrpcEvent::Done(end)),
        Err(e) => emit(GrpcEvent::Failed(e)),
    }
}

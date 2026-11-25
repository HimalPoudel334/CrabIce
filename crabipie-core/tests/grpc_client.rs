//! Loopback tests: a tiny in-process HTTP/2 server plays the gRPC server.

use bytes::Bytes;
use crabipie_core::grpc::client::*;
use crabipie_core::grpc::desc::Pool;
use crabipie_core::grpc::frame::{frame, Deframer};
use crabipie_core::grpc::job::{metadata_from_state, run, run_with, GrpcCall, GrpcEvent, GrpcInput};
use crabipie_core::grpc::proto::load_proto;
use crabipie_core::grpc::wire::*;
use crabipie_core::{ApiKeyPosition, AuthType, RequestHeaders, SavedState};
use http::{HeaderMap, HeaderValue, Response};
use std::sync::Arc;
use tokio::net::TcpListener;

/// (path, request payload) -> (reply messages, grpc-status). No messages plus a
/// non-zero status is answered "trailers-only", like real servers do for early errors.
type Handler = Arc<dyn Fn(&str, &[u8]) -> (Vec<Vec<u8>>, &'static str) + Send + Sync>;

async fn serve(handler: Handler) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut conn = h2::server::handshake(tcp).await.unwrap();
        while let Some(Ok((req, mut respond))) = conn.accept().await {
            let handler = handler.clone();
            tokio::spawn(async move {
                let (parts, mut body) = req.into_parts();
                let mut got = Vec::new();
                while let Some(Ok(chunk)) = body.data().await {
                    let _ = body.flow_control().release_capacity(chunk.len());
                    got.extend_from_slice(&chunk);
                }
                let (replies, status) = handler(parts.uri.path(), &got[5..]);
                let mut trailers = HeaderMap::new();
                trailers.insert("grpc-status", HeaderValue::from_static(status));
                trailers.insert("grpc-message", HeaderValue::from_static("boom%20bang"));
                let mut resp = Response::builder().status(200).header("content-type", "application/grpc").body(()).unwrap();
                if replies.is_empty() && status != "0" {
                    resp.headers_mut().extend(trailers);
                    respond.send_response(resp, true).unwrap();
                } else {
                    let mut tx = respond.send_response(resp, false).unwrap();
                    for r in replies {
                        tx.send_data(Bytes::from(frame(&r)), false).unwrap();
                    }
                    tx.send_trailers(trailers).unwrap();
                }
            });
        }
    });
    port
}

async fn connect_to(port: u16) -> Conn {
    let t = parse_target(&format!("http://127.0.0.1:{port}")).unwrap();
    connect(&t, &ConnectOpts::default()).await.unwrap()
}

#[test]
fn targets_parse_scheme_port_and_tls_defaults() {
    let t = parse_target("localhost:50051").unwrap();
    assert_eq!(t, Target { host: "localhost".into(), port: 50051, tls: false });
    assert!(parse_target("api.example.com:443").unwrap().tls, "bare remote host defaults to TLS");
    assert_eq!(parse_target("https://api.example.com").unwrap().port, 443);
    assert_eq!(parse_target("grpc://10.0.0.5:9000/some/path").unwrap(), Target { host: "10.0.0.5".into(), port: 9000, tls: false });
    assert_eq!(parse_target("grpcs://svc.dev").unwrap(), Target { host: "svc.dev".into(), port: 443, tls: true });
    let v6 = parse_target("http://[::1]:8080").unwrap();
    assert_eq!((v6.host.as_str(), v6.port, v6.tls), ("::1", 8080, false));
    assert!(parse_target("").is_err());
    assert!(parse_target("ftp://x").is_err());
    assert!(parse_target("host:notaport").is_err());
    assert_eq!(method_path("demo.Shop", "Get"), "/demo.Shop/Get");
}

#[tokio::test]
async fn unary_call_sends_one_framed_message_and_reads_status_from_trailers() {
    let port = serve(Arc::new(|path, req| {
        assert_eq!(path, "/demo.Shop/Get");
        (vec![req.to_vec()], "0")
    }))
    .await;
    let conn = connect_to(port).await;
    let mut got = Vec::new();
    let md = vec![("X-Token".to_string(), "abc".to_string())];
    let end = call(&conn, "/demo.Shop/Get", &md, b"hello", |m| got.push(m)).await.unwrap();
    assert_eq!(got, vec![b"hello".to_vec()]);
    assert!(end.ok());
    assert_eq!(end.summary(), "OK: boom bang", "message is percent-decoded");
}

#[tokio::test]
async fn server_streaming_delivers_every_message_in_order() {
    let port = serve(Arc::new(|_, _| (vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()], "0"))).await;
    let conn = connect_to(port).await;
    let mut got = Vec::new();
    let end = call(&conn, "/x.Y/Z", &[], b"", |m| got.push(m)).await.unwrap();
    assert_eq!(got, vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]);
    assert_eq!(end.status, 0);
}

#[tokio::test]
async fn error_status_in_trailers_only_reply_is_returned_not_thrown() {
    let port = serve(Arc::new(|_, _| (vec![], "5"))).await;
    let conn = connect_to(port).await;
    let end = call(&conn, "/x.Y/Z", &[], b"", |_| panic!("no messages expected")).await.unwrap();
    assert_eq!(end.status, 5);
    assert_eq!(end.summary(), "NOT_FOUND: boom bang");
    assert!(end.headers.iter().any(|(k, _)| k == "grpc-status"), "status travelled in the headers");
}

#[tokio::test]
async fn connecting_to_a_closed_port_is_an_error_message() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    let t = parse_target(&format!("http://127.0.0.1:{port}")).unwrap();
    let err = connect(&t, &ConnectOpts::default()).await.err().expect("should fail");
    assert!(err.contains("connect"), "{err}");
}

// ---- reflection: a fake server that only speaks v1alpha, to prove the fallback ----

fn put_str(out: &mut Vec<u8>, f: u32, s: &str) {
    put_len(out, f, s.as_bytes());
}

fn put_int(out: &mut Vec<u8>, f: u32, n: u64) {
    put_tag(out, f, VARINT);
    put_varint(out, n);
}

/// proto3 package demo: message Req { string q = 1; } service Echo { rpc Say(Req) returns (Req); }
fn echo_file() -> Vec<u8> {
    let mut q = Vec::new();
    put_str(&mut q, 1, "q");
    put_int(&mut q, 3, 1);
    put_int(&mut q, 4, 1);
    put_int(&mut q, 5, 9);
    let mut req = Vec::new();
    put_str(&mut req, 1, "Req");
    put_len(&mut req, 2, &q);
    let mut say = Vec::new();
    put_str(&mut say, 1, "Say");
    put_str(&mut say, 2, ".demo.Req");
    put_str(&mut say, 3, ".demo.Req");
    let mut svc = Vec::new();
    put_str(&mut svc, 1, "Echo");
    put_len(&mut svc, 2, &say);
    let mut file = Vec::new();
    put_str(&mut file, 1, "echo.proto");
    put_str(&mut file, 2, "demo");
    put_len(&mut file, 4, &req);
    put_len(&mut file, 6, &svc);
    put_str(&mut file, 12, "proto3");
    file
}

#[test]
fn reflection_replies_are_parsed() {
    let mut names = Vec::new();
    for n in ["demo.Echo", "grpc.reflection.v1.ServerReflection"] {
        let mut s = Vec::new();
        put_str(&mut s, 1, n);
        put_len(&mut names, 1, &s);
    }
    let mut resp = Vec::new();
    put_len(&mut resp, 6, &names);
    assert_eq!(parse_reflection_response(&resp).unwrap(), Refl::Services(vec!["demo.Echo".into(), "grpc.reflection.v1.ServerReflection".into()]));

    let mut files = Vec::new();
    put_len(&mut files, 1, b"AB");
    put_len(&mut files, 1, b"CD");
    let mut resp = Vec::new();
    put_len(&mut resp, 4, &files);
    assert_eq!(parse_reflection_response(&resp).unwrap(), Refl::Files(vec![b"AB".to_vec(), b"CD".to_vec()]));

    let mut e = Vec::new();
    put_int(&mut e, 1, 5);
    put_str(&mut e, 2, "nope");
    let mut resp = Vec::new();
    put_len(&mut resp, 7, &e);
    assert_eq!(parse_reflection_response(&resp).unwrap(), Refl::Error(5, "nope".into()));

    assert!(parse_reflection_response(&[]).is_err());
}

/// Speaks only reflection v1alpha (v1 answers UNIMPLEMENTED) plus `/demo.Echo/Say`, which echoes.
fn echo_handler(file: Vec<u8>) -> Handler {
    Arc::new(move |path, req| {
        if path == "/demo.Echo/Say" {
            return (vec![req.to_vec()], "0");
        }
        if path != "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo" {
            return (vec![], "12"); // v1 not implemented
        }
        let mut resp = Vec::new();
        match req.first() {
            Some(0x3a) => {
                // list_services (field 7)
                let mut names = Vec::new();
                for n in ["demo.Echo", "grpc.reflection.v1alpha.ServerReflection"] {
                    let mut s = Vec::new();
                    put_str(&mut s, 1, n);
                    put_len(&mut names, 1, &s);
                }
                put_len(&mut resp, 6, &names);
            }
            Some(0x22) => {
                // file_containing_symbol (field 4)
                let mut files = Vec::new();
                put_len(&mut files, 1, &file);
                put_len(&mut resp, 4, &files);
            }
            _ => return (vec![], "3"),
        }
        (vec![resp], "0")
    })
}

#[tokio::test]
async fn discover_falls_back_to_v1alpha_and_loads_the_service() {
    let port = serve(echo_handler(echo_file())).await;
    let conn = connect_to(port).await;
    let pool = discover(&conn, &[]).await.unwrap();
    assert_eq!(pool.services().len(), 1, "the reflection service itself is hidden");
    let m = pool.find_method("demo.Echo", "Say").unwrap();
    assert_eq!((m.input.as_str(), m.output.as_str()), ("demo.Req", "demo.Req"));
    assert!(pool.message("demo.Req").is_some());
}

fn call_to(port: u16, method: Option<&str>, body: &str) -> GrpcCall {
    GrpcCall { url: format!("http://127.0.0.1:{port}"), insecure: false, metadata: vec![], pool: None, method: method.map(String::from), body: body.into() }
}

#[tokio::test]
async fn run_discovers_calls_and_decodes_the_reply_to_json() {
    let port = serve(echo_handler(echo_file())).await;
    let mut events = Vec::new();
    run(call_to(port, Some("demo.Echo/Say"), r#"{"q": "hi"}"#), |e| events.push(e)).await;
    assert_eq!(events.len(), 3, "{events:?}");
    assert!(matches!(events[0], GrpcEvent::Pool(_)));
    match &events[1] {
        GrpcEvent::Message(m) => assert_eq!(serde_json::from_str::<serde_json::Value>(m).unwrap(), serde_json::json!({"q": "hi"})),
        other => panic!("{other:?}"),
    }
    match &events[2] {
        GrpcEvent::Done(end) => assert!(end.ok()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn run_reuses_loaded_descriptors_and_explains_failures() {
    let port = serve(echo_handler(echo_file())).await;
    // discovery only
    let mut events = Vec::new();
    run(call_to(port, None, ""), |e| events.push(e)).await;
    let pool = match &events[..] {
        [GrpcEvent::Pool(p)] => p.clone(),
        other => panic!("{other:?}"),
    };
    // a second call skips discovery (the fake server would answer it, but no Pool event comes)
    let mut again = call_to(port, Some("demo.Echo/Say"), "");
    again.pool = Some(pool.clone());
    let mut events = Vec::new();
    run(again, |e| events.push(e)).await;
    assert!(matches!(events[0], GrpcEvent::Message(_)), "{events:?}");

    // bad JSON, unknown method, unreachable server
    let mut bad = call_to(port, Some("demo.Echo/Say"), "{nope");
    bad.pool = Some(pool.clone());
    let mut events = Vec::new();
    run(bad, |e| events.push(e)).await;
    assert!(matches!(&events[..], [GrpcEvent::Failed(m)] if m.contains("valid JSON")), "{events:?}");

    let mut unknown = call_to(port, Some("demo.Echo/Nope"), "");
    unknown.pool = Some(pool);
    let mut events = Vec::new();
    run(unknown, |e| events.push(e)).await;
    assert!(matches!(&events[..], [GrpcEvent::Failed(m)] if m.contains("no method")), "{events:?}");

    let mut events = Vec::new();
    run(GrpcCall { url: "ftp://x".into(), ..call_to(port, None, "") }, |e| events.push(e)).await;
    assert!(matches!(&events[..], [GrpcEvent::DiscoveryFailed(m)] if m.contains("scheme")), "{events:?}");
}

#[test]
fn metadata_combines_enabled_headers_with_auth() {
    let mut s = SavedState::default();
    s.headers = vec![
        RequestHeaders { key: "X-Trace".into(), value: " 42 ".into(), enabled: true },
        RequestHeaders { key: "Skipped".into(), value: "x".into(), enabled: false },
        RequestHeaders { key: "".into(), value: "blank".into(), enabled: true },
        RequestHeaders { key: "Authorization".into(), value: "old".into(), enabled: true },
    ];
    s.auth_type = AuthType::Bearer;
    s.bearer_token = "tok".into();
    assert_eq!(metadata_from_state(&s, None), vec![("X-Trace".to_string(), "42".to_string()), ("authorization".to_string(), "Bearer tok".to_string())]);

    s.auth_type = AuthType::Basic;
    s.username = "u".into();
    s.password = "p".into();
    let md = metadata_from_state(&s, None);
    assert!(md.contains(&("authorization".to_string(), "Basic dTpw".to_string())), "{md:?}");

    s.auth_type = AuthType::ApiKey;
    s.api_key_name = "x-api-key".into();
    s.api_key = "k".into();
    s.api_key_position = ApiKeyPosition::Header;
    assert!(metadata_from_state(&s, None).contains(&("x-api-key".to_string(), "k".to_string())));
}

#[tokio::test]
async fn discover_reports_a_server_without_reflection() {
    let port = serve(Arc::new(|_, _| (vec![], "12"))).await;
    let conn = connect_to(port).await;
    let err = discover(&conn, &[]).await.unwrap_err();
    assert!(err.contains("reflection"), "{err}");
}

// ---- client-streaming and bidirectional calls ----

/// Full-duplex fake: every request message is echoed back the moment it arrives,
/// and the call ends with grpc-status 0 once the client closes its side.
async fn serve_echo_stream() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut conn = h2::server::handshake(tcp).await.unwrap();
        while let Some(Ok((req, mut respond))) = conn.accept().await {
            tokio::spawn(async move {
                let (_parts, mut body) = req.into_parts();
                let resp = Response::builder().status(200).header("content-type", "application/grpc").body(()).unwrap();
                let mut tx = respond.send_response(resp, false).unwrap();
                let mut de = Deframer::default();
                while let Some(Ok(chunk)) = body.data().await {
                    let _ = body.flow_control().release_capacity(chunk.len());
                    de.push(&chunk);
                    while let Some((_, m)) = de.pop().unwrap() {
                        tx.send_data(Bytes::from(frame(&m)), false).unwrap();
                    }
                }
                let mut trailers = HeaderMap::new();
                trailers.insert("grpc-status", HeaderValue::from_static("0"));
                tx.send_trailers(trailers).unwrap();
            });
        }
    });
    port
}

#[tokio::test]
async fn bidirectional_call_answers_before_the_client_is_done() {
    let port = serve_echo_stream().await;
    let conn = connect_to(port).await;
    let (otx, orx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let got = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let sink = got.clone();
    let task = tokio::spawn(async move { call_stream(&conn, "/x.Y/Chat", &[], orx, move |m| sink.lock().unwrap().push(m)).await });

    otx.send(b"a".to_vec()).unwrap();
    for _ in 0..200 {
        if got.lock().unwrap().len() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(got.lock().unwrap().clone(), vec![b"a".to_vec()], "the echo arrives while the request stream is still open");

    otx.send(b"b".to_vec()).unwrap();
    drop(otx); // half-close
    let end = task.await.unwrap().unwrap();
    assert!(end.ok());
    assert_eq!(got.lock().unwrap().clone(), vec![b"a".to_vec(), b"b".to_vec()]);
}

#[tokio::test]
async fn the_request_side_can_be_closed_without_sending_anything() {
    let port = serve_echo_stream().await;
    let conn = connect_to(port).await;
    let (otx, orx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    drop(otx);
    let end = call_stream(&conn, "/x.Y/Z", &[], orx, |_| panic!("nothing was sent, nothing comes back")).await.unwrap();
    assert!(end.ok());
}

fn chat_pool() -> Pool {
    let src = r#"syntax = "proto3"; package demo; message Req { string q = 1; }
        service Echo { rpc Chat(stream Req) returns (stream Req); }"#;
    let mut pool = Pool::default();
    load_proto(&mut pool, "chat.proto", src, &|_: &str| None).unwrap();
    pool
}

fn quoted(events: &[GrpcEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            GrpcEvent::Message(m) => Some(serde_json::from_str::<serde_json::Value>(m).unwrap()["q"].as_str().unwrap().to_string()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn run_with_streams_messages_until_finish_and_reports_bad_ones() {
    let port = serve_echo_stream().await;
    let mut call = call_to(port, Some("demo.Echo/Chat"), r#"{"q": "one"}"#);
    call.pool = Some(Arc::new(chat_pool()));
    let (itx, irx) = tokio::sync::mpsc::unbounded_channel();
    itx.send(GrpcInput::Message("{bad".into())).unwrap();
    itx.send(GrpcInput::Message(r#"{"q": "two"}"#.into())).unwrap();
    itx.send(GrpcInput::Finish).unwrap();
    let mut events = Vec::new();
    run_with(call, |e| events.push(e), irx).await;
    assert_eq!(quoted(&events), ["one", "two"], "{events:?}");
    assert!(events.iter().any(|e| matches!(e, GrpcEvent::StreamOpen)));
    assert!(events.iter().any(|e| matches!(e, GrpcEvent::Sent(2))));
    assert!(events.iter().any(|e| matches!(e, GrpcEvent::InputError(m) if m.contains("JSON"))), "{events:?}");
    assert!(matches!(events.last(), Some(GrpcEvent::Done(end)) if end.ok()), "{events:?}");
}

#[tokio::test]
async fn plain_run_sends_the_one_body_message_and_closes_a_client_streaming_call() {
    let port = serve_echo_stream().await;
    let mut call = call_to(port, Some("demo.Echo/Chat"), r#"{"q": "solo"}"#);
    call.pool = Some(Arc::new(chat_pool()));
    let mut events = Vec::new();
    run(call, |e| events.push(e)).await;
    assert_eq!(quoted(&events), ["solo"], "{events:?}");
    assert!(matches!(events.last(), Some(GrpcEvent::Done(end)) if end.ok()), "{events:?}");
}

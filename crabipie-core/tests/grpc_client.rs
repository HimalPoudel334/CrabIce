//! Loopback tests: a tiny in-process HTTP/2 server plays the gRPC server.

use bytes::Bytes;
use crabipie_core::grpc::client::*;
use crabipie_core::grpc::frame::frame;
use crabipie_core::grpc::wire::*;
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

#[tokio::test]
async fn discover_falls_back_to_v1alpha_and_loads_the_service() {
    let file = echo_file();
    let port = serve(Arc::new(move |path, req| {
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
    }))
    .await;
    let conn = connect_to(port).await;
    let pool = discover(&conn, &[]).await.unwrap();
    assert_eq!(pool.services().len(), 1, "the reflection service itself is hidden");
    let m = pool.find_method("demo.Echo", "Say").unwrap();
    assert_eq!((m.input.as_str(), m.output.as_str()), ("demo.Req", "demo.Req"));
    assert!(pool.message("demo.Req").is_some());
}

#[tokio::test]
async fn discover_reports_a_server_without_reflection() {
    let port = serve(Arc::new(|_, _| (vec![], "12"))).await;
    let conn = connect_to(port).await;
    let err = discover(&conn, &[]).await.unwrap_err();
    assert!(err.contains("reflection"), "{err}");
}

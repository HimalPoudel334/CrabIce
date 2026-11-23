//! gRPC over HTTP/2, straight on `h2`: connect, call (unary and server
//! streaming), and server-reflection discovery. `reqwest` is not used here
//! because it hides trailers, and trailers are where `grpc-status` lives.
//!
//! Metadata keys ending in `-bin` are sent as given (no base64 handling yet).

use super::desc::Pool;
use super::frame::{frame, gunzip, status_name, Deframer};
use super::wire::{put_len, Reader, LEN, VARINT};
use bytes::Bytes;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

// ------------------------------------------------------------------ target

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

fn is_local(host: &str) -> bool {
    host == "localhost" || host == "::1" || host == "0.0.0.0" || host.starts_with("127.")
}

/// `https://` / `grpcs://` use TLS, `http://` / `grpc://` are plaintext HTTP/2.
/// A bare `host:port` is plaintext for localhost and TLS for everything else.
pub fn parse_target(input: &str) -> Result<Target, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("enter a server address, e.g. localhost:50051".into());
    }
    let (scheme, rest) = match s.split_once("://") {
        Some((sc, r)) => (Some(sc.to_ascii_lowercase()), r),
        None => (None, s),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or("");
    let (host, port) = if let Some(r) = authority.strip_prefix('[') {
        let (h, after) = r.split_once(']').ok_or("missing ] in IPv6 address")?;
        (h.to_string(), after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p)),
            None => (authority.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err("the address has no host".into());
    }
    let tls = match scheme.as_deref() {
        Some("https") | Some("grpcs") => true,
        Some("http") | Some("grpc") => false,
        Some(other) => return Err(format!("unsupported scheme \"{other}\" (use http, https, grpc or grpcs)")),
        None => !is_local(&host),
    };
    let port = match port {
        Some(p) if !p.is_empty() => p.parse::<u16>().map_err(|_| format!("bad port \"{p}\""))?,
        _ => {
            if tls {
                443
            } else {
                80
            }
        }
    };
    Ok(Target { host, port, tls })
}

// -------------------------------------------------------------- connection

#[derive(Clone, Debug)]
pub struct ConnectOpts {
    /// Accept self-signed or mismatching certificates (dev servers).
    pub insecure: bool,
    pub timeout: Duration,
}

impl Default for ConnectOpts {
    fn default() -> Self {
        ConnectOpts { insecure: false, timeout: Duration::from_secs(10) }
    }
}

/// One HTTP/2 connection; cheap to clone, every call opens its own stream.
#[derive(Clone)]
pub struct Conn {
    send: h2::client::SendRequest<Bytes>,
    authority: String,
    tls: bool,
}

fn h2e(e: h2::Error) -> String {
    e.to_string()
}

pub async fn connect(t: &Target, opts: &ConnectOpts) -> Result<Conn, String> {
    match tokio::time::timeout(opts.timeout, connect_inner(t, opts)).await {
        Ok(r) => r,
        Err(_) => Err(format!("timed out connecting to {}:{}", t.host, t.port)),
    }
}

async fn connect_inner(t: &Target, opts: &ConnectOpts) -> Result<Conn, String> {
    let tcp = TcpStream::connect((t.host.as_str(), t.port)).await.map_err(|e| format!("connect to {}:{}: {e}", t.host, t.port))?;
    let _ = tcp.set_nodelay(true);
    if !t.tls {
        return handshake(tcp, t).await;
    }
    let mut b = native_tls::TlsConnector::builder();
    b.request_alpns(&["h2"]);
    if opts.insecure {
        b.danger_accept_invalid_certs(true);
        b.danger_accept_invalid_hostnames(true);
    }
    let connector = b.build().map_err(|e| format!("TLS setup: {e}"))?;
    let tls = tokio_native_tls::TlsConnector::from(connector)
        .connect(&t.host, tcp)
        .await
        .map_err(|e| format!("TLS handshake with {}: {e}", t.host))?;
    // No ALPN answer: try HTTP/2 anyway. An answer other than h2 means the server cannot do gRPC.
    if let Ok(Some(p)) = tls.get_ref().negotiated_alpn() {
        if p != b"h2" {
            return Err(format!("the server negotiated \"{}\" instead of h2", String::from_utf8_lossy(&p)));
        }
    }
    handshake(tls, t).await
}

async fn handshake<T>(io: T, t: &Target) -> Result<Conn, String>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (send, connection) = h2::client::handshake(io).await.map_err(|e| format!("HTTP/2 handshake: {e}"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let host = if t.host.contains(':') { format!("[{}]", t.host) } else { t.host.clone() };
    let default_port = if t.tls { 443 } else { 80 };
    let authority = if t.port == default_port { host } else { format!("{host}:{}", t.port) };
    Ok(Conn { send, authority, tls: t.tls })
}

// -------------------------------------------------------------------- call

#[derive(Clone, Debug, Default)]
pub struct CallEnd {
    pub status: i32,
    pub message: String,
    pub headers: Vec<(String, String)>,
    pub trailers: Vec<(String, String)>,
}

impl CallEnd {
    pub fn ok(&self) -> bool {
        self.status == 0
    }

    /// "OK", or "NOT_FOUND: no such thing".
    pub fn summary(&self) -> String {
        if self.message.is_empty() {
            status_name(self.status).to_string()
        } else {
            format!("{}: {}", status_name(self.status), self.message)
        }
    }
}

pub fn method_path(service: &str, method: &str) -> String {
    format!("/{service}/{method}")
}

fn headers_vec(h: &http::HeaderMap) -> Vec<(String, String)> {
    h.iter().map(|(k, v)| (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect()
}

fn apply_metadata(mut b: http::request::Builder, md: &[(String, String)]) -> Result<http::request::Builder, String> {
    for (k, v) in md {
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        let lk = k.to_ascii_lowercase();
        // pseudo-headers and anything the protocol owns
        if lk.starts_with(':') || matches!(lk.as_str(), "content-type" | "te" | "host" | "content-length" | "transfer-encoding" | "connection") {
            continue;
        }
        let name = http::HeaderName::from_bytes(lk.as_bytes()).map_err(|_| format!("invalid metadata key \"{k}\""))?;
        let value = http::HeaderValue::from_str(v.trim()).map_err(|_| format!("invalid metadata value for \"{k}\""))?;
        b = b.header(name, value);
    }
    Ok(b)
}

/// One call with a single request message (unary or server streaming).
/// Every response message is handed to `on_message` as it arrives. Dropping the
/// returned future cancels the call. `Err` means the transport failed; a gRPC
/// error status comes back as `Ok(CallEnd)` with `status != 0`.
pub async fn call<F>(conn: &Conn, path: &str, metadata: &[(String, String)], request: &[u8], mut on_message: F) -> Result<CallEnd, String>
where
    F: FnMut(Vec<u8>),
{
    let mut send = conn.send.clone().ready().await.map_err(h2e)?;
    let scheme = if conn.tls { "https" } else { "http" };
    let builder = http::Request::builder()
        .method("POST")
        .uri(format!("{scheme}://{}{path}", conn.authority))
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .header("grpc-accept-encoding", "gzip,identity")
        .header("user-agent", "crabipie");
    let req = apply_metadata(builder, metadata)?.body(()).map_err(|e| format!("bad request: {e}"))?;
    let (response, mut tx) = send.send_request(req, false).map_err(h2e)?;
    tx.send_data(Bytes::from(frame(request)), true).map_err(h2e)?;
    let response = response.await.map_err(h2e)?;
    let (parts, mut body) = response.into_parts();

    if !parts.status.is_success() && !parts.headers.contains_key("grpc-status") {
        return Err(format!("the server answered HTTP {} (not a gRPC response)", parts.status.as_u16()));
    }
    let encoding = parts.headers.get("grpc-encoding").and_then(|v| v.to_str().ok()).unwrap_or("identity").to_ascii_lowercase();

    let mut de = Deframer::default();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(h2e)?;
        body.flow_control().release_capacity(chunk.len()).map_err(h2e)?;
        de.push(&chunk);
        while let Some((compressed, payload)) = de.pop()? {
            let payload = if !compressed {
                payload
            } else if encoding == "gzip" {
                gunzip(&payload)?
            } else {
                return Err(format!("message compressed with unsupported encoding \"{encoding}\""));
            };
            on_message(payload);
        }
    }
    if de.has_partial() {
        return Err("the stream ended in the middle of a message".into());
    }
    let trailers = body.trailers().await.map_err(h2e)?;

    // grpc-status is in the trailers, or in the headers for "trailers-only" replies
    let pick = |name: &str| -> Option<String> {
        trailers
            .as_ref()
            .and_then(|t| t.get(name))
            .or_else(|| parts.headers.get(name))
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
    };
    let (status, mut message) = match pick("grpc-status") {
        Some(s) => (s.trim().parse::<i32>().unwrap_or(2), pick("grpc-message").unwrap_or_default()),
        None => (2, "the server ended the stream without a gRPC status".to_string()),
    };
    // grpc-message is percent-encoded
    let decoded = urlencoding::decode(&message).map(|c| c.into_owned());
    if let Ok(d) = decoded {
        message = d;
    }
    Ok(CallEnd { status, message, headers: headers_vec(&parts.headers), trailers: trailers.as_ref().map(headers_vec).unwrap_or_default() })
}

// -------------------------------------------------------------- reflection

const REFL_V1: &str = "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo";
const REFL_V1ALPHA: &str = "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo";

#[derive(Debug, PartialEq, Eq)]
pub enum Refl {
    Services(Vec<String>),
    Files(Vec<Vec<u8>>),
    Error(i32, String),
}

/// ServerReflectionRequest with one string field: 3 = file_by_filename,
/// 4 = file_containing_symbol, 7 = list_services.
fn refl_request(field: u32, value: &str) -> Vec<u8> {
    let mut b = Vec::new();
    put_len(&mut b, field, value.as_bytes());
    b
}

pub fn parse_reflection_response(bytes: &[u8]) -> Result<Refl, String> {
    let mut r = Reader::new(bytes);
    while let Some((f, w)) = r.tag()? {
        match (f, w) {
            (4, LEN) => {
                let mut files = Vec::new();
                let mut inner = Reader::new(r.bytes()?);
                while let Some((f2, w2)) = inner.tag()? {
                    if f2 == 1 && w2 == LEN {
                        files.push(inner.bytes()?.to_vec());
                    } else {
                        inner.skip(w2)?;
                    }
                }
                return Ok(Refl::Files(files));
            }
            (6, LEN) => {
                let mut names = Vec::new();
                let mut inner = Reader::new(r.bytes()?);
                while let Some((f2, w2)) = inner.tag()? {
                    if f2 == 1 && w2 == LEN {
                        let mut svc = Reader::new(inner.bytes()?);
                        while let Some((f3, w3)) = svc.tag()? {
                            if f3 == 1 && w3 == LEN {
                                names.push(String::from_utf8_lossy(svc.bytes()?).into_owned());
                            } else {
                                svc.skip(w3)?;
                            }
                        }
                    } else {
                        inner.skip(w2)?;
                    }
                }
                return Ok(Refl::Services(names));
            }
            (7, LEN) => {
                let mut code = 0i32;
                let mut msg = String::new();
                let mut inner = Reader::new(r.bytes()?);
                while let Some((f2, w2)) = inner.tag()? {
                    match (f2, w2) {
                        (1, VARINT) => code = inner.varint()? as i64 as i32,
                        (2, LEN) => msg = String::from_utf8_lossy(inner.bytes()?).into_owned(),
                        _ => inner.skip(w2)?,
                    }
                }
                return Ok(Refl::Error(code, msg));
            }
            _ => r.skip(w)?,
        }
    }
    Err("empty reflection response".into())
}

/// Err((true, ..)) = this reflection version is not implemented (try the next one).
async fn reflect_once(conn: &Conn, path: &str, md: &[(String, String)], request: Vec<u8>) -> Result<Refl, (bool, String)> {
    let mut first: Option<Vec<u8>> = None;
    let end = call(conn, path, md, &request, |m| {
        if first.is_none() {
            first = Some(m);
        }
    })
    .await
    .map_err(|e| (false, e))?;
    if end.status == 12 {
        return Err((true, end.summary()));
    }
    if !end.ok() {
        return Err((false, end.summary()));
    }
    let bytes = first.ok_or((false, "empty reflection response".to_string()))?;
    match parse_reflection_response(&bytes).map_err(|e| (false, e))? {
        Refl::Error(code, msg) => Err((false, format!("reflection error {code}: {msg}"))),
        other => Ok(other),
    }
}

async fn discover_via(conn: &Conn, path: &str, md: &[(String, String)]) -> Result<Pool, (bool, String)> {
    let names = match reflect_once(conn, path, md, refl_request(7, "*")).await? {
        Refl::Services(n) => n,
        _ => return Err((false, "unexpected reflection reply".into())),
    };
    let mut pool = Pool::default();
    let mut missing: Vec<String> = Vec::new();
    for name in names.iter().filter(|n| !n.starts_with("grpc.reflection.")) {
        if let Refl::Files(files) = reflect_once(conn, path, md, refl_request(4, name)).await? {
            for f in files {
                missing.extend(pool.add_file(&f).map_err(|e| (false, e))?);
            }
        }
    }
    // Servers normally send a file together with its dependencies; ask for stragglers.
    let mut asked = 0;
    while let Some(dep) = missing.pop() {
        if pool.has_file(&dep) {
            continue;
        }
        asked += 1;
        if asked > 100 {
            break;
        }
        if let Ok(Refl::Files(files)) = reflect_once(conn, path, md, refl_request(3, &dep)).await {
            for f in files {
                missing.extend(pool.add_file(&f).map_err(|e| (false, e))?);
            }
        }
    }
    Ok(pool)
}

/// Ask the server what it serves: lists services and loads their descriptors.
pub async fn discover(conn: &Conn, metadata: &[(String, String)]) -> Result<Pool, String> {
    let mut last = String::new();
    for path in [REFL_V1, REFL_V1ALPHA] {
        match discover_via(conn, path, metadata).await {
            Ok(pool) => return Ok(pool),
            Err((true, e)) => last = e,
            Err((false, e)) => return Err(e),
        }
    }
    Err(format!("this server does not offer gRPC reflection ({last})"))
}

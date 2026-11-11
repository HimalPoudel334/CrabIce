use crabipie_core::*;
use std::collections::HashMap;

fn env_with(vars: &[(&str, &str)]) -> Environment {
    let mut e = Environment::new(1, "dev");
    e.variables = vars
        .iter()
        .map(|(k, v)| EnvVar { key: k.to_string(), value: v.to_string(), enabled: true })
        .collect();
    e
}

#[test]
fn resolve_vars_substitutes_and_keeps_unknown() {
    let e = env_with(&[("host", "api.example.com")]);
    assert_eq!(resolve_vars("https://{{host}}/x", Some(&e)), "https://api.example.com/x");
    assert_eq!(resolve_vars("{{missing}}", Some(&e)), "{{missing}}");
    assert_eq!(resolve_vars("plain", None), "plain");
}

#[test]
fn build_request_defaults_scheme_and_applies_bearer_from_env() {
    let e = env_with(&[("host", "example.com"), ("tok", "abc123")]);
    let mut tab = SavedState::default();
    tab.url = "{{host}}/v1/items".into();
    tab.auth_type = AuthType::Bearer;
    tab.bearer_token = "{{tok}}".into();
    let (rb, url) = build_request(&tab, Some(&e), &HashMap::new());
    assert_eq!(url, "https://example.com/v1/items");
    let req = rb.build().unwrap();
    assert_eq!(req.headers().get("authorization").unwrap(), "Bearer abc123");
}

#[test]
fn build_request_api_key_in_query() {
    let mut tab = SavedState::default();
    tab.url = "https://example.com/a".into();
    tab.auth_type = AuthType::ApiKey;
    tab.api_key_name = "key".into();
    tab.api_key = "s3cret".into();
    tab.api_key_position = ApiKeyPosition::QueryParams;
    let (_rb, url) = build_request(&tab, None, &HashMap::new());
    assert_eq!(url, "https://example.com/a?key=s3cret");
}

#[test]
fn cookie_jar_is_sent_and_updated() {
    let mut jar: HashMap<String, Vec<CookieEntry>> = HashMap::new();
    merge_set_cookies(&mut jar, "https://example.com/login", &["sid=1; Path=/".into()]);
    merge_set_cookies(&mut jar, "https://example.com/login", &["sid=2; Path=/".into()]);
    let dom = extract_domain("https://example.com/login").unwrap();
    assert_eq!(jar[&dom].len(), 1);
    assert_eq!(jar[&dom][0].value, "2");

    let mut tab = SavedState::default();
    tab.url = "https://example.com/me".into();
    let (rb, _) = build_request(&tab, None, &jar);
    assert_eq!(rb.build().unwrap().headers().get("cookie").unwrap(), "sid=2");
}

#[test]
fn headers_raw_roundtrip_keeps_disabled() {
    let hs = vec![
        RequestHeaders { key: "A".into(), value: "1".into(), enabled: true },
        RequestHeaders { key: "B".into(), value: "2".into(), enabled: false },
    ];
    let raw = headers_to_raw(&hs);
    assert_eq!(raw, "A: 1\n# B: 2");
    let back = raw_to_headers(&raw);
    assert_eq!(back.len(), 2);
    assert!(back[0].enabled && !back[1].enabled);
}

#[test]
fn query_params_sync_with_url() {
    let params = parse_url_query("https://x.dev/p?a=1&b=two");
    assert_eq!(params.len(), 2);
    assert_eq!(rebuild_url("https://x.dev/p?old=1", &params), "https://x.dev/p?a=1&b=two");
}

#[test]
fn find_matches_modes() {
    let t = "Foo foo foobar FOO";
    assert_eq!(find_matches(t, "foo", true, false).len(), 2);
    assert_eq!(find_matches(t, "foo", false, false).len(), 4);
    assert_eq!(find_matches(t, "foo", false, true), vec![0, 4, 15]);
    assert_eq!(position_to_line_col("ab\ncd", 3), (1, 0));
    // multibyte text must not panic
    let _ = find_matches("héllo wörld héllo", "héllo", false, true);
}

#[test]
fn preventive_refresh_decision() {
    let mut e = env_with(&[]);
    assert!(!needs_preventive_refresh(AuthType::Bearer, Some(&e)));
    assert!(!needs_preventive_refresh(AuthType::None, Some(&e)));
    assert!(!can_retry_after_auth_failure(AuthType::Bearer, false, None));
    e.auth_config = Some(AuthRefreshConfig { auto_refresh_enabled: true, mode: AuthRefreshMode::ReLogin, ..AuthRefreshConfig::default() });
    // no token stored yet -> refresh needed
    assert!(needs_preventive_refresh(AuthType::Bearer, Some(&e)));
    assert!(can_retry_after_auth_failure(AuthType::Bearer, false, Some(&e)));
    assert!(!can_retry_after_auth_failure(AuthType::Bearer, true, Some(&e)));
    let out = AuthRefreshOutcome { access_token: "t".into(), refresh_token: None, expires_at: Some(i64::MAX) };
    apply_auth_refresh_outcome(&mut e, &out);
    assert!(!needs_preventive_refresh(AuthType::Bearer, Some(&e)));
}

#[test]
fn collection_tree_ops() {
    let mut items = vec![
        CollectionItem::Folder(CollectionFolder { id: 1, name: "f".into(), expanded: false, children: vec![] }),
    ];
    let req = CollectionRequest { id: 2, name: "r".into(), method: HttpMethod::GET, saved_state: SavedState::default() };
    assert!(insert_into(&mut items, Some(1), CollectionItem::Request(req)));
    assert!(find_request(&items, 2).is_some());
    rename_item(&mut items, 2, "renamed".into());
    assert_eq!(find_request(&items, 2).unwrap().name, "renamed");
    assert!(remove_item(&mut items, 2).is_some());
    assert!(find_request(&items, 2).is_none());
    duplicate_item(&mut items, 1, 9);
    assert_eq!(items.len(), 2);
}

#[test]
fn saved_state_gzip_roundtrip_on_disk() {
    let dir = std::env::temp_dir().join("crabipie_test_home");
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("HOME", &dir);
    std::env::set_var("USERPROFILE", &dir);
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    rt.block_on(async {
        let mut s = SavedState::default();
        s.id = 7;
        s.url = "https://roundtrip.test".into();
        s.save().await;
        let back = SavedState::load(7).await.unwrap();
        assert_eq!(back.url, "https://roundtrip.test");
    });
}

// ───────────── engine: Send-ness + end-to-end against a local server ─────────────

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

fn serve_once(response: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(&response);
        }
    });
    format!("http://{addr}/x")
}

fn run_engine(url: &str) -> Vec<ResponseEvent> {
    let mut st = SavedState::default();
    st.url = url.into();
    let (rb, final_url) = build_request(&st, None, &HashMap::new());
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(execute_request(rb, final_url, false, Arc::new(AtomicBool::new(false)), move |e| {
        sink.lock().unwrap().push(e)
    }));
    let v = events.lock().unwrap().clone();
    v
}

#[test]
fn engine_future_is_send_so_the_ui_can_spawn_it() {
    fn is_send<T: Send>(_: &T) {}
    let (rb, url) = build_request(&SavedState::default(), None, &HashMap::new());
    let fut = execute_request(rb, url, false, Arc::new(AtomicBool::new(false)), |_e| {});
    is_send(&fut);
}

#[test]
fn engine_buffered_json_response() {
    let body = r#"{"ok":true}"#;
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let evs = run_engine(&serve_once(resp.into_bytes()));
    assert_eq!(evs.len(), 2);
    match &evs[0] {
        ResponseEvent::Response(r) => {
            assert_eq!(r.status, "200 OK");
            assert_eq!(r.body, body);
            assert!(r.headers.contains("content-type: application/json"));
            assert!(!r.is_streaming && r.response_time.is_some());
        }
        e => panic!("unexpected {e:?}"),
    }
    assert!(matches!(evs[1], ResponseEvent::Done));
}

#[test]
fn engine_streams_chunked_event_stream() {
    let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n8\r\ndata: 1\n\r\n8\r\ndata: 2\n\r\n0\r\n\r\n";
    let evs = run_engine(&serve_once(resp.as_bytes().to_vec()));
    assert!(matches!(&evs[0], ResponseEvent::Response(r) if r.is_streaming));
    let text: String = evs.iter().filter_map(|e| if let ResponseEvent::Chunk(s) = e { Some(s.as_str()) } else { None }).collect();
    assert_eq!(text, "data: 1\ndata: 2\n");
    assert!(matches!(evs.last(), Some(ResponseEvent::Done)));
}

#[test]
fn engine_reports_connection_errors_without_hanging() {
    let evs = run_engine("http://127.0.0.1:1/nothing"); // nothing listens on port 1
    assert_eq!(evs.len(), 1);
    assert!(matches!(&evs[0], ResponseEvent::Response(r) if r.status == "Error" && r.body.starts_with("Request failed")));
}

// ───────────── auth flow: login-first and retry-on-401 against a local server ─────────────

fn auth_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for _ in 0..6 {
            let Ok((mut s, _)) = listener.accept() else { return };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 2048];
            let mut want = None;
            loop {
                let n = s.read(&mut chunk).unwrap_or(0);
                if n == 0 { break; }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(h) = text.find("\r\n\r\n") {
                    let len = text.to_lowercase().split("content-length:").nth(1)
                        .and_then(|r| r.split("\r\n").next()).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    want = Some(h + 4 + len);
                }
                if want.map_or(false, |w| buf.len() >= w) { break; }
            }
            let req = String::from_utf8_lossy(&buf).to_string();
            let (status, body) = if req.starts_with("POST /login") {
                ("200 OK", r#"{"access_token":"T1"}"#)
            } else if req.to_lowercase().contains("authorization: bearer t1") {
                ("200 OK", r#"{"ok":true}"#)
            } else {
                ("401 Unauthorized", r#"{"error":"nope"}"#)
            };
            let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    format!("http://{addr}")
}

fn auth_env(base: &str, token: &str, expires: Option<i64>) -> Environment {
    let mut e = Environment::new(1, "dev");
    e.variables.push(EnvVar { key: "access_token".into(), value: token.into(), enabled: true });
    e.token_expires_at = expires;
    e.auth_config = Some(AuthRefreshConfig {
        mode: AuthRefreshMode::ReLogin,
        auto_refresh_enabled: true,
        login_endpoint: format!("{base}/login"),
        login_body_template: "{}".into(),
        access_token_path: "access_token".into(),
        ..Default::default()
    });
    e
}

fn run_flow(base: &str, env: Environment) -> Vec<FlowEvent> {
    let mut st = SavedState::default();
    st.url = format!("{base}/data");
    st.auth_type = AuthType::Bearer;
    st.bearer_token = "{{access_token}}".into();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(run_request(st, Some(env), HashMap::new(), Arc::new(AtomicBool::new(false)), move |e| {
        sink.lock().unwrap().push(e)
    }));
    let v = events.lock().unwrap().clone();
    v
}

fn final_status(evs: &[FlowEvent]) -> String {
    evs.iter().rev().find_map(|e| match e {
        FlowEvent::Net(ResponseEvent::Response(r)) => Some(r.status.clone()),
        _ => None,
    }).unwrap_or_default()
}

#[test]
fn flow_logs_in_first_when_there_is_no_token() {
    let base = auth_server();
    let evs = run_flow(&base, auth_env(&base, "", None));
    assert!(matches!(&evs[0], FlowEvent::Token(Ok(o)) if o.access_token == "T1"));
    assert_eq!(final_status(&evs), "200 OK");
}

#[test]
fn flow_retries_once_after_401_and_hides_the_first_failure() {
    let base = auth_server();
    // stale token that looks valid (far-future expiry): the server rejects it
    let evs = run_flow(&base, auth_env(&base, "OLD", Some(i64::MAX / 2)));
    let statuses: Vec<_> = evs.iter().filter_map(|e| match e {
        FlowEvent::Net(ResponseEvent::Response(r)) => Some(r.status.clone()), _ => None }).collect();
    assert_eq!(statuses, vec!["200 OK"], "the 401 must not reach the UI");
    assert!(evs.iter().any(|e| matches!(e, FlowEvent::Token(Ok(_)))));
}

#[test]
fn flow_future_is_send_so_the_ui_can_spawn_it() {
    fn is_send<T: Send>(_: &T) {}
    let fut = run_request(SavedState::default(), None, HashMap::new(), Arc::new(AtomicBool::new(false)), |_e| {});
    is_send(&fut);
}

#[test]
fn replace_vars_matches_the_old_regex_on_random_inputs() {
    let re = regex::Regex::new(r"\{\{([\w$:+-]+)\}\}").unwrap();
    let alphabet = ["{", "}", "{{", "}}", "a", "B", "_", "$", ":", "+", "-", " ", "é", "1", "日"];
    let mut seed = 0x2545F4914F6CDD1Du64;
    for _ in 0..20_000 {
        let mut s = String::new();
        seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
        let len = (seed % 14) as usize;
        for _ in 0..len {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            s.push_str(alphabet[(seed % alphabet.len() as u64) as usize]);
        }
        let expected = re.replace_all(&s, "<$1>").to_string();
        let got = replace_vars(&s, |n| Some(format!("<{n}>")));
        assert_eq!(got, expected, "input {s:?}");
        // unresolved names must be kept verbatim
        assert_eq!(replace_vars(&s, |_| None), s);
    }
}

#[test]
fn graphql_requests_post_query_variables_and_operation_as_json() {
    let mut st = SavedState::default();
    st.url = "x.dev/graphql".into();
    st.request_type = RequestType::GraphQL;
    st.graphql_query = "{ me }".into();
    st.graphql_variables = r#"{"id":1}"#.into();
    st.graphql_operation = "Q1".into();
    let (rb, _) = build_request(&st, None, &HashMap::new());
    let req = rb.build().unwrap();
    assert_eq!(req.method(), reqwest::Method::POST);
    let body: serde_json::Value = serde_json::from_slice(req.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["query"], "{ me }");
    assert_eq!(body["variables"]["id"], 1);
    assert_eq!(body["operationName"], "Q1");
}

#[test]
fn ws_payloads_follow_the_chosen_message_type() {
    use WsBinaryMessageType::*;
    use WsMessageType::*;
    assert_eq!(encode_ws_payload(Text, Base64, "hi").unwrap(), WsPayload::Text("hi".into()));
    assert_eq!(encode_ws_payload(HTML, Base64, "<b>x</b>").unwrap(), WsPayload::Text("<b>x</b>".into()));
    assert!(encode_ws_payload(Json, Base64, "{bad").unwrap_err().starts_with("Invalid JSON"));
    assert_eq!(encode_ws_payload(Json, Base64, r#"{"a":1}"#).unwrap(), WsPayload::Text(r#"{"a":1}"#.into()));
    assert_eq!(encode_ws_payload(Binary, Base64, "SGk=").unwrap(), WsPayload::Binary(vec![72, 105]));
    assert_eq!(encode_ws_payload(Binary, HexaDecimal, "0x48 69").unwrap(), WsPayload::Binary(vec![72, 105]));
    assert!(encode_ws_payload(Binary, HexaDecimal, "4").is_err());
    assert!(encode_ws_payload(Binary, HexaDecimal, "zz").is_err());
    assert_eq!(hex_dump(&[1, 2, 255], 2), "01 02 …");
}

const GQL_SCHEMA: &str = r#"{"data":{"__schema":{"queryType":{"name":"Query"},"types":[
 {"name":"Query","kind":"OBJECT","fields":[{"name":"users","type":{"name":null,"kind":"LIST","ofType":{"name":"User","kind":"OBJECT","ofType":null}},"args":[{"name":"first","type":{"name":"Int","kind":"SCALAR","ofType":null}}]}],"inputFields":null},
 {"name":"User","kind":"OBJECT","fields":[{"name":"id","type":{"name":null,"kind":"NON_NULL","ofType":{"name":"Int","kind":"SCALAR","ofType":null}},"args":[]},{"name":"name","type":{"name":"String","kind":"SCALAR","ofType":null},"args":[]}],"inputFields":null},
 {"name":"Int","kind":"SCALAR","fields":null,"inputFields":null},
 {"name":"String","kind":"SCALAR","fields":null,"inputFields":null}]}}}"#;

#[test]
fn graphql_document_includes_selected_fields_arguments_and_variable_declarations() {
    let schema = parse_graphql_schema(GQL_SCHEMA).unwrap();
    let mut sel: std::collections::HashSet<String> = ["Query", "Query.users", "Query.users.id"].iter().map(|s| s.to_string()).collect();
    let doc = build_graphql_document(&schema, &sel);
    assert_eq!(doc, "query {\n  users {\n    id\n  }\n}");
    assert!(variables_template(&schema, &sel).is_none());

    sel.insert("Query.users.arg:first".into());
    let doc = build_graphql_document(&schema, &sel);
    assert!(doc.starts_with("query ($first: Int) {"), "{doc}");
    assert!(doc.contains("users(first: $first) {"), "{doc}");
    assert_eq!(variables_template(&schema, &sel).unwrap(), "{\n  \"first\": null\n}");
}

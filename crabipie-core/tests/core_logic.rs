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

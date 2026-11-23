use crabipie_core::grpc::desc::Pool;
use crabipie_core::grpc::dynmsg;
use crabipie_core::grpc::frame::{frame, Deframer};
use crabipie_core::grpc::wire::*;
use serde_json::json;

// ---- a tiny FileDescriptorProto builder, so tests need no protoc ----

fn put_str(out: &mut Vec<u8>, f: u32, s: &str) {
    put_len(out, f, s.as_bytes());
}

fn put_int(out: &mut Vec<u8>, f: u32, n: u64) {
    put_tag(out, f, VARINT);
    put_varint(out, n);
}

/// FieldDescriptorProto: label 1 = optional, 3 = repeated.
fn field(name: &str, number: u64, label: u64, ty: u64, type_name: &str) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, 1, name);
    put_int(&mut b, 3, number);
    put_int(&mut b, 4, label);
    put_int(&mut b, 5, ty);
    if !type_name.is_empty() {
        put_str(&mut b, 6, type_name);
    }
    b
}

fn message(name: &str, fields: Vec<Vec<u8>>, nested: Vec<Vec<u8>>, map_entry: bool) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, 1, name);
    for f in fields {
        put_len(&mut b, 2, &f);
    }
    for n in nested {
        put_len(&mut b, 3, &n);
    }
    if map_entry {
        let mut opts = Vec::new();
        put_int(&mut opts, 7, 1);
        put_len(&mut b, 7, &opts);
    }
    b
}

/// proto3, package demo:
///   message Inner { double x = 1; bool ok = 2; }
///   message Item { string name = 1; int64 qty = 2; repeated int32 tags = 3; Kind kind = 4;
///                  map<string,string> attrs = 5; bytes blob = 6; Inner inner = 7; }
///   enum Kind { UNKNOWN = 0; BIG = 1; }
///   service Shop { rpc Get(Item) returns (Item); rpc Watch(Item) returns (stream Item); }
fn sample_file() -> Vec<u8> {
    let inner = message("Inner", vec![field("x", 1, 1, 1, ""), field("ok", 2, 1, 8, "")], vec![], false);
    let entry = message("AttrsEntry", vec![field("key", 1, 1, 9, ""), field("value", 2, 1, 9, "")], vec![], true);
    let item = message(
        "Item",
        vec![
            field("name", 1, 1, 9, ""),
            field("qty", 2, 1, 3, ""),
            field("tags", 3, 3, 5, ""),
            field("kind", 4, 1, 14, ".demo.Kind"),
            field("attrs", 5, 3, 11, ".demo.Item.AttrsEntry"),
            field("blob", 6, 1, 12, ""),
            field("inner", 7, 1, 11, ".demo.Inner"),
        ],
        vec![entry],
        false,
    );
    let mut kind = Vec::new();
    put_str(&mut kind, 1, "Kind");
    for (name, n) in [("UNKNOWN", 0u64), ("BIG", 1)] {
        let mut v = Vec::new();
        put_str(&mut v, 1, name);
        put_int(&mut v, 2, n);
        put_len(&mut kind, 2, &v);
    }
    let mut get = Vec::new();
    put_str(&mut get, 1, "Get");
    put_str(&mut get, 2, ".demo.Item");
    put_str(&mut get, 3, ".demo.Item");
    let mut watch = Vec::new();
    put_str(&mut watch, 1, "Watch");
    put_str(&mut watch, 2, ".demo.Item");
    put_str(&mut watch, 3, ".demo.Item");
    put_int(&mut watch, 6, 1);
    let mut svc = Vec::new();
    put_str(&mut svc, 1, "Shop");
    put_len(&mut svc, 2, &get);
    put_len(&mut svc, 2, &watch);

    let mut file = Vec::new();
    put_str(&mut file, 1, "demo.proto");
    put_str(&mut file, 2, "demo");
    put_str(&mut file, 3, "other.proto"); // a dependency that is not loaded
    put_len(&mut file, 4, &inner);
    put_len(&mut file, 4, &item);
    put_len(&mut file, 5, &kind);
    put_len(&mut file, 6, &svc);
    put_str(&mut file, 12, "proto3");
    file
}

fn pool() -> Pool {
    let mut p = Pool::default();
    p.add_file(&sample_file()).unwrap();
    p
}

#[test]
fn varints_round_trip_including_the_edges() {
    let mut b = Vec::new();
    put_varint(&mut b, 300);
    assert_eq!(b, vec![0xac, 0x02]);
    let mut b = Vec::new();
    put_varint(&mut b, u64::MAX);
    assert_eq!(b.len(), 10);
    assert_eq!(Reader::new(&b).varint().unwrap(), u64::MAX);
    assert!(Reader::new(&[0x80]).varint().is_err(), "truncated");
}

#[test]
fn descriptor_pool_finds_services_methods_and_missing_dependencies() {
    let mut p = Pool::default();
    let missing = p.add_file(&sample_file()).unwrap();
    assert_eq!(missing, vec!["other.proto".to_string()]);
    assert!(p.has_file("demo.proto"));
    assert!(p.add_file(&sample_file()).unwrap().is_empty(), "second add is a no-op");
    assert_eq!(p.services().len(), 1);
    let svc = &p.services()[0];
    assert_eq!(svc.full_name, "demo.Shop");
    assert_eq!(svc.methods.len(), 2);
    let watch = p.find_method("demo.Shop", "Watch").unwrap();
    assert!(watch.server_streaming && !watch.client_streaming);
    assert_eq!(watch.input, "demo.Item");
    assert!(!p.find_method("demo.Shop", "Get").unwrap().server_streaming);
    assert!(p.message("demo.Item.AttrsEntry").unwrap().map_entry);
}

#[test]
fn encode_matches_known_bytes() {
    let bytes = dynmsg::encode(&pool(), "demo.Item", &json!({"qty": 150, "name": "hi"})).unwrap();
    // field 1 string "hi", then field 2 varint 150 (0x96 0x01), ordered by field number
    assert_eq!(bytes, vec![0x0a, 0x02, b'h', b'i', 0x10, 0x96, 0x01]);
}

#[test]
fn every_field_type_round_trips() {
    let p = pool();
    let input = json!({
        "name": "hi",
        "qty": "9007199254740993", // beyond 2^53: must survive as a string
        "tags": [1, -2, 300],
        "kind": "BIG",
        "attrs": {"a": "b"},
        "blob": "AQID",
        "inner": {"x": 1.5, "ok": true}
    });
    let bytes = dynmsg::encode(&p, "demo.Item", &input).unwrap();
    assert_eq!(dynmsg::decode(&p, "demo.Item", &bytes).unwrap(), input);
}

#[test]
fn repeated_scalars_are_packed_and_both_forms_decode() {
    let p = pool();
    let bytes = dynmsg::encode(&p, "demo.Item", &json!({"tags": [1, 2, 3]})).unwrap();
    assert_eq!(bytes, vec![0x1a, 0x03, 1, 2, 3], "proto3 packs by default");
    // unpacked elements (field 3, varint) are accepted too
    let v = dynmsg::decode(&p, "demo.Item", &[0x18, 1, 0x18, 2]).unwrap();
    assert_eq!(v, json!({"tags": [1, 2]}));
}

#[test]
fn negative_int32_is_sign_extended_on_the_wire_and_comes_back() {
    let p = pool();
    let bytes = dynmsg::encode(&p, "demo.Item", &json!({"tags": [-1]})).unwrap();
    assert_eq!(bytes.len(), 2 + 10, "tag + len + a 10-byte varint");
    assert_eq!(dynmsg::decode(&p, "demo.Item", &bytes).unwrap(), json!({"tags": [-1]}));
}

#[test]
fn unknown_json_fields_are_rejected_with_their_name() {
    let err = dynmsg::encode(&pool(), "demo.Item", &json!({"nope": 1})).unwrap_err();
    assert!(err.contains("nope"), "{err}");
    let err = dynmsg::encode(&pool(), "demo.Item", &json!({"qty": "abc"})).unwrap_err();
    assert!(err.contains("qty"), "{err}");
}

#[test]
fn map_entries_without_a_key_decode_to_the_default_key() {
    // field 5 entry containing only value (field 2) = "x": key was omitted on the wire
    let v = dynmsg::decode(&pool(), "demo.Item", &[0x2a, 0x03, 0x12, 0x01, b'x']).unwrap();
    assert_eq!(v, json!({"attrs": {"": "x"}}));
}

#[test]
fn unknown_wire_fields_are_shown_not_dropped() {
    // field 99, varint 7
    let mut b = Vec::new();
    put_tag(&mut b, 99, VARINT);
    put_varint(&mut b, 7);
    let v = dynmsg::decode(&pool(), "demo.Item", &b).unwrap();
    assert_eq!(v, json!({"?99": 7}));
}

#[test]
fn skeleton_lists_every_field_with_defaults() {
    let v = dynmsg::skeleton(&pool(), "demo.Item");
    assert_eq!(v["name"], json!(""));
    assert_eq!(v["qty"], json!("0"));
    assert_eq!(v["tags"], json!([]));
    assert_eq!(v["kind"], json!("UNKNOWN"));
    assert_eq!(v["attrs"], json!({}));
    assert_eq!(v["inner"], json!({"x": 0.0, "ok": false}));
    // and the template itself is a valid request
    assert!(dynmsg::encode(&pool(), "demo.Item", &v).is_ok());
}

#[test]
fn frames_survive_arbitrary_chunking() {
    let mut wire = frame(b"abc");
    wire.extend(frame(b""));
    let mut d = Deframer::default();
    let mut got = Vec::new();
    for byte in wire {
        d.push(&[byte]);
        while let Some(m) = d.pop().unwrap() {
            got.push(m);
        }
    }
    assert_eq!(got, vec![(false, b"abc".to_vec()), (false, Vec::new())]);
    assert!(!d.has_partial());

    let mut d = Deframer::default();
    d.push(&frame(b"abcdef")[..4]);
    assert!(d.pop().unwrap().is_none());
    assert!(d.has_partial());
}

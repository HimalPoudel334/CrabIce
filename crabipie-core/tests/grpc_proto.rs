use crabipie_core::grpc::desc::{Kind, Pool};
use crabipie_core::grpc::dynmsg;
use crabipie_core::grpc::proto::load_proto;
use serde_json::json;
use std::cell::Cell;

const SHOP: &str = r#"
syntax = "proto3";
package shop.v1;

import "google/protobuf/timestamp.proto";
import "common.proto";   // found through the finder

// line comment
/* block
   comment */
option go_package = "example.com/shop";

enum Kind {
  KIND_UNSPECIFIED = 0;
  BIG = 1 [deprecated = true];
  reserved 5;
}

message Item {
  string name = 1;
  int64 qty = 2 [json_name = "quantity"];
  repeated int32 tags = 3 [packed = false];
  Kind kind = 4;
  map<string, string> attrs = 5;
  bytes blob = 6;
  Inner inner = 7;
  google.protobuf.Timestamp created = 8;
  common.Money price = 9;
  optional string note = 10;
  oneof choice {
    string a = 11;
    int32 b = 12;
  }
  message Inner { double x = 1; bool ok = 2; Kind k = 3; }
  reserved 20 to 30;
  option deprecated = false;
}

service Shop {
  option (some.opt) = { a: 1, b: "x" };
  rpc Get(Item) returns (Item);
  rpc Watch(Item) returns (stream Item) {
    option (google.api.http) = { get: "/v1/watch" };
  }
  rpc Upload(stream Item) returns (Item);
}
"#;

const COMMON: &str = r#"syntax = "proto3"; package common; message Money { string currency = 1; int64 units = 2; }"#;

fn load(src: &str, extra: &[(&str, &str)]) -> Result<Pool, String> {
    let mut pool = Pool::default();
    let find = |name: &str| extra.iter().find(|(n, _)| *n == name).map(|(_, s)| s.to_string());
    load_proto(&mut pool, "shop.proto", src, &find)?;
    Ok(pool)
}

fn shop() -> Pool {
    load(SHOP, &[("common.proto", COMMON)]).unwrap()
}

#[test]
fn services_streaming_flags_and_field_details_are_parsed() {
    let pool = shop();
    let svc = &pool.services()[0];
    assert_eq!(svc.full_name, "shop.v1.Shop");
    let names: Vec<&str> = svc.methods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Get", "Watch", "Upload"]);
    let get = pool.find_method("shop.v1.Shop", "Get").unwrap();
    assert!(!get.client_streaming && !get.server_streaming);
    assert_eq!((get.input.as_str(), get.output.as_str()), ("shop.v1.Item", "shop.v1.Item"));
    let watch = pool.find_method("shop.v1.Shop", "Watch").unwrap();
    assert!(watch.server_streaming && !watch.client_streaming);
    let upload = pool.find_method("shop.v1.Shop", "Upload").unwrap();
    assert!(upload.client_streaming && !upload.server_streaming);

    let item = pool.message("shop.v1.Item").unwrap();
    let f = |n: &str| item.fields.iter().find(|f| f.name == n).unwrap();
    assert_eq!(f("qty").json_name, "quantity");
    assert_eq!(f("tags").packed, Some(false));
    assert_eq!(f("kind").kind, Kind::Enum);
    assert_eq!(f("kind").type_name, "shop.v1.Kind");
    assert_eq!(f("inner").type_name, "shop.v1.Item.Inner");
    assert_eq!(f("created").type_name, "google.protobuf.Timestamp");
    assert_eq!(f("price").type_name, "common.Money");
    assert!(f("note").proto3_optional);
    assert_eq!((f("a").oneof, f("b").oneof), (Some(0), Some(0)));
    assert_eq!(f("attrs").type_name, "shop.v1.Item.AttrsEntry");
    assert!(pool.message("shop.v1.Item.AttrsEntry").unwrap().map_entry);
    assert!(pool.enum_desc("shop.v1.Kind").is_some());
}

#[test]
fn json_round_trips_through_a_parsed_proto() {
    let pool = shop();
    let input = json!({
        "name": "hi",
        "quantity": "7",
        "tags": [1, 2],
        "kind": "BIG",
        "attrs": {"k": "v"},
        "inner": {"x": 1.5, "ok": true},
        "created": {"seconds": "5", "nanos": 1},
        "price": {"currency": "USD", "units": "3"},
        "note": "n",
        "a": "x"
    });
    let bytes = dynmsg::encode(&pool, "shop.v1.Item", &input).unwrap();
    assert_eq!(dynmsg::decode(&pool, "shop.v1.Item", &bytes).unwrap(), input);

    let sk = dynmsg::skeleton(&pool, "shop.v1.Item");
    assert!(sk.get("a").is_some() && sk.get("b").is_none(), "only the first oneof member is templated");
    assert!(sk.get("note").is_some());
    assert!(dynmsg::encode(&pool, "shop.v1.Item", &sk).is_ok());
}

#[test]
fn proto2_does_not_pack_and_extension_syntax_is_skipped() {
    let src = r#"syntax = "proto2"; package p;
        message M { repeated int32 a = 1; optional string s = 2 [default = "x"]; extensions 100 to 199; }
        extend M { optional int32 ext = 100; }"#;
    let pool = load(src, &[]).unwrap();
    let bytes = dynmsg::encode(&pool, "p.M", &json!({"a": [1, 2]})).unwrap();
    assert_eq!(bytes, vec![0x08, 1, 0x08, 2]);
}

#[test]
fn type_names_resolve_the_way_protoc_does() {
    let src = r#"syntax = "proto3"; package p;
        message Outer { message Inner { } Inner i = 1; enum E { Z = 0; } E e = 2; }
        message Other { Outer.Inner x = 1; .p.Outer y = 2; Outer.E z = 3; }
        service S { rpc Go(Outer) returns (.p.Other); }"#;
    let pool = load(src, &[]).unwrap();
    let field = |m: &str, n: &str| pool.message(m).unwrap().fields.iter().find(|f| f.name == n).unwrap().clone();
    assert_eq!(field("p.Outer", "i").type_name, "p.Outer.Inner");
    assert_eq!((field("p.Outer", "e").kind, field("p.Outer", "e").type_name), (Kind::Enum, "p.Outer.E".to_string()));
    assert_eq!(field("p.Other", "x").type_name, "p.Outer.Inner");
    assert_eq!(field("p.Other", "y").type_name, "p.Outer");
    assert_eq!(field("p.Other", "z").kind, Kind::Enum);
    let go = pool.find_method("p.S", "Go").unwrap();
    assert_eq!((go.input.as_str(), go.output.as_str()), ("p.Outer", "p.Other"));
}

#[test]
fn map_fields_get_protoc_entry_names_and_round_trip() {
    let src = r#"syntax = "proto3";
        message M { map<string, int32> word_counts = 1; map<int32, Item> by_id = 2; }
        message Item { }"#;
    let pool = load(src, &[]).unwrap();
    assert!(pool.message("M.WordCountsEntry").unwrap().map_entry);
    assert!(pool.message("M.ByIdEntry").unwrap().map_entry);
    let input = json!({"wordCounts": {"a": 1}, "byId": {"7": {}}});
    let bytes = dynmsg::encode(&pool, "M", &input).unwrap();
    assert_eq!(dynmsg::decode(&pool, "M", &bytes).unwrap(), input);
}

#[test]
fn errors_name_the_file_and_the_line() {
    let err = load("syntax = \"proto3\";\nmessage A {\n  string x = ;\n}\n", &[]).unwrap_err();
    assert!(err.contains("shop.proto:3"), "{err}");
    let err = load("syntax = \"proto3\";\nmessage A {\n  Missing m = 1;\n}\n", &[]).unwrap_err();
    assert!(err.contains("Missing"), "{err}");
    let err = load("syntax = \"proto3\"; import \"nope.proto\";", &[]).unwrap_err();
    assert!(err.contains("nope.proto"), "{err}");
    let err = load("syntax = \"proto3", &[]).unwrap_err();
    assert!(err.contains("unterminated"), "{err}");
    let err = load("syntax = \"proto3\"; service S { rpc Go(Nope) returns (Nope); }", &[]).unwrap_err();
    assert!(err.contains("Nope"), "{err}");
}

#[test]
fn imports_load_once_and_cycles_do_not_loop() {
    let src = r#"syntax = "proto3"; import "b.proto"; import "b.proto"; message A { B b = 1; }"#;
    let b = r#"syntax = "proto3"; import "shop.proto"; message B { int32 x = 1; }"#;
    let asked = Cell::new(0);
    let find = |name: &str| {
        asked.set(asked.get() + 1);
        (name == "b.proto").then(|| b.to_string())
    };
    let mut pool = Pool::default();
    load_proto(&mut pool, "shop.proto", src, &find).unwrap();
    assert_eq!(asked.get(), 1, "b.proto is fetched once even though it is imported twice and imports back");
    assert_eq!(pool.message("A").unwrap().fields[0].type_name, "B");
}

#[test]
fn standard_google_imports_need_no_files() {
    let src = r#"syntax = "proto3"; package t;
        import "google/protobuf/struct.proto";
        import "google/protobuf/timestamp.proto";
        import "google/protobuf/empty.proto";
        import "google/api/annotations.proto";
        message Doc { google.protobuf.Struct data = 1; google.protobuf.Timestamp at = 2; }
        service Svc { rpc Ping(google.protobuf.Empty) returns (Doc) { option (google.api.http) = { get: "/ping" }; } }"#;
    let pool = load(src, &[]).unwrap();
    assert_eq!(pool.find_method("t.Svc", "Ping").unwrap().input, "google.protobuf.Empty");
    let input = json!({"data": {"fields": {"a": {"stringValue": "x"}}}, "at": {"seconds": "9"}});
    let bytes = dynmsg::encode(&pool, "t.Doc", &input).unwrap();
    assert_eq!(dynmsg::decode(&pool, "t.Doc", &bytes).unwrap(), input);
}

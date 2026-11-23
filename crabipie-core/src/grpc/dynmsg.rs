//! Dynamic protobuf messages: JSON <-> wire bytes, driven by a descriptor Pool.
//!
//! JSON follows the proto3 JSON mapping: 64-bit integers are strings, bytes are
//! base64, enums are names, maps are objects. Field names are accepted as either
//! the original name or the camelCase json name; output uses the json name.
//! Unknown fields on the wire come out as `"?<number>"` so nothing is hidden.

use super::desc::{Field, Kind, Message, Pool};
use super::wire::*;
use serde_json::{Map, Number, Value};
use std::collections::HashSet;

const MAX_DEPTH: usize = 64;

// ---------------------------------------------------------------- shared

fn wire_of(k: Kind) -> u8 {
    match k {
        Kind::Double | Kind::Fixed64 | Kind::SFixed64 => FIXED64,
        Kind::Float | Kind::Fixed32 | Kind::SFixed32 => FIXED32,
        Kind::String | Kind::Bytes | Kind::Message | Kind::Group => LEN,
        _ => VARINT,
    }
}

fn map_entry_of<'p>(pool: &'p Pool, f: &Field) -> Option<&'p Message> {
    if f.repeated && f.kind == Kind::Message {
        pool.message(&f.type_name).filter(|m| m.map_entry)
    } else {
        None
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .or_else(|_| base64::Engine::decode(&base64::engine::general_purpose::STANDARD_NO_PAD, s))
        .map_err(|e| format!("invalid base64: {e}"))
}

fn bad(f: &Field, what: &str) -> String {
    format!("field \"{}\" must be {what}", f.name)
}

// ---------------------------------------------------------------- encode

pub fn encode(pool: &Pool, msg: &str, json: &Value) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    encode_msg(pool, msg, json, &mut out, 0)?;
    Ok(out)
}

fn encode_msg(pool: &Pool, name: &str, json: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("message nesting too deep".into());
    }
    let m = pool.message(name).ok_or_else(|| format!("unknown message type {name}"))?;
    let obj = match json {
        Value::Object(o) => o,
        Value::Null => return Ok(()),
        _ => return Err(format!("expected a JSON object for {name}")),
    };
    let mut items: Vec<(&Field, &Value)> = Vec::with_capacity(obj.len());
    for (key, val) in obj {
        let f = m
            .fields
            .iter()
            .find(|f| f.json_name == *key || f.name == *key)
            .ok_or_else(|| format!("unknown field \"{key}\" in {name}"))?;
        if !val.is_null() {
            items.push((f, val));
        }
    }
    items.sort_by_key(|(f, _)| f.number);
    for (f, val) in items {
        encode_field(pool, m, f, val, out, depth)?;
    }
    Ok(())
}

fn encode_field(pool: &Pool, m: &Message, f: &Field, val: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), String> {
    if !f.repeated {
        return put_value(pool, f, val, out, depth);
    }
    if let Some(entry) = map_entry_of(pool, f) {
        let obj = val.as_object().ok_or_else(|| bad(f, "a JSON object (map)"))?;
        let kf = entry.fields.iter().find(|x| x.number == 1).ok_or("map entry without key")?;
        let vf = entry.fields.iter().find(|x| x.number == 2).ok_or("map entry without value")?;
        for (k, v) in obj {
            let mut e = Vec::new();
            put_value(pool, kf, &Value::String(k.clone()), &mut e, depth + 1)?;
            if !v.is_null() {
                put_value(pool, vf, v, &mut e, depth + 1)?;
            }
            put_len(out, f.number, &e);
        }
        return Ok(());
    }
    let arr = val.as_array().ok_or_else(|| bad(f, "a JSON array"))?;
    if f.kind.packable() && f.packed.unwrap_or(m.proto3) {
        let mut body = Vec::new();
        for v in arr {
            scalar_payload(pool, f, v, &mut body)?;
        }
        if !body.is_empty() {
            put_len(out, f.number, &body);
        }
    } else {
        for v in arr {
            put_value(pool, f, v, out, depth)?;
        }
    }
    Ok(())
}

/// Tag + payload of one element.
fn put_value(pool: &Pool, f: &Field, v: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), String> {
    match f.kind {
        Kind::String => {
            let s = v.as_str().ok_or_else(|| bad(f, "a string"))?;
            put_len(out, f.number, s.as_bytes());
        }
        Kind::Bytes => {
            let s = v.as_str().ok_or_else(|| bad(f, "a base64 string"))?;
            let b = b64_decode(s).map_err(|e| format!("field \"{}\": {e}", f.name))?;
            put_len(out, f.number, &b);
        }
        Kind::Message => {
            let mut sub = Vec::new();
            encode_msg(pool, &f.type_name, v, &mut sub, depth + 1)?;
            put_len(out, f.number, &sub);
        }
        Kind::Group => return Err(format!("field \"{}\": groups are not supported", f.name)),
        _ => {
            put_tag(out, f.number, wire_of(f.kind));
            scalar_payload(pool, f, v, out)?;
        }
    }
    Ok(())
}

/// Payload of one numeric/bool/enum element, without a tag.
fn scalar_payload(pool: &Pool, f: &Field, v: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    match f.kind {
        Kind::Double => put_fixed64(out, as_f64(v, f)?.to_bits()),
        Kind::Float => put_fixed32(out, (as_f64(v, f)? as f32).to_bits()),
        Kind::Int64 => put_varint(out, as_i64(v, f)? as u64),
        Kind::UInt64 => put_varint(out, as_u64(v, f)?),
        Kind::Int32 => put_varint(out, as_i32(v, f)? as i64 as u64),
        Kind::UInt32 => put_varint(out, as_u32(v, f)? as u64),
        Kind::SInt32 => {
            let n = as_i32(v, f)?;
            put_varint(out, ((n << 1) ^ (n >> 31)) as u32 as u64);
        }
        Kind::SInt64 => {
            let n = as_i64(v, f)?;
            put_varint(out, ((n << 1) ^ (n >> 63)) as u64);
        }
        Kind::Bool => put_varint(out, as_bool(v, f)? as u64),
        Kind::Enum => put_varint(out, enum_number(pool, f, v)? as i64 as u64),
        Kind::Fixed32 => put_fixed32(out, as_u32(v, f)?),
        Kind::SFixed32 => put_fixed32(out, as_i32(v, f)? as u32),
        Kind::Fixed64 => put_fixed64(out, as_u64(v, f)?),
        Kind::SFixed64 => put_fixed64(out, as_i64(v, f)? as u64),
        Kind::String | Kind::Bytes | Kind::Message | Kind::Group => {
            return Err(format!("field \"{}\" is not a scalar", f.name));
        }
    }
    Ok(())
}

fn as_i64(v: &Value, f: &Field) -> Result<i64, String> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|x| x.fract() == 0.0 && x.abs() < 9.0e18).map(|x| x as i64))
            .ok_or_else(|| bad(f, "an integer")),
        Value::String(s) => s.trim().parse::<i64>().map_err(|_| bad(f, "an integer")),
        _ => Err(bad(f, "an integer")),
    }
}

fn as_u64(v: &Value, f: &Field) -> Result<u64, String> {
    match v {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|x| x.fract() == 0.0 && *x >= 0.0 && *x < 1.8e19).map(|x| x as u64))
            .ok_or_else(|| bad(f, "a non-negative integer")),
        Value::String(s) => s.trim().parse::<u64>().map_err(|_| bad(f, "a non-negative integer")),
        _ => Err(bad(f, "a non-negative integer")),
    }
}

fn as_i32(v: &Value, f: &Field) -> Result<i32, String> {
    i32::try_from(as_i64(v, f)?).map_err(|_| bad(f, "a 32-bit integer"))
}

fn as_u32(v: &Value, f: &Field) -> Result<u32, String> {
    u32::try_from(as_u64(v, f)?).map_err(|_| bad(f, "an unsigned 32-bit integer"))
}

fn as_f64(v: &Value, f: &Field) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| bad(f, "a number")),
        Value::String(s) => match s.trim() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            t => t.parse::<f64>().map_err(|_| bad(f, "a number")),
        },
        _ => Err(bad(f, "a number")),
    }
}

fn as_bool(v: &Value, f: &Field) -> Result<bool, String> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::String(s) if s == "true" => Ok(true),
        Value::String(s) if s == "false" => Ok(false),
        _ => Err(bad(f, "true or false")),
    }
}

fn enum_number(pool: &Pool, f: &Field, v: &Value) -> Result<i32, String> {
    match v {
        Value::String(s) => {
            if let Some(e) = pool.enum_desc(&f.type_name) {
                if let Some(found) = e.values.iter().find(|x| x.0 == *s) {
                    return Ok(found.1);
                }
            }
            s.trim().parse::<i32>().map_err(|_| format!("field \"{}\": unknown enum value \"{s}\"", f.name))
        }
        Value::Number(_) => as_i32(v, f),
        _ => Err(bad(f, "an enum name or number")),
    }
}

// ---------------------------------------------------------------- decode

pub fn decode(pool: &Pool, msg: &str, bytes: &[u8]) -> Result<Value, String> {
    decode_msg(pool, msg, bytes, 0)
}

fn decode_msg(pool: &Pool, name: &str, bytes: &[u8], depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err("message nesting too deep".into());
    }
    let m = pool.message(name).ok_or_else(|| format!("unknown message type {name}"))?;
    let mut obj: Map<String, Value> = Map::new();
    let mut r = Reader::new(bytes);
    while let Some((num, wire)) = r.tag()? {
        match m.fields.iter().find(|f| f.number == num) {
            Some(f) => decode_field(pool, f, wire, &mut r, &mut obj, depth)?,
            None => {
                let v = unknown_value(&mut r, wire)?;
                obj.insert(format!("?{num}"), v);
            }
        }
    }
    Ok(Value::Object(obj))
}

fn unknown_value(r: &mut Reader, wire: u8) -> Result<Value, String> {
    Ok(match wire {
        VARINT => Value::from(r.varint()?),
        FIXED32 => Value::from(r.fixed32()?),
        FIXED64 => Value::String(r.fixed64()?.to_string()),
        LEN => Value::String(b64(r.bytes()?)),
        w => return Err(format!("unsupported wire type {w}")),
    })
}

fn slot_array(obj: &mut Map<String, Value>, key: String) -> &mut Vec<Value> {
    let slot = obj.entry(key).or_insert_with(|| Value::Array(Vec::new()));
    if !slot.is_array() {
        *slot = Value::Array(Vec::new());
    }
    match slot {
        Value::Array(a) => a,
        _ => unreachable!(),
    }
}

fn decode_field(pool: &Pool, f: &Field, wire: u8, r: &mut Reader, obj: &mut Map<String, Value>, depth: usize) -> Result<(), String> {
    let key = f.json_name.clone();
    if let Some(entry) = map_entry_of(pool, f) {
        if wire != LEN {
            return r.skip(wire);
        }
        let (k, v) = decode_map_entry(pool, entry, r.bytes()?, depth + 1)?;
        let slot = obj.entry(key).or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(o) = slot {
            o.insert(k, v);
        }
        return Ok(());
    }
    if f.repeated && f.kind.packable() && wire == LEN {
        let mut pr = Reader::new(r.bytes()?);
        let arr = slot_array(obj, key);
        while !pr.is_empty() {
            arr.push(read_scalar(pool, f, &mut pr)?);
        }
        return Ok(());
    }
    if wire != wire_of(f.kind) {
        return r.skip(wire); // wrong wire type for this field: ignore it
    }
    let v = read_value(pool, f, r, depth)?;
    if f.repeated {
        slot_array(obj, key).push(v);
    } else {
        obj.insert(key, v);
    }
    Ok(())
}

/// One element after its tag has been consumed.
fn read_value(pool: &Pool, f: &Field, r: &mut Reader, depth: usize) -> Result<Value, String> {
    match f.kind {
        Kind::String => Ok(Value::String(String::from_utf8_lossy(r.bytes()?).into_owned())),
        Kind::Bytes => Ok(Value::String(b64(r.bytes()?))),
        Kind::Message => decode_msg(pool, &f.type_name, r.bytes()?, depth + 1),
        Kind::Group => Err(format!("field \"{}\": groups are not supported", f.name)),
        _ => read_scalar(pool, f, r),
    }
}

fn num_f64(x: f64) -> Value {
    if x.is_finite() {
        Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null)
    } else if x.is_nan() {
        Value::String("NaN".into())
    } else if x > 0.0 {
        Value::String("Infinity".into())
    } else {
        Value::String("-Infinity".into())
    }
}

fn num_f32(x: f32) -> Value {
    if x.is_finite() {
        // via the shortest decimal form, so 1.1f32 shows as 1.1 and not 1.100000023841858
        format!("{x}").parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number).unwrap_or(Value::Null)
    } else {
        num_f64(x as f64)
    }
}

fn enum_json(pool: &Pool, f: &Field, n: i32) -> Value {
    if let Some(e) = pool.enum_desc(&f.type_name) {
        if let Some(found) = e.values.iter().find(|x| x.1 == n) {
            return Value::String(found.0.clone());
        }
    }
    Value::from(n)
}

fn read_scalar(pool: &Pool, f: &Field, r: &mut Reader) -> Result<Value, String> {
    Ok(match f.kind {
        Kind::Double => num_f64(f64::from_bits(r.fixed64()?)),
        Kind::Float => num_f32(f32::from_bits(r.fixed32()?)),
        Kind::Int64 => Value::String((r.varint()? as i64).to_string()),
        Kind::UInt64 => Value::String(r.varint()?.to_string()),
        Kind::Int32 => Value::from(r.varint()? as i64 as i32),
        Kind::UInt32 => Value::from(r.varint()? as u32),
        Kind::SInt32 => {
            let n = r.varint()? as u32;
            Value::from(((n >> 1) as i32) ^ -((n & 1) as i32))
        }
        Kind::SInt64 => {
            let n = r.varint()?;
            Value::String((((n >> 1) as i64) ^ -((n & 1) as i64)).to_string())
        }
        Kind::Bool => Value::Bool(r.varint()? != 0),
        Kind::Enum => enum_json(pool, f, r.varint()? as i64 as i32),
        Kind::Fixed32 => Value::from(r.fixed32()?),
        Kind::SFixed32 => Value::from(r.fixed32()? as i32),
        Kind::Fixed64 => Value::String(r.fixed64()?.to_string()),
        Kind::SFixed64 => Value::String((r.fixed64()? as i64).to_string()),
        Kind::String | Kind::Bytes | Kind::Message | Kind::Group => {
            return Err(format!("field \"{}\" is not a scalar", f.name));
        }
    })
}

fn decode_map_entry(pool: &Pool, entry: &Message, raw: &[u8], depth: usize) -> Result<(String, Value), String> {
    let kf = entry.fields.iter().find(|x| x.number == 1).ok_or("map entry without key")?;
    let vf = entry.fields.iter().find(|x| x.number == 2).ok_or("map entry without value")?;
    // proto3 leaves zero values off the wire, so start from the defaults
    let mut key = match kf.kind {
        Kind::String | Kind::Bytes => String::new(),
        Kind::Bool => "false".to_string(),
        _ => "0".to_string(),
    };
    let mut val = default_value(pool, vf);
    let mut r = Reader::new(raw);
    while let Some((n, w)) = r.tag()? {
        if n == 1 && w == wire_of(kf.kind) {
            key = match read_value(pool, kf, &mut r, depth)? {
                Value::String(s) => s,
                other => other.to_string(),
            };
        } else if n == 2 && w == wire_of(vf.kind) {
            val = read_value(pool, vf, &mut r, depth)?;
        } else {
            r.skip(w)?;
        }
    }
    Ok((key, val))
}

// -------------------------------------------------------------- skeleton

fn default_value(pool: &Pool, f: &Field) -> Value {
    match f.kind {
        Kind::String | Kind::Bytes => Value::String(String::new()),
        Kind::Bool => Value::Bool(false),
        Kind::Double | Kind::Float => Value::from(0.0),
        Kind::Int64 | Kind::UInt64 | Kind::Fixed64 | Kind::SFixed64 | Kind::SInt64 => Value::String("0".into()),
        Kind::Enum => enum_json(pool, f, 0),
        Kind::Message | Kind::Group => Value::Object(Map::new()),
        _ => Value::from(0),
    }
}

/// A request template: every field with its default value, ready to edit.
/// Recursive message types are cut off with `{}`; only the first member of
/// each oneof is listed so the template stays a valid message.
pub fn skeleton(pool: &Pool, msg: &str) -> Value {
    skel(pool, msg, 0, &mut Vec::new())
}

fn skel(pool: &Pool, name: &str, depth: usize, stack: &mut Vec<String>) -> Value {
    let Some(m) = pool.message(name) else {
        return Value::Object(Map::new());
    };
    if depth > MAX_DEPTH || stack.iter().any(|s| s == name) {
        return Value::Object(Map::new());
    }
    stack.push(name.to_string());
    let mut obj = Map::new();
    let mut seen_oneofs: HashSet<u32> = HashSet::new();
    for f in &m.fields {
        if let Some(o) = f.oneof {
            if !f.proto3_optional && !seen_oneofs.insert(o) {
                continue;
            }
        }
        let v = if map_entry_of(pool, f).is_some() {
            Value::Object(Map::new())
        } else if f.repeated {
            Value::Array(Vec::new())
        } else if f.kind == Kind::Message {
            skel(pool, &f.type_name, depth + 1, stack)
        } else {
            default_value(pool, f)
        };
        obj.insert(f.json_name.clone(), v);
    }
    stack.pop();
    Value::Object(obj)
}

//! Descriptor model: messages, enums and services decoded from serialized
//! `FileDescriptorProto` bytes (exactly what gRPC server reflection returns).

use super::wire::{Reader, LEN, VARINT};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Double,
    Float,
    Int64,
    UInt64,
    Int32,
    Fixed64,
    Fixed32,
    Bool,
    String,
    Group,
    Message,
    Bytes,
    UInt32,
    Enum,
    SFixed32,
    SFixed64,
    SInt32,
    SInt64,
}

impl Kind {
    fn from_num(n: u64) -> Option<Kind> {
        Some(match n {
            1 => Kind::Double,
            2 => Kind::Float,
            3 => Kind::Int64,
            4 => Kind::UInt64,
            5 => Kind::Int32,
            6 => Kind::Fixed64,
            7 => Kind::Fixed32,
            8 => Kind::Bool,
            9 => Kind::String,
            10 => Kind::Group,
            11 => Kind::Message,
            12 => Kind::Bytes,
            13 => Kind::UInt32,
            14 => Kind::Enum,
            15 => Kind::SFixed32,
            16 => Kind::SFixed64,
            17 => Kind::SInt32,
            18 => Kind::SInt64,
            _ => return None,
        })
    }

    /// Scalars that may be packed when repeated.
    pub fn packable(self) -> bool {
        !matches!(self, Kind::String | Kind::Bytes | Kind::Message | Kind::Group)
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub json_name: String,
    pub number: u32,
    pub kind: Kind,
    pub repeated: bool,
    /// Fully-qualified name (no leading dot) for Message and Enum fields.
    pub type_name: String,
    pub oneof: Option<u32>,
    pub proto3_optional: bool,
    /// Explicit `[packed = ...]`, if any.
    pub packed: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub full_name: String,
    pub fields: Vec<Field>,
    pub map_entry: bool,
    pub proto3: bool,
}

#[derive(Clone, Debug)]
pub struct EnumDesc {
    pub full_name: String,
    pub values: Vec<(String, i32)>,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub name: String,
    pub input: String,
    pub output: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

#[derive(Clone, Debug)]
pub struct Service {
    pub full_name: String,
    pub methods: Vec<Method>,
}

#[derive(Clone, Debug, Default)]
pub struct Pool {
    messages: HashMap<String, Message>,
    enums: HashMap<String, EnumDesc>,
    services: Vec<Service>,
    files: HashSet<String>,
}

fn utf8(b: &[u8]) -> Result<String, String> {
    String::from_utf8(b.to_vec()).map_err(|e| e.to_string())
}

fn strip_dot(s: &str) -> String {
    s.strip_prefix('.').unwrap_or(s).to_string()
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// protoc's default JSON name: drop underscores, upper-case the next letter.
fn lower_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut up = false;
    for c in name.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

impl Pool {
    pub fn message(&self, full_name: &str) -> Option<&Message> {
        self.messages.get(full_name)
    }

    pub fn enum_desc(&self, full_name: &str) -> Option<&EnumDesc> {
        self.enums.get(full_name)
    }

    pub fn services(&self) -> &[Service] {
        &self.services
    }

    pub fn find_method(&self, service: &str, method: &str) -> Option<&Method> {
        self.services.iter().find(|s| s.full_name == service)?.methods.iter().find(|m| m.name == method)
    }

    pub fn has_file(&self, name: &str) -> bool {
        self.files.contains(name)
    }

    /// Adds one serialized FileDescriptorProto. Returns the names of its
    /// dependencies that are not loaded yet, so a reflection client knows
    /// what else to ask for. Adding a file twice is a no-op.
    pub fn add_file(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        let mut r = Reader::new(bytes);
        let mut name = String::new();
        let mut package = String::new();
        let mut syntax = String::new();
        let mut deps: Vec<String> = Vec::new();
        let mut msgs: Vec<&[u8]> = Vec::new();
        let mut enums: Vec<&[u8]> = Vec::new();
        let mut services: Vec<&[u8]> = Vec::new();
        while let Some((f, w)) = r.tag()? {
            match (f, w) {
                (1, LEN) => name = utf8(r.bytes()?)?,
                (2, LEN) => package = utf8(r.bytes()?)?,
                (3, LEN) => deps.push(utf8(r.bytes()?)?),
                (4, LEN) => msgs.push(r.bytes()?),
                (5, LEN) => enums.push(r.bytes()?),
                (6, LEN) => services.push(r.bytes()?),
                (12, LEN) => syntax = utf8(r.bytes()?)?,
                _ => r.skip(w)?,
            }
        }
        if !name.is_empty() && !self.files.insert(name) {
            return Ok(Vec::new());
        }
        let proto3 = syntax == "proto3";
        for m in msgs {
            self.add_message(m, &package, proto3)?;
        }
        for e in enums {
            self.add_enum(e, &package)?;
        }
        for s in services {
            self.add_service(s, &package)?;
        }
        Ok(deps.into_iter().filter(|d| !self.files.contains(d)).collect())
    }

    fn add_message(&mut self, bytes: &[u8], prefix: &str, proto3: bool) -> Result<(), String> {
        let mut r = Reader::new(bytes);
        let mut name = String::new();
        let mut fields_raw: Vec<&[u8]> = Vec::new();
        let mut nested: Vec<&[u8]> = Vec::new();
        let mut enums: Vec<&[u8]> = Vec::new();
        let mut map_entry = false;
        while let Some((f, w)) = r.tag()? {
            match (f, w) {
                (1, LEN) => name = utf8(r.bytes()?)?,
                (2, LEN) => fields_raw.push(r.bytes()?),
                (3, LEN) => nested.push(r.bytes()?),
                (4, LEN) => enums.push(r.bytes()?),
                (7, LEN) => map_entry = parse_map_entry(r.bytes()?)?,
                _ => r.skip(w)?,
            }
        }
        let full = join(prefix, &name);
        let mut fields = Vec::with_capacity(fields_raw.len());
        for fr in fields_raw {
            fields.push(parse_field(fr)?);
        }
        for n in nested {
            self.add_message(n, &full, proto3)?;
        }
        for e in enums {
            self.add_enum(e, &full)?;
        }
        self.messages.insert(full.clone(), Message { full_name: full, fields, map_entry, proto3 });
        Ok(())
    }

    fn add_enum(&mut self, bytes: &[u8], prefix: &str) -> Result<(), String> {
        let mut r = Reader::new(bytes);
        let mut name = String::new();
        let mut values: Vec<(String, i32)> = Vec::new();
        while let Some((f, w)) = r.tag()? {
            match (f, w) {
                (1, LEN) => name = utf8(r.bytes()?)?,
                (2, LEN) => values.push(parse_enum_value(r.bytes()?)?),
                _ => r.skip(w)?,
            }
        }
        let full = join(prefix, &name);
        self.enums.insert(full.clone(), EnumDesc { full_name: full, values });
        Ok(())
    }

    fn add_service(&mut self, bytes: &[u8], prefix: &str) -> Result<(), String> {
        let mut r = Reader::new(bytes);
        let mut name = String::new();
        let mut methods: Vec<Method> = Vec::new();
        while let Some((f, w)) = r.tag()? {
            match (f, w) {
                (1, LEN) => name = utf8(r.bytes()?)?,
                (2, LEN) => methods.push(parse_method(r.bytes()?)?),
                _ => r.skip(w)?,
            }
        }
        self.services.push(Service { full_name: join(prefix, &name), methods });
        Ok(())
    }
}

fn parse_map_entry(bytes: &[u8]) -> Result<bool, String> {
    let mut r = Reader::new(bytes);
    let mut v = false;
    while let Some((f, w)) = r.tag()? {
        if f == 7 && w == VARINT {
            v = r.varint()? != 0;
        } else {
            r.skip(w)?;
        }
    }
    Ok(v)
}

fn parse_packed(bytes: &[u8]) -> Result<Option<bool>, String> {
    let mut r = Reader::new(bytes);
    let mut v = None;
    while let Some((f, w)) = r.tag()? {
        if f == 2 && w == VARINT {
            v = Some(r.varint()? != 0);
        } else {
            r.skip(w)?;
        }
    }
    Ok(v)
}

fn parse_field(bytes: &[u8]) -> Result<Field, String> {
    let mut r = Reader::new(bytes);
    let mut name = String::new();
    let mut json_name = String::new();
    let mut type_name = String::new();
    let mut number = 0u32;
    let mut label = 1u64;
    let mut ty = 0u64;
    let mut oneof = None;
    let mut proto3_optional = false;
    let mut packed = None;
    while let Some((f, w)) = r.tag()? {
        match (f, w) {
            (1, LEN) => name = utf8(r.bytes()?)?,
            (3, VARINT) => number = r.varint()? as u32,
            (4, VARINT) => label = r.varint()?,
            (5, VARINT) => ty = r.varint()?,
            (6, LEN) => type_name = strip_dot(&utf8(r.bytes()?)?),
            (8, LEN) => packed = parse_packed(r.bytes()?)?,
            (9, VARINT) => oneof = Some(r.varint()? as u32),
            (10, LEN) => json_name = utf8(r.bytes()?)?,
            (17, VARINT) => proto3_optional = r.varint()? != 0,
            _ => r.skip(w)?,
        }
    }
    let kind = Kind::from_num(ty).ok_or_else(|| format!("field {name}: unknown type {ty}"))?;
    if json_name.is_empty() {
        json_name = lower_camel(&name);
    }
    Ok(Field { name, json_name, number, kind, repeated: label == 3, type_name, oneof, proto3_optional, packed })
}

fn parse_enum_value(bytes: &[u8]) -> Result<(String, i32), String> {
    let mut r = Reader::new(bytes);
    let mut name = String::new();
    let mut number = 0i32;
    while let Some((f, w)) = r.tag()? {
        match (f, w) {
            (1, LEN) => name = utf8(r.bytes()?)?,
            (2, VARINT) => number = r.varint()? as i64 as i32,
            _ => r.skip(w)?,
        }
    }
    Ok((name, number))
}

fn parse_method(bytes: &[u8]) -> Result<Method, String> {
    let mut r = Reader::new(bytes);
    let mut m = Method { name: String::new(), input: String::new(), output: String::new(), client_streaming: false, server_streaming: false };
    while let Some((f, w)) = r.tag()? {
        match (f, w) {
            (1, LEN) => m.name = utf8(r.bytes()?)?,
            (2, LEN) => m.input = strip_dot(&utf8(r.bytes()?)?),
            (3, LEN) => m.output = strip_dot(&utf8(r.bytes()?)?),
            (5, VARINT) => m.client_streaming = r.varint()? != 0,
            (6, VARINT) => m.server_streaming = r.varint()? != 0,
            _ => r.skip(w)?,
        }
    }
    Ok(m)
}

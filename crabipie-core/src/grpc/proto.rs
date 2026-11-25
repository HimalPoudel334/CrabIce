//! A small `.proto` parser (proto2 and proto3 subset) that fills a descriptor
//! `Pool`, for servers that do not offer reflection. Understood: syntax,
//! package, imports, messages (nested, maps, oneofs, optional/repeated, field
//! options json_name and packed), enums, services with streaming rpcs.
//! Skipped without complaint: other options, reserved, extensions, extend blocks.

use super::desc::{lower_camel, EnumDesc, Field, Kind, Message, Method, Pool, Service};
use std::collections::{HashMap, HashSet};

// ------------------------------------------------------------------ lexer

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(String),
    Str(String),
    Sym(char),
}

fn lex(file: &str, src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            loop {
                if i + 1 >= chars.len() {
                    return Err(format!("{file}:{line}: unterminated /* comment"));
                }
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((Tok::Ident(chars[start..i].iter().collect()), line));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            out.push((Tok::Num(chars[start..i].iter().collect()), line));
        } else if c == '"' || c == '\'' {
            i += 1;
            let mut s = String::new();
            loop {
                let Some(&ch) = chars.get(i) else {
                    return Err(format!("{file}:{line}: unterminated string"));
                };
                i += 1;
                if ch == c {
                    break;
                }
                if ch == '\n' {
                    return Err(format!("{file}:{line}: unterminated string"));
                }
                if ch == '\\' {
                    let Some(&e) = chars.get(i) else {
                        return Err(format!("{file}:{line}: unterminated string"));
                    };
                    i += 1;
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                } else {
                    s.push(ch);
                }
            }
            out.push((Tok::Str(s), line));
        } else {
            out.push((Tok::Sym(c), line));
            i += 1;
        }
    }
    Ok(out)
}

fn parse_int(s: &str) -> Option<i64> {
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).ok()
    } else if s.len() > 1 && s.starts_with('0') {
        i64::from_str_radix(&s[1..], 8).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

// -------------------------------------------------------------------- AST

#[derive(Clone, Debug)]
enum TypeRef {
    Scalar(Kind),
    Named(String),
}

#[derive(Clone, Debug)]
struct FieldAst {
    name: String,
    number: u32,
    ty: TypeRef,
    repeated: bool,
    oneof: Option<u32>,
    proto3_optional: bool,
    packed: Option<bool>,
    json_name: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct MsgAst {
    name: String,
    fields: Vec<FieldAst>,
    nested: Vec<MsgAst>,
    enums: Vec<EnumAst>,
    map_entry: bool,
}

#[derive(Clone, Debug)]
struct EnumAst {
    name: String,
    values: Vec<(String, i32)>,
}

#[derive(Clone, Debug)]
struct RpcAst {
    name: String,
    input: String,
    output: String,
    client_streaming: bool,
    server_streaming: bool,
}

#[derive(Clone, Debug)]
struct SvcAst {
    name: String,
    rpcs: Vec<RpcAst>,
}

#[derive(Clone, Debug, Default)]
struct FileAst {
    name: String,
    package: String,
    proto3: bool,
    imports: Vec<String>,
    messages: Vec<MsgAst>,
    enums: Vec<EnumAst>,
    services: Vec<SvcAst>,
}

fn scalar(name: &str) -> Option<Kind> {
    Some(match name {
        "double" => Kind::Double,
        "float" => Kind::Float,
        "int32" => Kind::Int32,
        "int64" => Kind::Int64,
        "uint32" => Kind::UInt32,
        "uint64" => Kind::UInt64,
        "sint32" => Kind::SInt32,
        "sint64" => Kind::SInt64,
        "fixed32" => Kind::Fixed32,
        "fixed64" => Kind::Fixed64,
        "sfixed32" => Kind::SFixed32,
        "sfixed64" => Kind::SFixed64,
        "bool" => Kind::Bool,
        "string" => Kind::String,
        "bytes" => Kind::Bytes,
        _ => return None,
    })
}

/// protoc names a map field's entry type after the field: word_counts -> WordCountsEntry.
fn entry_name(field: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in field.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out.push_str("Entry");
    out
}

#[derive(Default)]
struct FieldOpts {
    packed: Option<bool>,
    json_name: Option<String>,
}

// ----------------------------------------------------------------- parser

struct Parser<'a> {
    file: &'a str,
    toks: Vec<(Tok, usize)>,
    pos: usize,
    proto3: bool,
}

impl<'a> Parser<'a> {
    fn line(&self) -> usize {
        self.toks.get(self.pos).or(self.toks.last()).map_or(1, |t| t.1)
    }

    fn err<T>(&self, msg: &str) -> Result<T, String> {
        Err(format!("{}:{}: {msg}", self.file, self.line()))
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.0)
    }

    fn peek_sym(&self, c: char) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == c)
    }

    fn eat_sym(&mut self, c: char) -> bool {
        if self.peek_sym(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, c: char) -> Result<(), String> {
        if self.eat_sym(c) {
            Ok(())
        } else {
            self.err(&format!("expected '{c}'"))
        }
    }

    fn eat_ident(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(w)) if w == kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Result<String, String> {
        match self.peek().cloned() {
            Some(Tok::Ident(s)) => {
                self.pos += 1;
                Ok(s)
            }
            _ => self.err("expected a name"),
        }
    }

    /// `a.b.C`, optionally with a leading dot.
    fn full_ident(&mut self) -> Result<String, String> {
        let mut s = String::new();
        if self.eat_sym('.') {
            s.push('.');
        }
        s.push_str(&self.ident()?);
        while self.peek_sym('.') {
            self.pos += 1;
            s.push('.');
            s.push_str(&self.ident()?);
        }
        Ok(s)
    }

    fn string(&mut self) -> Result<String, String> {
        match self.peek().cloned() {
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(s)
            }
            _ => self.err("expected a quoted string"),
        }
    }

    fn int(&mut self) -> Result<i64, String> {
        let neg = self.eat_sym('-');
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                let Some(v) = parse_int(&n) else {
                    return self.err(&format!("bad number \"{n}\""));
                };
                self.pos += 1;
                Ok(if neg { -v } else { v })
            }
            _ => self.err("expected a number"),
        }
    }

    fn field_number(&mut self) -> Result<u32, String> {
        let n = self.int()?;
        match u32::try_from(n) {
            Ok(v) if v > 0 => Ok(v),
            _ => self.err("field numbers must be positive"),
        }
    }

    /// Skips tokens up to and including the next `;` outside any brackets.
    fn skip_statement(&mut self) -> Result<(), String> {
        let mut depth = 0i32;
        loop {
            let Some(tok) = self.peek().cloned() else {
                return self.err("unexpected end of file");
            };
            self.pos += 1;
            if let Tok::Sym(c) = tok {
                match c {
                    '{' | '(' | '[' => depth += 1,
                    '}' | ')' | ']' => depth -= 1,
                    ';' if depth <= 0 => return Ok(()),
                    _ => {}
                }
            }
        }
    }

    /// Skips up to the next `{` and then the whole balanced block.
    fn skip_block(&mut self) -> Result<(), String> {
        while !self.peek_sym('{') {
            if self.peek().is_none() {
                return self.err("unexpected end of file");
            }
            self.pos += 1;
        }
        let mut depth = 0i32;
        loop {
            let Some(tok) = self.peek().cloned() else {
                return self.err("unexpected end of file");
            };
            self.pos += 1;
            if let Tok::Sym(c) = tok {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Skips an option value up to a `,` or `]` at nesting depth 0 (not consumed).
    fn skip_value(&mut self) -> Result<(), String> {
        let mut depth = 0i32;
        loop {
            match self.peek() {
                None => return self.err("unexpected end of file in an option"),
                Some(Tok::Sym(c)) => {
                    let c = *c;
                    if depth == 0 && (c == ',' || c == ']') {
                        return Ok(());
                    }
                    if c == '{' || c == '(' || c == '[' {
                        depth += 1;
                    } else if c == '}' || c == ')' || c == ']' {
                        depth -= 1;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
    }

    fn skip_bracketed(&mut self) -> Result<(), String> {
        self.expect_sym('[')?;
        self.skip_value()?;
        // skip_value stops at a top-level ',' or ']': keep going until the closing bracket
        while self.eat_sym(',') {
            self.skip_value()?;
        }
        self.expect_sym(']')
    }

    fn file(&mut self) -> Result<FileAst, String> {
        let mut f = FileAst { name: self.file.to_string(), ..FileAst::default() };
        while let Some(tok) = self.peek().cloned() {
            match tok {
                Tok::Sym(';') => self.pos += 1,
                Tok::Ident(w) => {
                    self.pos += 1;
                    match w.as_str() {
                        "syntax" => {
                            self.expect_sym('=')?;
                            let s = self.string()?;
                            self.expect_sym(';')?;
                            self.proto3 = s == "proto3";
                            f.proto3 = self.proto3;
                        }
                        "edition" => {
                            self.proto3 = true; // editions behave like proto3 for what we need
                            f.proto3 = true;
                            self.skip_statement()?;
                        }
                        "package" => {
                            f.package = self.full_ident()?;
                            self.expect_sym(';')?;
                        }
                        "import" => {
                            if !self.eat_ident("public") {
                                self.eat_ident("weak");
                            }
                            f.imports.push(self.string()?);
                            self.expect_sym(';')?;
                        }
                        "option" => self.skip_statement()?,
                        "message" => {
                            let m = self.message()?;
                            f.messages.push(m);
                        }
                        "enum" => {
                            let e = self.enum_def()?;
                            f.enums.push(e);
                        }
                        "service" => {
                            let s = self.service()?;
                            f.services.push(s);
                        }
                        "extend" => self.skip_block()?,
                        other => {
                            self.pos -= 1;
                            return self.err(&format!("unexpected \"{other}\""));
                        }
                    }
                }
                _ => return self.err("unexpected token"),
            }
        }
        Ok(f)
    }

    fn message(&mut self) -> Result<MsgAst, String> {
        let name = self.ident()?;
        self.expect_sym('{')?;
        let mut m = MsgAst { name, ..MsgAst::default() };
        let mut oneofs = 0u32;
        loop {
            match self.peek().cloned() {
                None => return self.err("unexpected end of file inside a message"),
                Some(Tok::Sym('}')) => {
                    self.pos += 1;
                    return Ok(m);
                }
                Some(Tok::Sym(';')) => self.pos += 1,
                Some(Tok::Ident(w)) => match w.as_str() {
                    "message" => {
                        self.pos += 1;
                        let n = self.message()?;
                        m.nested.push(n);
                    }
                    "enum" => {
                        self.pos += 1;
                        let e = self.enum_def()?;
                        m.enums.push(e);
                    }
                    "oneof" => {
                        self.pos += 1;
                        self.ident()?;
                        self.expect_sym('{')?;
                        let idx = oneofs;
                        oneofs += 1;
                        loop {
                            if self.eat_sym('}') {
                                break;
                            }
                            if self.eat_sym(';') {
                                continue;
                            }
                            if self.eat_ident("option") {
                                self.pos -= 1;
                                self.skip_statement()?;
                                continue;
                            }
                            if self.peek().is_none() {
                                return self.err("unexpected end of file inside a oneof");
                            }
                            self.field(&mut m, Some(idx))?;
                        }
                    }
                    "option" | "reserved" | "extensions" => self.skip_statement()?,
                    "extend" => {
                        self.pos += 1;
                        self.skip_block()?;
                    }
                    _ => self.field(&mut m, None)?,
                },
                // a field whose type is written with a leading dot: `.pkg.Message name = 1;`
                Some(Tok::Sym('.')) => self.field(&mut m, None)?,
                Some(_) => return self.err("unexpected token in a message"),
            }
        }
    }

    fn type_ref(&mut self) -> Result<TypeRef, String> {
        if let Some(Tok::Ident(w)) = self.peek() {
            if let Some(k) = scalar(w) {
                self.pos += 1;
                return Ok(TypeRef::Scalar(k));
            }
        }
        Ok(TypeRef::Named(self.full_ident()?))
    }

    fn field_options(&mut self) -> Result<FieldOpts, String> {
        let mut o = FieldOpts::default();
        self.expect_sym('[')?;
        loop {
            let mut name = String::new();
            while !self.peek_sym('=') {
                match self.peek().cloned() {
                    Some(Tok::Ident(s)) => name.push_str(&s),
                    Some(Tok::Sym(c)) if c == '.' || c == '(' || c == ')' => name.push(c),
                    _ => return self.err("expected an option name and a value"),
                }
                self.pos += 1;
            }
            self.expect_sym('=')?;
            match name.as_str() {
                "packed" => {
                    let v = self.ident()?;
                    o.packed = Some(v == "true");
                }
                "json_name" => o.json_name = Some(self.string()?),
                _ => self.skip_value()?,
            }
            if self.eat_sym(',') {
                continue;
            }
            self.expect_sym(']')?;
            return Ok(o);
        }
    }

    /// One field (the label and type are still ahead); `oneof` is set inside a oneof block.
    fn field(&mut self, m: &mut MsgAst, oneof: Option<u32>) -> Result<(), String> {
        let mut repeated = false;
        let mut optional = false;
        if oneof.is_none() {
            if self.eat_ident("repeated") {
                repeated = true;
            } else if self.eat_ident("optional") {
                optional = true;
            } else {
                self.eat_ident("required");
            }
        }
        if matches!(self.peek(), Some(Tok::Ident(w)) if w == "group") {
            return self.err("groups are not supported");
        }
        if matches!(self.peek(), Some(Tok::Ident(w)) if w == "map") && matches!(self.peek_at(1), Some(Tok::Sym('<'))) {
            self.pos += 2;
            let key = self.type_ref()?;
            self.expect_sym(',')?;
            let val = self.type_ref()?;
            self.expect_sym('>')?;
            let name = self.ident()?;
            self.expect_sym('=')?;
            let number = self.field_number()?;
            let opts = if self.peek_sym('[') { self.field_options()? } else { FieldOpts::default() };
            self.expect_sym(';')?;
            let entry = entry_name(&name);
            let plain = |name: &str, number: u32, ty: TypeRef| FieldAst { name: name.to_string(), number, ty, repeated: false, oneof: None, proto3_optional: false, packed: None, json_name: None };
            m.nested.push(MsgAst { name: entry.clone(), fields: vec![plain("key", 1, key), plain("value", 2, val)], nested: Vec::new(), enums: Vec::new(), map_entry: true });
            m.fields.push(FieldAst { name, number, ty: TypeRef::Named(entry), repeated: true, oneof: None, proto3_optional: false, packed: None, json_name: opts.json_name });
            return Ok(());
        }
        let ty = self.type_ref()?;
        let name = self.ident()?;
        self.expect_sym('=')?;
        let number = self.field_number()?;
        let opts = if self.peek_sym('[') { self.field_options()? } else { FieldOpts::default() };
        self.expect_sym(';')?;
        m.fields.push(FieldAst { name, number, ty, repeated, oneof, proto3_optional: optional && self.proto3, packed: opts.packed, json_name: opts.json_name });
        Ok(())
    }

    fn enum_def(&mut self) -> Result<EnumAst, String> {
        let name = self.ident()?;
        self.expect_sym('{')?;
        let mut values = Vec::new();
        loop {
            match self.peek().cloned() {
                None => return self.err("unexpected end of file inside an enum"),
                Some(Tok::Sym('}')) => {
                    self.pos += 1;
                    return Ok(EnumAst { name, values });
                }
                Some(Tok::Sym(';')) => self.pos += 1,
                Some(Tok::Ident(w)) if w == "option" || w == "reserved" => self.skip_statement()?,
                Some(Tok::Ident(_)) => {
                    let vname = self.ident()?;
                    self.expect_sym('=')?;
                    let n = self.int()?;
                    if self.peek_sym('[') {
                        self.skip_bracketed()?;
                    }
                    self.expect_sym(';')?;
                    values.push((vname, n as i32));
                }
                Some(_) => return self.err("unexpected token in an enum"),
            }
        }
    }

    fn service(&mut self) -> Result<SvcAst, String> {
        let name = self.ident()?;
        self.expect_sym('{')?;
        let mut rpcs = Vec::new();
        loop {
            match self.peek().cloned() {
                None => return self.err("unexpected end of file inside a service"),
                Some(Tok::Sym('}')) => {
                    self.pos += 1;
                    return Ok(SvcAst { name, rpcs });
                }
                Some(Tok::Sym(';')) => self.pos += 1,
                Some(Tok::Ident(w)) if w == "option" => self.skip_statement()?,
                Some(Tok::Ident(w)) if w == "rpc" => {
                    self.pos += 1;
                    let rname = self.ident()?;
                    self.expect_sym('(')?;
                    let client_streaming = self.eat_ident("stream");
                    let input = self.full_ident()?;
                    self.expect_sym(')')?;
                    if !self.eat_ident("returns") {
                        return self.err("expected 'returns'");
                    }
                    self.expect_sym('(')?;
                    let server_streaming = self.eat_ident("stream");
                    let output = self.full_ident()?;
                    self.expect_sym(')')?;
                    if self.peek_sym('{') {
                        self.skip_block()?;
                        self.eat_sym(';');
                    } else {
                        self.expect_sym(';')?;
                    }
                    rpcs.push(RpcAst { name: rname, input, output, client_streaming, server_streaming });
                }
                Some(_) => return self.err("unexpected token in a service"),
            }
        }
    }
}

fn parse(file: &str, src: &str) -> Result<FileAst, String> {
    let toks = lex(file, src)?;
    Parser { file, toks, pos: 0, proto3: false }.file()
}

// ------------------------------------------------------------ resolution

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

fn collect(prefix: &str, msgs: &[MsgAst], enums: &[EnumAst], out: &mut HashMap<String, bool>) {
    for e in enums {
        out.insert(join(prefix, &e.name), true);
    }
    for m in msgs {
        let full = join(prefix, &m.name);
        out.insert(full.clone(), false);
        collect(&full, &m.nested, &m.enums, out);
    }
}

/// Every known type name -> is it an enum. Types of this load plus whatever the pool already has.
struct Types<'p> {
    local: HashMap<String, bool>,
    pool: &'p Pool,
}

impl Types<'_> {
    fn lookup(&self, name: &str) -> Option<bool> {
        if let Some(e) = self.local.get(name) {
            return Some(*e);
        }
        if self.pool.message(name).is_some() {
            return Some(false);
        }
        if self.pool.enum_desc(name).is_some() {
            return Some(true);
        }
        None
    }

    /// protobuf scoping: try the innermost scope first, then each enclosing one.
    fn resolve(&self, scope: &str, name: &str) -> Option<(String, bool)> {
        if let Some(abs) = name.strip_prefix('.') {
            return self.lookup(abs).map(|e| (abs.to_string(), e));
        }
        let mut scope = scope.to_string();
        loop {
            let cand = join(&scope, name);
            if let Some(is_enum) = self.lookup(&cand) {
                return Some((cand, is_enum));
            }
            if scope.is_empty() {
                return None;
            }
            match scope.rfind('.') {
                Some(i) => scope.truncate(i),
                None => scope.clear(),
            }
        }
    }
}

fn build_message(file: &str, ast: &MsgAst, prefix: &str, proto3: bool, types: &Types, msgs: &mut Vec<Message>, enums: &mut Vec<EnumDesc>) -> Result<(), String> {
    let full = join(prefix, &ast.name);
    let mut fields = Vec::with_capacity(ast.fields.len());
    for f in &ast.fields {
        let (kind, type_name) = match &f.ty {
            TypeRef::Scalar(k) => (*k, String::new()),
            TypeRef::Named(n) => {
                let (t, is_enum) = types.resolve(&full, n).ok_or_else(|| format!("{file}: unknown type \"{n}\" in {full}.{}", f.name))?;
                (if is_enum { Kind::Enum } else { Kind::Message }, t)
            }
        };
        fields.push(Field {
            name: f.name.clone(),
            json_name: f.json_name.clone().unwrap_or_else(|| lower_camel(&f.name)),
            number: f.number,
            kind,
            repeated: f.repeated,
            type_name,
            oneof: f.oneof,
            proto3_optional: f.proto3_optional,
            packed: f.packed,
        });
    }
    msgs.push(Message { full_name: full.clone(), fields, map_entry: ast.map_entry, proto3 });
    for n in &ast.nested {
        build_message(file, n, &full, proto3, types, msgs, enums)?;
    }
    for e in &ast.enums {
        enums.push(EnumDesc { full_name: join(&full, &e.name), values: e.values.clone() });
    }
    Ok(())
}

// --------------------------------------------------------------- builtins

const FILE_HEAD: &str = "syntax = \"proto3\"; package google.protobuf; ";

/// The standard imports, so protos that use Timestamp and friends load without extra files.
fn builtin(path: &str) -> Option<String> {
    let body = match path {
        "google/protobuf/timestamp.proto" => "message Timestamp { int64 seconds = 1; int32 nanos = 2; }",
        "google/protobuf/duration.proto" => "message Duration { int64 seconds = 1; int32 nanos = 2; }",
        "google/protobuf/empty.proto" => "message Empty {}",
        "google/protobuf/any.proto" => "message Any { string type_url = 1; bytes value = 2; }",
        "google/protobuf/field_mask.proto" => "message FieldMask { repeated string paths = 1; }",
        "google/protobuf/wrappers.proto" => {
            "message DoubleValue { double value = 1; } message FloatValue { float value = 1; } \
             message Int64Value { int64 value = 1; } message UInt64Value { uint64 value = 1; } \
             message Int32Value { int32 value = 1; } message UInt32Value { uint32 value = 1; } \
             message BoolValue { bool value = 1; } message StringValue { string value = 1; } \
             message BytesValue { bytes value = 1; }"
        }
        "google/protobuf/struct.proto" => {
            "message Struct { map<string, Value> fields = 1; } \
             message Value { oneof kind { NullValue null_value = 1; double number_value = 2; string string_value = 3; \
             bool bool_value = 4; Struct struct_value = 5; ListValue list_value = 6; } } \
             enum NullValue { NULL_VALUE = 0; } message ListValue { repeated Value values = 1; }"
        }
        // only here so that imports of them resolve; their options are skipped anyway
        "google/protobuf/descriptor.proto" => "",
        "google/api/annotations.proto" | "google/api/http.proto" => return Some("syntax = \"proto3\"; package google.api;".to_string()),
        _ => return None,
    };
    Some(format!("{FILE_HEAD}{body}"))
}

// -------------------------------------------------------------------- api

/// Parses `src` (named `name`, as imports would refer to it) and everything it
/// imports, and adds all of it to `pool`. `find` fetches an imported file by its
/// import path; the standard google/protobuf files are built in.
pub fn load_proto(pool: &mut Pool, name: &str, src: &str, find: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let mut files: Vec<FileAst> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(name.to_string());
    let mut queue: Vec<(String, String)> = vec![(name.to_string(), src.to_string())];
    while let Some((fname, text)) = queue.pop() {
        let ast = parse(&fname, &text)?;
        for imp in &ast.imports {
            if pool.has_file(imp) || !seen.insert(imp.clone()) {
                continue;
            }
            let text = builtin(imp).or_else(|| find(imp)).ok_or_else(|| format!("{fname}: cannot find the import \"{imp}\" (put it next to the .proto or in a parent folder)"))?;
            queue.push((imp.clone(), text));
        }
        files.push(ast);
    }

    let mut msgs: Vec<Message> = Vec::new();
    let mut enums: Vec<EnumDesc> = Vec::new();
    let mut services: Vec<Service> = Vec::new();
    {
        let mut local = HashMap::new();
        for f in &files {
            collect(&f.package, &f.messages, &f.enums, &mut local);
        }
        let types = Types { local, pool: &*pool };
        for f in &files {
            for m in &f.messages {
                build_message(&f.name, m, &f.package, f.proto3, &types, &mut msgs, &mut enums)?;
            }
            for e in &f.enums {
                enums.push(EnumDesc { full_name: join(&f.package, &e.name), values: e.values.clone() });
            }
            for s in &f.services {
                let mut methods = Vec::new();
                for r in &s.rpcs {
                    let mut ends = [String::new(), String::new()];
                    for (slot, ty) in ends.iter_mut().zip([&r.input, &r.output]) {
                        match types.resolve(&f.package, ty) {
                            Some((full, false)) => *slot = full,
                            _ => return Err(format!("{}: rpc {}.{} uses \"{ty}\", which is not a known message", f.name, s.name, r.name)),
                        }
                    }
                    let [input, output] = ends;
                    methods.push(Method { name: r.name.clone(), input, output, client_streaming: r.client_streaming, server_streaming: r.server_streaming });
                }
                services.push(Service { full_name: join(&f.package, &s.name), methods });
            }
        }
    }
    for f in &files {
        pool.mark_file(&f.name);
    }
    for m in msgs {
        pool.insert_message(m);
    }
    for e in enums {
        pool.insert_enum(e);
    }
    for s in services {
        pool.insert_service(s);
    }
    Ok(())
}

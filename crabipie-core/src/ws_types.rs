use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected(WsConnection),
    Disconnected(String),
    MessageReceived(String),
    BinaryReceived(Vec<u8>),
    Error(String),
}

pub enum WsCommand {
    Connect(String), // URL
    Disconnect,
    SendMessage(String),
}

#[derive(Debug, Clone)]
pub struct WsMessage {
    pub timestamp: String,
    pub direction: MessageDirection,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageDirection {
    Sent,
    Received,
    System,
}

#[derive(Debug, Clone)]
pub struct WsMessageDisplay {
    pub timestamp: String,
    pub direction: MessageDirection,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct WsConnection(pub futures::channel::mpsc::Sender<WsOut>);

/// What the UI hands to the socket task.
#[derive(Debug, Clone)]
pub enum WsOut {
    Text(String),
    Binary(Vec<u8>),
}

impl WsConnection {
    pub fn send(&mut self, message: String) -> Result<(), String> {
        self.0.try_send(WsOut::Text(message)).map_err(|e| e.to_string())
    }

    pub fn send_binary(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.0.try_send(WsOut::Binary(bytes)).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum WsMessageType {
    Text,
    Json,
    Binary,
    HTML,
}

impl std::fmt::Display for WsMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl WsMessageType {
    pub const ALL: [WsMessageType; 4] = [
        WsMessageType::Text,
        WsMessageType::Json,
        WsMessageType::Binary,
        WsMessageType::HTML,
    ];
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum WsBinaryMessageType {
    Base64,
    HexaDecimal,
}

impl std::fmt::Display for WsBinaryMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl WsBinaryMessageType {
    pub const ALL: [WsBinaryMessageType; 2] = [
        WsBinaryMessageType::Base64,
        WsBinaryMessageType::HexaDecimal,
    ];
}

/// A message ready to go on the wire, after applying the chosen message type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsPayload {
    Text(String),
    Binary(Vec<u8>),
}

/// Text and HTML go out as-is, Json must parse first, Binary is decoded from
/// Base64 or hex into a real binary frame. (The iced app sent everything as text.)
pub fn encode_ws_payload(kind: WsMessageType, bin: WsBinaryMessageType, input: &str) -> Result<WsPayload, String> {
    match kind {
        WsMessageType::Text | WsMessageType::HTML => Ok(WsPayload::Text(input.to_string())),
        WsMessageType::Json => serde_json::from_str::<serde_json::Value>(input)
            .map(|_| WsPayload::Text(input.to_string()))
            .map_err(|e| format!("Invalid JSON: {e}")),
        WsMessageType::Binary => match bin {
            WsBinaryMessageType::Base64 => {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input.trim())
                    .map(WsPayload::Binary)
                    .map_err(|e| format!("Invalid Base64: {e}"))
            }
            WsBinaryMessageType::HexaDecimal => decode_hex(input).map(WsPayload::Binary),
        },
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s.trim().trim_start_matches("0x").chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.is_ascii() {
        return Err("Invalid hex: non-ASCII character".into());
    }
    if cleaned.len() % 2 != 0 {
        return Err("Invalid hex: odd number of digits".into());
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|_| "Invalid hex digit".to_string()))
        .collect()
}

/// "48 65 6c ..." for the first `max` bytes, with an ellipsis if cut.
pub fn hex_dump(bytes: &[u8], max: usize) -> String {
    let mut s = bytes.iter().take(max).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    if bytes.len() > max {
        s.push_str(" …");
    }
    s
}

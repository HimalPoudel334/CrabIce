use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected(WsConnection),
    Disconnected(String),
    MessageReceived(String),
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
pub struct WsConnection(pub futures::channel::mpsc::Sender<String>);

impl WsConnection {
    pub fn send(&mut self, message: String) -> Result<(), String> {
        self.0.try_send(message).map_err(|e| e.to_string())
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

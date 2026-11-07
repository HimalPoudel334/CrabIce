//! WebSocket message/event types and the connect helper.

use serde::{Deserialize, Serialize};
use reqwest_websocket::RequestBuilderExt;

#[derive(Debug, Clone)]
pub(crate) enum WsEvent {
    Connected(WsConnection),
    Disconnected(String),
    MessageReceived(String),
    Error(String),
}

pub(crate) enum WsCommand {
    Connect(String), // URL
    Disconnect,
    SendMessage(String),
}

#[derive(Debug, Clone)]
pub(crate) struct WsMessage {
    pub(crate) timestamp: String,
    pub(crate) direction: MessageDirection,
    pub(crate) content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MessageDirection {
    Sent,
    Received,
    System,
}

#[derive(Debug, Clone)]
pub(crate) struct WsMessageDisplay {
    pub(crate) timestamp: String,
    pub(crate) direction: MessageDirection,
    pub(crate) content: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WsConnection(pub(crate) iced::futures::channel::mpsc::Sender<String>);

impl WsConnection {
    pub(crate) fn send(&mut self, message: String) {
        self.0
            .try_send(message)
            .expect("Send message through WebSocket");
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum WsMessageType {
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
    pub(crate) const ALL: [WsMessageType; 4] = [
        WsMessageType::Text,
        WsMessageType::Json,
        WsMessageType::Binary,
        WsMessageType::HTML,
    ];
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum WsBinaryMessageType {
    Base64,
    HexaDecimal,
}

impl std::fmt::Display for WsBinaryMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl WsBinaryMessageType {
    pub(crate) const ALL: [WsBinaryMessageType; 2] = [
        WsBinaryMessageType::Base64,
        WsBinaryMessageType::HexaDecimal,
    ];
}

// Helper function to connect
pub(crate) async fn connect(
    url: &str,
) -> Result<reqwest_websocket::WebSocket, Box<dyn std::error::Error + Send + Sync>> {
    let response = reqwest::Client::new().get(url).upgrade().send().await?;
    let websocket = response.into_websocket().await?;
    Ok(websocket)
}

//! WebSocket runtime: the subscription recipe/stream plus tab-level logging helpers.

use iced::widget::text_editor;
use iced::futures::SinkExt;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::websocket::{WsConnection, WsEvent};

impl CrabiPie {
    pub(crate) fn add_to_log(&mut self, prefix: &str, message: &str) {
        let Some(tab) = self.current_tab_mut() else {
            return;
        };
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let formatted = format!("[{}] {} {}\n", timestamp, prefix, message);
        tab.ws_messages_content
            .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
        tab.ws_messages_content
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                std::sync::Arc::new(formatted),
            )));
    }

    pub(crate) fn add_ws_received_message(&mut self, content: &str) {
        self.add_to_log("←", content);
        if let Some(tab) = self.current_tab_mut() {
            tab.ws_count_received += 1;
        }
    }

    pub(crate) fn add_ws_system_message(&mut self, content: &str) {
        self.add_to_log("•", content);
    }

    // Call this in your WsSendMessage logic
    pub(crate) fn add_ws_sent_message(&mut self, content: &str) {
        self.add_to_log("→", content);
        if let Some(tab) = self.current_tab_mut() {
            tab.ws_count_sent += 1;
        }
    }
}

#[derive(Debug, Hash, Clone)]
pub(crate) struct WebSocketRecipe {
    pub(crate) ws_connection_id: usize,
    pub(crate) url: String,
}

impl iced::advanced::subscription::Recipe for WebSocketRecipe {
    type Output = Message;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        use std::hash::Hash;
        self.ws_connection_id.hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: std::pin::Pin<
            Box<dyn futures::Stream<Item = iced::advanced::subscription::Event> + Send>,
        >,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = Self::Output> + Send>> {
        let url = self.url.clone();

        Box::pin(websocket_stream(url))
    }
}

// Create the actual stream function
pub(crate) fn websocket_stream(url: String) -> impl futures::Stream<Item = Message> {
    use futures::sink::SinkExt;
    use futures::stream::StreamExt;

    futures::stream::unfold((WebSocketState::Disconnected, 0 as u8), move |state| {
        let url = url.clone();
        async move {
            match state {
                (WebSocketState::Disconnected, retry_count) => {
                    // Stop retrying after 3 attempts
                    if retry_count >= 3 {
                        return Some((
                                Message::WsEvent(WsEvent::Error(
                                    "Failed to connect after 3 attempts. Please check the URL and try again.".to_string()
                                )),
                                (WebSocketState::Failed, retry_count)
                            ));
                    }

                    // Try to connect
                    match crate::domain::websocket::connect(&url).await {
                        Ok(websocket) => {
                            let (sender, receiver) = iced::futures::channel::mpsc::channel(100);
                            let connection = WsConnection(sender);

                            Some((
                                Message::WsEvent(WsEvent::Connected(connection)),
                                (
                                    WebSocketState::Connected {
                                        websocket,
                                        receiver,
                                    },
                                    0,
                                ),
                            ))
                        }
                        Err(e) => {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            Some((
                                Message::WsEvent(WsEvent::Error(format!(
                                    "Connection attempt {} failed: {}",
                                    retry_count + 1,
                                    e
                                ))),
                                (WebSocketState::Disconnected, retry_count + 1),
                            ))
                        }
                    }
                }
                (WebSocketState::Failed, retry_count) => {
                    // Stop the stream - no more retries
                    None
                }
                (
                    WebSocketState::Connected {
                        mut websocket,
                        mut receiver,
                    },
                    _,
                ) => {
                    tokio::select! {
                        result = websocket.next() => {
                            match result {
                                Some(Ok(reqwest_websocket::Message::Text(text))) => {
                                    Some((
                                        Message::WsEvent(WsEvent::MessageReceived(text)),
                                        (WebSocketState::Connected { websocket, receiver }, 0)
                                    ))
                                }
                                Some(Err(e)) => {
                                    Some((
                                        Message::WsEvent(WsEvent::Disconnected(format!("{}", e))),
                                        (WebSocketState::Failed, 0) // Don't retry on disconnection
                                    ))
                                }
                                None => {
                                    Some((
                                        Message::WsEvent(WsEvent::Disconnected("Connection closed".to_string())),
                                        (WebSocketState::Failed, 0)
                                    ))
                                }
                                _ => {
                                    Some((
                                        Message::WsEvent(WsEvent::MessageReceived("[Other]".to_string())),
                                        (WebSocketState::Connected { websocket, receiver }, 0)
                                    ))
                                }
                            }
                        }

                        Some(message) = receiver.next() => { //ui related
                            match websocket.send(reqwest_websocket::Message::Text(message)).await {
                                Ok(_) => Some((
                                    Message::NoOp,
                                    (WebSocketState::Connected { websocket, receiver }, 0)
                                )),
                                Err(e) => {
                                    Some((
                                        Message::WsEvent(WsEvent::Disconnected(format!("Send error: {}", e))),
                                        (WebSocketState::Failed, 0)
                                    ))
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

// State enum for the stream
pub(crate) enum WebSocketState {
    Disconnected,
    Connected {
        websocket: reqwest_websocket::WebSocket,
        receiver: iced::futures::channel::mpsc::Receiver<String>,
    },
    Failed,
}

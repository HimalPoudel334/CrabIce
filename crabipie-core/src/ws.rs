//! WebSocket client as a plain `Stream<Item = WsEvent>`.
//!
//! The first event is `Connected(WsConnection)`; use that handle to send.
//! Up to 3 connect attempts (2s apart), then the stream ends after an Error.

use crate::*;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use reqwest_websocket::RequestBuilderExt;

pub async fn connect_ws(
    url: &str,
) -> Result<reqwest_websocket::WebSocket, Box<dyn std::error::Error + Send + Sync>> {
    let response = reqwest::Client::new().get(url).upgrade().send().await?;
    let websocket = response.into_websocket().await?;
    Ok(websocket)
}

enum WebSocketState {
    Disconnected,
    Connected {
        websocket: reqwest_websocket::WebSocket,
        receiver: futures::channel::mpsc::Receiver<WsOut>,
    },
    Failed,
}

pub fn websocket_stream(url: String) -> impl futures::Stream<Item = WsEvent> {
    futures::stream::unfold((WebSocketState::Disconnected, 0u8), move |state| {
        let url = url.clone();
        async move {
            match state {
                (WebSocketState::Disconnected, retry_count) => {
                    if retry_count >= 3 {
                        return Some((
                            Some(WsEvent::Error(
                                "Failed to connect after 3 attempts. Please check the URL and try again."
                                    .to_string(),
                            )),
                            (WebSocketState::Failed, retry_count),
                        ));
                    }

                    match connect_ws(&url).await {
                        Ok(websocket) => {
                            let (sender, receiver) = futures::channel::mpsc::channel(100);
                            Some((
                                Some(WsEvent::Connected(WsConnection(sender))),
                                (WebSocketState::Connected { websocket, receiver }, 0),
                            ))
                        }
                        Err(e) => {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            Some((
                                Some(WsEvent::Error(format!(
                                    "Connection attempt {} failed: {}",
                                    retry_count + 1,
                                    e
                                ))),
                                (WebSocketState::Disconnected, retry_count + 1),
                            ))
                        }
                    }
                }
                (WebSocketState::Failed, _) => None,
                (WebSocketState::Connected { mut websocket, mut receiver }, _) => {
                    tokio::select! {
                        result = websocket.next() => {
                            match result {
                                Some(Ok(reqwest_websocket::Message::Text(text))) => Some((
                                    Some(WsEvent::MessageReceived(text)),
                                    (WebSocketState::Connected { websocket, receiver }, 0),
                                )),
                                Some(Ok(reqwest_websocket::Message::Binary(bytes))) => Some((
                                    Some(WsEvent::BinaryReceived(bytes.to_vec())),
                                    (WebSocketState::Connected { websocket, receiver }, 0),
                                )),
                                Some(Err(e)) => Some((
                                    Some(WsEvent::Disconnected(format!("{}", e))),
                                    (WebSocketState::Failed, 0),
                                )),
                                None => Some((
                                    Some(WsEvent::Disconnected("Connection closed".to_string())),
                                    (WebSocketState::Failed, 0),
                                )),
                                _ => Some((
                                    Some(WsEvent::MessageReceived("[Other]".to_string())),
                                    (WebSocketState::Connected { websocket, receiver }, 0),
                                )),
                            }
                        }
                        Some(message) = receiver.next() => {
                            let frame = match message {
                                WsOut::Text(t) => reqwest_websocket::Message::Text(t),
                                WsOut::Binary(b) => reqwest_websocket::Message::Binary(b.into()),
                            };
                            match websocket.send(frame).await {
                                // A successful send produces no event.
                                Ok(_) => Some((
                                    None,
                                    (WebSocketState::Connected { websocket, receiver }, 0),
                                )),
                                Err(e) => Some((
                                    Some(WsEvent::Disconnected(format!("Send error: {}", e))),
                                    (WebSocketState::Failed, 0),
                                )),
                            }
                        }
                    }
                }
            }
        }
    })
    .filter_map(|ev| async move { ev })
}

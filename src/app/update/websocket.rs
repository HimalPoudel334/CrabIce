//! `update` handlers for the **websocket** messages.

use iced::widget::text_editor;
use iced::advanced::text::Editor;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::websocket::WsEvent;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::WsConnect => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            let url = tab.url.clone();

            // Validate URL
            if !url.starts_with("ws://") && !url.starts_with("wss://") {
                app.add_ws_system_message("Error: WebSocket URL must start with ws:// or wss://");
                return iced::Task::none();
            }

            tab.loading = true;
            tab.ws_connection_id += 1;
            app.add_ws_system_message(&format!("Connecting to {}...", url));

            iced::Task::none()
        }
        Message::WsDisconnect => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.ws_connection_id = 0;
            tab.ws_connection = None;
            tab.ws_connected = false;
            tab.loading = false;
            app.add_ws_system_message("Disconnected");
            iced::Task::none()
        }
        Message::WsEvent(event) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            match event {
                WsEvent::Connected(connection) => {
                    tab.loading = false;
                    tab.ws_connected = true;
                    tab.ws_connection = Some(connection);
                    app.add_ws_system_message("Connected successfully");
                }
                WsEvent::Disconnected(reason) => {
                    tab.ws_connection = None;
                    tab.loading = false;
                    tab.ws_connected = false;
                    app.add_ws_system_message(&format!("Disconnected: {}", reason));
                }
                WsEvent::MessageReceived(content) => {
                    app.add_ws_received_message(&content);
                }
                WsEvent::Error(error) => {
                    tab.loading = false;
                    app.add_ws_system_message(&format!("Error: {}", error));
                }
            }
            iced::Task::none()
        }
        Message::WsMessageInputChanged(text) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.ws_input = text;
            iced::Task::none()
        }
        Message::WsSendMessage => {
            // 1. Isolate the mutable borrow of 'app' (via 'tab') in a block
            let message = {
                let Some(tab) = app.current_tab_mut() else {
                    return iced::Task::none();
                };

                let msg = tab.ws_input.clone();
                if msg.is_empty() {
                    return iced::Task::none();
                }

                tab.ws_input.clear();

                // If you need to send the message, do it while you have the tab
                if let Some(conn) = tab.ws_connection.as_mut() {
                    let _ = conn.send(msg.clone());
                }

                msg // Return the msg so we can use it outside the block
            }; // <--- 'tab' goes out of scope here, 'app' is no longer borrowed

            // 2. Now 'app' is free to be borrowed again here
            app.add_ws_sent_message(&message);

            iced::Task::none()
        }
        Message::WsClearMessages => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.ws_messages_content = text_editor::Content::new();
            tab.ws_count_sent = 0;
            tab.ws_count_received = 0;

            iced::Task::none()
        }
        Message::WsToggleAutoScroll => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_auto_scroll = !tab.ws_auto_scroll;
            iced::Task::none()
        }
        Message::WsMessageEditorAction(action) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_messages_content.perform(action);
            iced::Task::none()
        }
        Message::WsMessageTypeSelected(msg_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_message_type = msg_type;
            iced::Task::none()
        }
        Message::WsBinaryMessageTypeSelected(msg_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.ws_binary_message_type = msg_type;
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

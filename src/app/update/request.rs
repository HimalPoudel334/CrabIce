//! `update` handlers for the **request** messages.

use iced::widget::{scrollable, text_editor};
use std::sync::atomic::Ordering;
use iced::advanced::text::Editor;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::{TabLoadState, TabState};
use crate::domain::http::RequestType;
use crate::domain::persistence::SavedState;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::RequestTypeSelected(req_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            match req_type {
                RequestType::HTTP | RequestType::GraphQL => {
                    tab.request_type = req_type;
                }
                RequestType::WebSocket => {
                    tab.request_type = req_type;
                    tab.ws_connected = false;
                    tab.ws_input = String::new();
                    tab.ws_auto_scroll = true;
                    tab.url = "wss://echo.websocket.org".to_string();
                }
                _ => {
                    tab.response_body = std::sync::Arc::from("Ops! Sorry. Not implemented yet!");
                }
            }
            iced::Task::none()
        }
        Message::MethodSelected(method) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.method = method;
            iced::Task::none()
        }
        Message::UrlChanged(url) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.url = url;
            app.parse_url_query();
            iced::Task::none()
        }
        Message::SendRequest => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            // A fresh, user-initiated send always gets its own chance at an
            // auth-refresh retry, independent of any earlier send on this tab.
            tab.auth_retry_used = false;
            if !tab.loading && !tab.url.trim().is_empty() {
                tab.loading = true;
                app.send_request()
            } else {
                iced::Task::none()
            }
        }
        // Internal resend issued after an auth-refresh completes. Deliberately
        // does NOT reset `auth_retry_used` — otherwise a token that's still
        // bad after refreshing (e.g. wrong creds) would retry forever.
        Message::SendRequestRetry => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.loading = true;
            app.send_request_now()
        }
        Message::BodyAction(action) => {
            match action {
                text_editor::Action::Edit(_) => {
                    app.request_body_content.perform(action);
                    let updated_text = app.request_body_content.text();
                    if let Some(tab) = app.current_tab_mut() {
                        tab.request_body = std::sync::Arc::from(updated_text.as_str());
                    }
                }
                _ => app.request_body_content.perform(action),
            }
            iced::Task::none()
        }
        Message::AuthTypeSelected(auth_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.auth_type = auth_type;
            iced::Task::none()
        }
        Message::BearerTokenChanged(token) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.bearer_token = token;
            iced::Task::none()
        }
        Message::ContentTypeSelected(content_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.content_type = content_type;
            iced::Task::none()
        }
        Message::CancelRequest => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.cancel_flag.store(true, Ordering::Relaxed);
            tab.loading = false;
            tab.response_body = std::sync::Arc::from("Request cancelled by user");
            tab.response_status = "Cancelled".to_string();
            app.response_body_content =
                text_editor::Content::with_text("Request cancelled by user");
            iced::Task::none()
        }
        Message::SaveRequest => {
            let Some(tab) = app.current_tab() else {
                return iced::Task::none();
            };
            let state = tab.to_saved(&app.json_theme.to_string(), &app.app_theme.to_string());

            iced::Task::perform(
                async move {
                    match rfd::AsyncFileDialog::new()
                        .set_title("Save CrabiPie State")
                        .set_file_name("crabipie_state.json")
                        .save_file()
                        .await
                    {
                        Some(file_handle) => {
                            // Serialize JSON
                            let json = serde_json::to_string_pretty(&state)
                                .map_err(|e| format!("Serialization error: {}", e))?;

                            // Async write
                            tokio::fs::write(file_handle.path(), json)
                                .await
                                .map_err(|e| format!("Failed to write file: {}", e))?;

                            Ok::<_, String>(file_handle.file_name().to_string())
                        }
                        None => Err("Save dialog cancelled".to_string()),
                    }
                },
                |result| match result {
                    Ok(filename) => Message::FileSaved(Ok(filename)),
                    Err(err) => Message::FileSaved(Err(err)),
                },
            )
        }
        Message::LoadRequest => {
            iced::Task::perform(
                async move {
                    match rfd::AsyncFileDialog::new()
                        .set_title("Open CrabiPie State")
                        .pick_file()
                        .await
                    {
                        Some(file_handle) => {
                            // Async read file
                            let bytes = tokio::fs::read(file_handle.path())
                                .await
                                .map_err(|e| format!("Failed to read file: {}", e))?;

                            // Convert bytes to string
                            let content = String::from_utf8(bytes)
                                .map_err(|e| format!("Invalid UTF-8 in file: {}", e))?;

                            // Deserialize JSON into SavedState
                            let saved_state: SavedState = serde_json::from_str(&content)
                                .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                            Ok::<_, String>(saved_state)
                        }
                        None => Err("Open file dialog cancelled".to_string()),
                    }
                },
                |result| match result {
                    Ok(saved_state) => Message::RequestLoaded(saved_state),
                    Err(err) => Message::RequestLoadFailed(err),
                },
            )
        }
        Message::RequestLoaded(saved_state) => {
            if let Some(slot) = app.tabs.get_mut(app.active_tab) {
                *slot = TabLoadState::Loaded(Box::new(TabState::from_saved(saved_state)));
            }
            iced::Task::none()
        }
        Message::RequestLoadFailed(err) => iced::Task::none(),
        Message::RequestTabSelected(request_tab) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            tab.active_request_tab = request_tab;
            iced::Task::none()
        }
        Message::UsernameChanged(username) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.username = username;
            iced::Task::none()
        }
        Message::PasswordChanged(password) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.password = password;
            iced::Task::none()
        }
        Message::ApiKeyPositionChanged(position) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.api_key_position = position;
            iced::Task::none()
        }
        Message::ApiKeyNameChanged(key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.api_key_name = key;
            iced::Task::none()
        }
        Message::ApiKeyChanged(key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.api_key = key;
            iced::Task::none()
        }
        Message::StreamChunk(chunk) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.stream_buffer.push_str(&chunk);
            iced::widget::operation::snap_to(
                format!("stream-scroll-{}", tab.id),
                scrollable::RelativeOffset::END,
            )
        }
        Message::StreamDone => {
            if let Some(tab) = app.current_tab_mut() {
                tab.is_streaming = false;
                tab.loading = false;
                if !tab.stream_buffer.is_empty() {
                    let body = std::mem::take(&mut tab.stream_buffer);
                    tab.response_body = std::sync::Arc::from(body.as_str());
                    app.response_body_content = text_editor::Content::with_text(&body);
                }
            }
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

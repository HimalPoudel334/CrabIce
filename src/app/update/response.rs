//! `update` handlers for the **response** messages.

use iced::widget::text_editor;
use iced::advanced::text::Editor;

use crate::app::CrabiPie;
use crate::app::message::{JsonTarget, Message};
use crate::app::tab::VideoState;
use crate::domain::cookies::{extract_domain, parse_set_cookie};
use crate::domain::view_state::ResponseTab;
use crate::domain::pdf_viewer;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::SaveBinaryResponse => {
            let Some(tab) = app.current_tab() else {
                return iced::Task::none();
            };
            if !tab.is_response_binary {
                return iced::Task::none();
            }

            let file_name = tab.response_filename.clone();
            let response_bytes = tab.response_bytes.clone();

            iced::Task::perform(
                async move {
                    match rfd::AsyncFileDialog::new()
                        .set_file_name(&file_name)
                        .save_file()
                        .await
                    {
                        Some(file) => match file.write(&response_bytes).await {
                            Ok(_) => Message::FileSaved(Ok(file.file_name().to_string())),
                            Err(e) => Message::FileSaved(Err(format!("Failed to save: {}", e))),
                        },
                        None => Message::FileSaved(Err("Save dialog cancelled".to_string())),
                    }
                },
                |message| message,
            )
        }
        Message::FileSaved(result) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            match result {
                Ok(filename) => {
                    tab.response_body =
                        std::sync::Arc::from(format!("File saved successfully: {}", filename))
                }
                Err(error) => {
                    tab.response_body =
                        std::sync::Arc::from(format!("Error saving file: {}", error))
                }
            }
            app.response_body_content = text_editor::Content::with_text(&tab.response_body);
            iced::Task::none()
        }
        Message::ClearResponseText => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.response_body = std::sync::Arc::from("");
            tab.response_headers = std::sync::Arc::from("");
            app.response_body_content = text_editor::Content::new();
            app.response_headers_content = text_editor::Content::new();
            iced::Task::none()
        }
        Message::ResponseReceived(resp) => {
            // Auth auto-refresh backstop: a 401 with auto-refresh configured
            // and not yet retried triggers one refresh + resend instead of
            // surfacing the failure straight away.
            if resp.status.starts_with("401") {
                if let Some(retry_task) = app.maybe_retry_after_auth_failure() {
                    return retry_task;
                }
            }
            let url = {
                let Some(tab) = app.current_tab_mut() else {
                    return iced::Task::none();
                };
                tab.loading = false;
                tab.is_streaming = resp.is_streaming;

                tab.active_response_tab = if resp.is_streaming || !resp.body.is_empty() {
                    ResponseTab::Body
                } else {
                    ResponseTab::Headers
                };

                tab.is_response_binary = resp.is_binary;
                tab.response_status = resp.status;
                tab.response_content_type = resp.content_type.clone();
                tab.response_time = resp.response_time;
                tab.response_headers = std::sync::Arc::from(resp.headers.as_str());
                let url = tab.url.clone();

                if resp.is_binary && resp.content_type.starts_with("video/") && resp.accepts_range {
                    let parsed_url = url::Url::parse(&url).unwrap();
                    match iced_video_player::Video::new(&parsed_url) {
                        Ok(video) => {
                            tab.video_player = Some(video);
                            tab.video_state = Some(VideoState {
                                playing: true,
                                buffering: true,
                                position: 0.0,
                                duration: 0.0,
                                volume: 0.8,
                                dragging: false,
                            });
                        }
                        Err(e) => {
                            eprintln!("Failed to load video: {e:?}");
                            tab.video_player = None;
                        }
                    }
                } else if resp.is_binary && resp.content_type.starts_with("image/") {
                    tab.image_handle =
                        Some(iced::widget::image::Handle::from_bytes(resp.bytes.clone()));
                    tab.response_bytes = resp.bytes.clone();
                    tab.response_filename = resp.filename.clone();
                } else if resp.is_binary && resp.content_type == "application/pdf" {
                    tab.video_player = None;
                    tab.image_handle = None;
                    match pdf_viewer::PdfViewer::from_bytes(resp.bytes.clone()) {
                        Ok(viewer) => {
                            tab.pdf_viewer = Some(viewer);
                            tab.pdf_current_image = tab
                                .pdf_viewer
                                .as_mut()
                                .and_then(|p| p.render_current_page().ok());
                        }

                        Err(e) => {
                            eprintln!("PDF load error: {e:?}");
                        }
                    }

                    tab.response_bytes = resp.bytes.clone();
                    tab.response_filename = resp.filename.clone();
                } else {
                    // Text / JSON / Empty (HEAD, OPTIONS) response init
                    tab.video_player = None;
                    tab.video_state = None;

                    let body_text = if resp.is_streaming {
                        // Chunks are coming via StreamChunk — don't paint a
                        // placeholder that might stomp on data about to arrive.
                        String::new()
                    } else if resp.body.is_empty() {
                        "No response body".to_string()
                    } else {
                        resp.body.clone()
                    };
                    tab.response_body = std::sync::Arc::from(body_text.as_str());
                    app.response_body_content = text_editor::Content::with_text(&body_text);
                }
                url
            };
            app.response_headers_content = text_editor::Content::with_text(&resp.headers);
            if let Some(domain) = extract_domain(&url) {
                for raw in &resp.set_cookies {
                    if let Some(cookie) = parse_set_cookie(raw) {
                        let jar = app.cookie_jar.entry(domain.clone()).or_default();
                        if let Some(existing) = jar.iter_mut().find(|c| c.name == cookie.name) {
                            *existing = cookie;
                        } else {
                            jar.push(cookie);
                        }
                    }
                }
            }
            iced::Task::none()
        }
        Message::ResponseBodyAction(action) => {
            match action {
                text_editor::Action::Edit(_) => {}
                _ => app.response_body_content.perform(action),
            }
            iced::Task::none()
        }
        Message::ResponseHeadersAction(action) => {
            match action {
                text_editor::Action::Edit(_) => {}
                _ => app.response_headers_content.perform(action),
            }
            iced::Task::none()
        }
        Message::ResponseTabSelected(response_tab) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.active_response_tab = response_tab;
            iced::Task::none()
        }
        Message::PrettifyJson(target) => {
            let Some(tab) = app.current_tab() else {
                return iced::Task::none();
            };

            let body_text = match target {
                JsonTarget::RequestBody => tab.request_body.to_string(),

                JsonTarget::ResponseBody => {
                    if tab.is_response_binary {
                        return iced::Task::none();
                    }

                    tab.response_body.to_string()
                }

                JsonTarget::ResponseHeaders => tab.response_headers.to_string(),
            };

            iced::Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let json: serde_json::Value =
                            serde_json::from_str(&body_text).map_err(|e| e.to_string())?;

                        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
                    })
                    .await
                    .map_err(|e| e.to_string())?
                },
                move |result| Message::JsonPrettified(target, result),
            )
        }
        Message::JsonPrettified(target, Ok(pretty)) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            match target {
                JsonTarget::RequestBody => {
                    tab.request_body = std::sync::Arc::from(pretty.as_str());

                    app.request_body_content
                        .perform(text_editor::Action::SelectAll);
                    app.request_body_content.perform(text_editor::Action::Edit(
                        text_editor::Edit::Paste(std::sync::Arc::new(pretty)),
                    ));
                }

                JsonTarget::ResponseBody => {
                    tab.response_body = std::sync::Arc::from(pretty.as_str());

                    app.response_body_content
                        .perform(text_editor::Action::SelectAll);
                    app.response_body_content.perform(text_editor::Action::Edit(
                        text_editor::Edit::Paste(std::sync::Arc::new(pretty)),
                    ));
                }

                JsonTarget::ResponseHeaders => {
                    tab.response_headers = std::sync::Arc::from(pretty.as_str());

                    app.response_headers_content
                        .perform(text_editor::Action::SelectAll);
                    app.response_headers_content
                        .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                            std::sync::Arc::new(pretty),
                        )));
                }
            }

            iced::Task::none()
        }

        Message::JsonPrettified(_, Err(err)) => {
            eprintln!("Prettify failed: {err}");
            iced::Task::none()
        }
        Message::CopyToClipboard => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if tab.is_response_binary {
                return iced::Task::none();
            }
            let is_binary = tab.is_response_binary;
            let active_response_tab = tab.active_response_tab.clone();
            tab.copied = true;

            let text = match active_response_tab {
                ResponseTab::Body => app.response_body_content.text(),
                ResponseTab::Headers => app.response_headers_content.text(),
            };

            iced::Task::batch([
                iced::clipboard::write(text),
                iced::Task::perform(
                    async {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    },
                    |_| Message::ResetCopied,
                ),
            ])
        }
        Message::ResetCopied => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.copied = false;
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

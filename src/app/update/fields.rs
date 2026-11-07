//! `update` handlers for the **fields** messages.

use iced::widget::text_editor;
use iced::advanced::text::Editor;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::TabState;
use crate::domain::http::{ContentType, FormField, QueryParam, RequestHeaders};
use crate::domain::view_state::{FormViewType, HeaderViewType};

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::HeadersViewTypeChanged(view_type) => {
            match view_type {
                HeaderViewType::Raw => {
                    let raw = {
                        let Some(tab) = app.current_tab_mut() else {
                            return iced::Task::none();
                        };

                        let raw = TabState::headers_to_raw(&tab.headers);
                        tab.raw_headers_content = std::sync::Arc::from(raw.as_str());
                        tab.header_view_type = HeaderViewType::Raw;
                        raw
                    };

                    app.raw_headers_content = text_editor::Content::with_text(&raw);
                }

                HeaderViewType::Formatted => {
                    let raw = app.raw_headers_content.text();

                    let Some(tab) = app.current_tab_mut() else {
                        return iced::Task::none();
                    };

                    tab.raw_headers_content = std::sync::Arc::from(raw.as_str());
                    tab.headers = TabState::raw_to_headers(&raw);
                    tab.header_view_type = HeaderViewType::Formatted;
                }
            }

            iced::Task::none()
        }
        Message::HeaderRawAction(action) => {
            app.raw_headers_content.perform(action);
            let raw_text = app.raw_headers_content.text();
            if let Some(tab) = app.current_tab_mut() {
                tab.raw_headers_content = std::sync::Arc::from(raw_text.as_str());
            };
            iced::Task::none()
        }
        Message::QueryParamAdd => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.query_params.push(QueryParam::new());
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamRemove(idx) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if idx < tab.query_params.len() {
                tab.query_params.remove(idx);
            }
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamKeyChanged(idx, key) => {
            app.update_query(idx, |p| p.key = key);
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamValueChanged(idx, value) => {
            app.update_query(idx, |p| p.value = value);
            app.rebuild_url();
            iced::Task::none()
        }
        Message::QueryParamToggled(idx) => {
            app.update_query(idx, |p| p.enabled = !p.enabled);
            app.rebuild_url();
            iced::Task::none()
        }
        Message::FormFieldKeyChanged(index, key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            if let Some(field) = tab.form_data.get_mut(index) {
                field.key = key;
            }
            iced::Task::none()
        }
        Message::FormFieldValueChanged(index, value) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(index) {
                field.value = value;
            }
            iced::Task::none()
        }
        Message::FormFieldTypeSelected(idx, form_field_type) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(idx) {
                field.field_type = form_field_type;
                field.value.clear();
                field.files.clear();
            }
            iced::Task::none()
        }
        Message::FormFieldToggled(idx) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(idx) {
                field.enabled = !field.enabled;
            }
            iced::Task::none()
        }
        Message::FormFieldFileSelect(idx) => {
            let future = async move {
                let files = rfd::AsyncFileDialog::new()
                    .set_directory("~/Downloads")
                    .pick_files()
                    .await;

                // Extract file paths as Strings
                let paths = files
                    .map(|handles| {
                        handles
                            .into_iter()
                            .filter_map(|handle| handle.path().to_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Message::FormFieldFilesSelected(idx, paths)
            };

            iced::Task::perform(future, std::convert::identity)
        }
        Message::FormFieldFilesSelected(index, files) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(field) = tab.form_data.get_mut(index) {
                field.files = files;
            }
            iced::Task::none()
        }
        Message::FormFieldRemove(index) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if index < tab.form_data.len() {
                tab.form_data.remove(index);
            }
            iced::Task::none()
        }
        Message::FormFieldAdd => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.form_data.push(FormField::new());
            iced::Task::none()
        }
        Message::ViewRawForm => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if matches!(
                tab.content_type,
                ContentType::FormData | ContentType::XWWWFormUrlEncoded
            ) {
                let raw = TabState::form_data_to_raw(&tab.form_data);
                tab.raw_form_content = std::sync::Arc::from(raw.as_str());
                tab.form_view_type = FormViewType::Raw;
                app.raw_form_content = text_editor::Content::with_text(&raw);
            }
            iced::Task::none()
        }
        Message::ViewFormattedForm => {
            let raw = app.raw_form_content.text();
            if let Some(tab) = app.current_tab_mut() {
                if matches!(
                    tab.content_type,
                    ContentType::FormData | ContentType::XWWWFormUrlEncoded
                ) {
                    tab.raw_form_content = std::sync::Arc::from(raw.as_str());
                    tab.form_data = TabState::raw_to_form_data(&raw);
                    tab.form_view_type = FormViewType::Formatted;
                }
            }
            iced::Task::none()
        }
        Message::FormRawAction(action) => {
            app.raw_form_content.perform(action);
            let raw_text = app.raw_form_content.text();
            if let Some(tab) = app.current_tab_mut() {
                tab.raw_form_content = std::sync::Arc::from(raw_text.as_str());
            };
            iced::Task::none()
        }
        Message::HeaderAdd => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.headers.push(RequestHeaders::new());
            iced::Task::none()
        }
        Message::HeaderRemove(id) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if id < tab.headers.len() {
                tab.headers.remove(id);
            }
            iced::Task::none()
        }
        Message::HeaderKeyChanged(id, key) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            if let Some(header) = tab.headers.get_mut(id) {
                header.key = key;
            }
            iced::Task::none()
        }
        Message::HeaderValueChanged(id, value) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(header) = tab.headers.get_mut(id) {
                header.value = value;
            }
            iced::Task::none()
        }
        Message::HeaderToggled(id) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let Some(header) = tab.headers.get_mut(id) {
                header.enabled = !header.enabled;
            }
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

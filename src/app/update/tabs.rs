//! `update` handlers for the **tabs** messages.

use iced::widget::text_editor;
use iced::advanced::text::Editor;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::{TabLoadState, TabState, proximity_order};
use crate::domain::persistence::SavedState;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::TabSelected(index) => {
            let req_body = app.request_body_content.text();
            let raw_form = app.raw_form_content.text();
            let gql_query = app.graphql_query_content.text();
            let gql_vars = app.graphql_variables_content.text();

            if let Some(tab) = app.current_tab_mut() {
                tab.request_body = std::sync::Arc::from(req_body.as_str());
                tab.raw_form_content = std::sync::Arc::from(raw_form.as_str());
                tab.graphql_query = std::sync::Arc::from(gql_query.as_str());
                tab.graphql_variables = std::sync::Arc::from(gql_vars.as_str());
            }

            // 3. SWITCH THE ACTIVE INDEX
            app.active_tab = index;

            // 4. LOAD NEW DATA INTO EDITORS
            match &app.tabs[index] {
                TabLoadState::Loaded(tab) => {
                    app.request_body_content = text_editor::Content::with_text(&tab.request_body);
                    app.raw_headers_content =
                        text_editor::Content::with_text(&tab.raw_headers_content);
                    app.raw_form_content = text_editor::Content::with_text(&tab.raw_form_content);
                    app.graphql_query_content = text_editor::Content::with_text(&tab.graphql_query);
                    app.graphql_variables_content =
                        text_editor::Content::with_text(&tab.graphql_variables);
                    app.response_body_content = text_editor::Content::with_text(&tab.response_body);
                    app.response_headers_content =
                        text_editor::Content::with_text(&tab.response_headers);
                    iced::Task::none()
                }
                TabLoadState::Unloaded(meta) => {
                    let id = meta.id;
                    let meta_clone = meta.clone();
                    app.tabs[index] = TabLoadState::Loading(meta_clone);
                    iced::Task::perform(SavedState::load(id), move |s| Message::TabBodyLoaded {
                        id,
                        saved: s,
                    })
                }
                TabLoadState::Loading(_) => iced::Task::none(),
            }
        }
        Message::AddNewTab => {
            let new_id = app.next_tab_id;
            app.next_tab_id += 1;
            app.tabs.push(TabLoadState::Loaded(Box::new(TabState::new(
                new_id,
                app.active_environment_id,
            ))));
            app.active_tab = app.tabs.len() - 1;
            iced::Task::none()
        }
        Message::TabBodyLoaded { id, saved } => {
            println!("Loaded tab {}", id);
            // find the slot and hydrate it
            if let Some(slot) = app.tabs.iter_mut().find(|t| t.id() == id) {
                *slot = match saved {
                    Some(s) => TabLoadState::Loaded(Box::new(TabState::from_saved(s))),
                    None => TabLoadState::Unloaded(slot.metadata().clone()), // fallback
                };
            }

            if app.tabs.get(app.active_tab).map(|t| t.id()) == Some(id) {
                if let Some(tab) = app.current_tab() {
                    let req_body = tab.request_body.clone();
                    let raw_headers = tab.raw_headers_content.clone();
                    let raw_form = tab.raw_form_content.clone();
                    let gql_query = tab.graphql_query.clone();
                    let gql_vars = tab.graphql_variables.clone();
                    let res_body = tab.response_body.clone();
                    let res_headers = tab.response_headers.clone();

                    app.request_body_content = text_editor::Content::with_text(&req_body);
                    app.raw_headers_content = text_editor::Content::with_text(&raw_headers);
                    app.raw_form_content = text_editor::Content::with_text(&raw_form);
                    app.graphql_query_content = text_editor::Content::with_text(&gql_query);
                    app.graphql_variables_content = text_editor::Content::with_text(&gql_vars);
                    app.response_body_content = text_editor::Content::with_text(&res_body);
                    app.response_headers_content = text_editor::Content::with_text(&res_headers);
                }
            }

            // after active tab done, schedule background loading for others
            // proximity order: active±1, active±2, ...
            let active = app.active_tab;
            let unloaded: Vec<usize> = proximity_order(active, app.tabs.len())
                .into_iter()
                .filter(|&i| matches!(app.tabs[i], TabLoadState::Unloaded(_)))
                .collect();

            if let Some(&next_idx) = unloaded.first() {
                if let TabLoadState::Unloaded(meta) = &app.tabs[next_idx] {
                    let id = meta.id;
                    app.tabs[next_idx] = TabLoadState::Loading(meta.clone());
                    // low priority — runs in background while UI is live
                    return iced::Task::perform(SavedState::load(id), move |s| {
                        Message::TabBodyLoaded { id, saved: s }
                    });
                }
            }
            iced::Task::none()
        }
        Message::RequestTabLoad(index) => match app.tabs.get(index) {
            Some(TabLoadState::Unloaded(meta)) => {
                let id = meta.id;
                app.tabs[index] = TabLoadState::Loading(meta.clone());
                iced::Task::perform(SavedState::load(id), move |s| Message::TabBodyLoaded {
                    id,
                    saved: s,
                })
            }
            _ => iced::Task::none(),
        },
        Message::CloseTab(index) => {
            if app.tabs.len() > 1 {
                app.tabs.remove(index);
                if app.active_tab >= app.tabs.len() {
                    app.active_tab = app.tabs.len() - 1;
                } else if app.active_tab > index {
                    app.active_tab -= 1;
                }
            }
            iced::Task::none()
        }
        Message::TabDuplicate(index) => {
            // 1. Flush current editors into the active tab IF we're duplicating it.
            //    Do this FIRST, before borrowing tabs, so the borrow checker is happy
            //    and the snapshot below sees the up-to-date body.
            if index == app.active_tab {
                let req_body = app.request_body_content.text();
                let raw_form = app.raw_form_content.text();
                let gql_query = app.graphql_query_content.text();
                let gql_vars = app.graphql_variables_content.text();
                if let Some(t) = app.current_tab_mut() {
                    t.request_body = std::sync::Arc::from(req_body.as_str());
                    t.raw_form_content = std::sync::Arc::from(raw_form.as_str());
                    t.graphql_query = std::sync::Arc::from(gql_query.as_str());
                    t.graphql_variables = std::sync::Arc::from(gql_vars.as_str());
                }
            }

            // 2. Snapshot source tab: either its SavedState (Loaded) or its metadata.
            //    The scoped block ensures `slot` is dropped before we mutate app.tabs.
            let (saved, meta) = {
                let Some(slot) = app.tabs.get(index) else {
                    return iced::Task::none();
                };
                let saved = match slot {
                    TabLoadState::Loaded(t) => {
                        Some(t.to_saved(&app.json_theme.to_string(), &app.app_theme.to_string()))
                    }
                    _ => None,
                };
                let meta = slot.metadata().clone();
                (saved, meta)
            }; // <-- slot borrow ends here

            // 3. Create the new tab.
            let new_id = app.next_tab_id;
            app.next_tab_id += 1;

            let new_tab = match saved {
                Some(mut s) => {
                    s.id = new_id;
                    s.title = format!("{} (copy)", s.title);
                    TabLoadState::Loaded(Box::new(TabState::from_saved(s)))
                }
                None => {
                    let mut fresh = TabState::new(new_id, app.active_environment_id);
                    fresh.title = format!("{} (copy)", meta.title);
                    fresh.metadata.title = fresh.title.clone();
                    fresh.url = meta.url.clone();
                    fresh.method = meta.method.clone();
                    fresh.request_type = meta.request_type.clone();
                    fresh.metadata.url = meta.url.clone();
                    fresh.metadata.method = meta.method.clone();
                    fresh.metadata.request_type = meta.request_type.clone();
                    TabLoadState::Loaded(Box::new(fresh))
                }
            };

            // 4. Insert after source, activate it.
            app.tabs.insert(index + 1, new_tab);
            app.active_tab = index + 1;

            // 5. Reload shared editors from the new active tab.
            if let Some(tab) = app.current_tab() {
                let req_body = tab.request_body.clone();
                let raw_headers = tab.raw_headers_content.clone();
                let raw_form = tab.raw_form_content.clone();
                let gql_query = tab.graphql_query.clone();
                let gql_vars = tab.graphql_variables.clone();
                let res_body = tab.response_body.clone();
                let res_headers = tab.response_headers.clone();

                app.request_body_content = text_editor::Content::with_text(&req_body);
                app.raw_headers_content = text_editor::Content::with_text(&raw_headers);
                app.raw_form_content = text_editor::Content::with_text(&raw_form);
                app.graphql_query_content = text_editor::Content::with_text(&gql_query);
                app.graphql_variables_content = text_editor::Content::with_text(&gql_vars);
                app.response_body_content = text_editor::Content::with_text(&res_body);
                app.response_headers_content = text_editor::Content::with_text(&res_headers);
            }

            app.save_task()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

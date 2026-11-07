//! `update` handlers for the **graphql** messages.

use iced::widget::text_editor;
use iced::advanced::text::Editor;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::graphql::{INTROSPECTION_QUERY, parse_graphql_schema};
use crate::domain::http::HTTP_CLIENT;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::GraphqlQueryAction(action) => {
            app.graphql_query_content.perform(action);
            let query_text = app.graphql_query_content.text();
            if let Some(tab) = app.current_tab_mut() {
                tab.graphql_query = std::sync::Arc::from(query_text.as_str());
            }
            iced::Task::none()
        }
        Message::GraphqlVariablesAction(action) => {
            app.graphql_variables_content.perform(action);
            let vars_text = app.graphql_variables_content.text();
            if let Some(tab) = app.current_tab_mut() {
                tab.graphql_variables = std::sync::Arc::from(vars_text.as_str());
            }
            iced::Task::none()
        }
        Message::GraphqlOperationChanged(val) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_operation = val;
            iced::Task::none()
        }
        Message::FetchGraphqlSchema => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            let url = tab.url.clone();
            if url.is_empty() {
                return iced::Task::none();
            }

            tab.graphql_schema_loading = true;
            tab.graphql_schema_error = None;

            iced::Task::perform(
                async move {
                    let body = serde_json::json!({ "query": INTROSPECTION_QUERY });
                    let resp = HTTP_CLIENT
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(body.to_string())
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    let text = resp.text().await.map_err(|e| e.to_string())?;

                    // This function should now return your new GraphqlSchema struct
                    parse_graphql_schema(&text)
                },
                Message::GraphqlSchemaFetched,
            )
        }

        Message::GraphqlSchemaFetched(result) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_schema_loading = false;
            match result {
                Ok(schema) => {
                    tab.graphql_schema = Some(schema);
                    tab.graphql_schema_error = None;
                    // Optional: Auto-expand the root query type
                    // tab.graphql_expanded_types.insert(schema.query_type.name.clone());
                }
                Err(e) => {
                    tab.graphql_schema_error = Some(e);
                }
            }
            iced::Task::none()
        }
        Message::GraphqlFieldToggled(path) | Message::GraphqlArgToggled(path) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            let selected = &mut tab.graphql_selected_paths;
            let manual = &mut tab.manually_selected_paths;

            if selected.contains(&path) {
                // --- TOGGLE OFF ---
                // 1. Remove the path itself
                selected.remove(&path);
                manual.remove(&path);

                // 2. Remove ALL children (including args)
                let child_prefix = format!("{}.", path);
                selected.retain(|p| !p.starts_with(&child_prefix));
                manual.retain(|p| !p.starts_with(&child_prefix));

                // Also remove args for this path
                let arg_prefix = format!("{}.arg:", path);
                selected.retain(|p| !p.starts_with(&arg_prefix));
                manual.retain(|p| !p.starts_with(&arg_prefix));

                // 3. Walk backwards (Upwards) - uncheck parents if no children remain
                let mut parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
                while parts.len() > 1 {
                    parts.pop();
                    let parent_path = parts.join(".");

                    let sibling_prefix = format!("{}.", parent_path);
                    let has_active_children =
                        selected.iter().any(|p| p.starts_with(&sibling_prefix));

                    // Only remove if not manually selected AND no children
                    if !has_active_children && !manual.contains(&parent_path) {
                        selected.remove(&parent_path);
                    } else {
                        break;
                    }
                }
            } else {
                // --- TOGGLE ON ---
                // 1. Add the clicked path
                manual.insert(path.clone());
                selected.insert(path.clone());

                // 2. Bubbling check (Upwards) - auto-select all parents
                let parts: Vec<&str> = path.split('.').collect();
                let mut current_path = String::new();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        current_path.push('.');
                    }
                    current_path.push_str(part);

                    // Don't override manual selections, but add if not present
                    if !selected.contains(&current_path) {
                        selected.insert(current_path.clone());
                    }
                }
            }

            // Update the query
            let query = app.build_query();
            app.graphql_query_content = text_editor::Content::with_text(&query);
            if let Some(tab) = app.current_tab_mut() {
                tab.graphql_query = std::sync::Arc::from(query);
            }
            iced::Task::none()
        }

        Message::GraphqlTypeToggled(name) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };

            let expanded = &mut tab.graphql_expanded_types;
            if !expanded.remove(&name) {
                expanded.insert(name);
            }
            iced::Task::none()
        }

        Message::GraphqlSearchChanged(val) => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            tab.graphql_search = val;
            iced::Task::none()
        }
        Message::GraphqlCollapseAll => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            // Clear the set to collapse everything
            tab.graphql_expanded_types.clear();
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

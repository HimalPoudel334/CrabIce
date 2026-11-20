//! GraphQL panel and schema tree.

use iced::{Alignment, Element, Length, Padding};
use iced::widget::{
    Column,
    Space,
    button,
    checkbox,
    column,
    container,
    row,
    rule,
    scrollable,
    space,
    text,
    text_editor,
    text_input,
};

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::graphql::GraphqlType;

impl CrabiPie {
    pub(crate) fn render_graphql_tab(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let is_connected = tab.ws_connected;

        // ── left: schema tree ────────────────
        let schema_panel: Element<'_, Message> = if tab.graphql_schema_loading {
            container(text("Loading schema...").size(14))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        } else if let Some(err) = &tab.graphql_schema_error {
            container(
                Column::new()
                    .spacing(10)
                    .push(text(err).style(|_| text::Style {
                        color: Some(iced::Color::from_rgb(1.0, 0.3, 0.3)),
                    }))
                    .push(
                        button(text("🔄 Try Again").shaping(text::Shaping::Advanced))
                            .on_press(Message::FetchGraphqlSchema)
                            .padding(8)
                            .style(button::text),
                    ),
            )
            .padding(10)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else if let Some(schema) = &tab.graphql_schema {
            // --- Search + Collapse Bar ---
            let top_bar = row![
                text_input("Search fields", &tab.graphql_search)
                    .on_input(Message::GraphqlSearchChanged)
                    .width(Length::Fill)
                    .padding(5),
                button(text("󰡍").size(16))
                    .style(button::text)
                    .padding(5)
                    .on_press(Message::GraphqlCollapseAll),
                button(text("↻").size(16))
                    .style(button::text)
                    .padding(5)
                    .on_press(Message::FetchGraphqlSchema),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            // --- Root Type Row ---
            let root_type_name = &schema.query_type.name;
            let root_expanded = tab.graphql_expanded_types.contains(root_type_name);

            let root_row = row![
                button(text(if root_expanded { "▼" } else { "▶" }).size(10))
                    .style(button::text)
                    .on_press(Message::GraphqlTypeToggled(root_type_name.clone())),
                text(root_type_name).size(16),
            ]
            .align_y(Alignment::Center);

            let mut scroll_content = Column::new().spacing(0); // Set spacing to 0 for tight rows
            scroll_content = scroll_content.push(
                // container(root_row).padding(Padding::new(0.0).top(8)), // Space before root
                root_row,
            );

            if root_expanded {
                if let Some(root_type) = schema
                    .types
                    .iter()
                    .find(|t| t.name.as_ref() == Some(root_type_name))
                {
                    let tree_rows = render_schema_tree(
                        &schema.types,
                        root_type,
                        &tab.graphql_expanded_types,
                        &tab.graphql_selected_paths,
                        &tab.graphql_search,
                        1, // Indentation level
                        root_type_name,
                    );
                    for row in tree_rows {
                        scroll_content = scroll_content.push(row);
                    }
                }
            }

            column![
                top_bar,
                scrollable(scroll_content)
                    .direction(scrollable::Direction::Both {
                        vertical: scrollable::Scrollbar::default().width(0).scroller_width(0),
                        horizontal: scrollable::Scrollbar::default().width(0).scroller_width(0),
                    })
                    .height(Length::Fill)
                    .width(Length::Fill)
            ]
            .spacing(4) // Tight spacing between bar and content
            .height(Length::Fill)
            .into()
        } else {
            // ... (Empty State - unchanged)
            container(
                button("Fetch Schema")
                    .style(button::text)
                    .on_press(Message::FetchGraphqlSchema),
            )
            .into()
        };

        // ── right: query + variables ──────────
        let operation_row = row![
            text("Operation:").size(12),
            text_input("optional", &tab.graphql_operation)
                .on_input(Message::GraphqlOperationChanged)
                .width(Length::FillPortion(1)),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let right_panel = column![
            operation_row,
            text("Query:").size(12),
            text_editor(&self.graphql_query_content)
                .on_action(Message::GraphqlQueryAction)
                .height(Length::FillPortion(3)),
            text("Variables (JSON):").size(12),
            text_editor(&self.graphql_variables_content)
                .on_action(Message::GraphqlVariablesAction)
                .height(Length::FillPortion(2)),
        ]
        .spacing(8)
        .height(Length::Fill);

        // ── main layout ──────────────────────
        row![
            container(schema_panel)
                .width(Length::FillPortion(1))
                .height(Length::Fill),
            rule::vertical(1),
            container(right_panel)
                .width(Length::FillPortion(2))
                .height(Length::Fill)
        ]
        .spacing(5)
        .height(Length::Fill)
        .into()
    }
}

pub(crate) fn render_schema_tree<'a>(
    types: &'a [GraphqlType],
    current_type: &'a GraphqlType,
    expanded: &std::collections::HashSet<String>,
    selected_paths: &std::collections::HashSet<String>,
    search: &str,
    depth: u16,
    path: &str,
) -> Vec<Element<'a, Message>> {
    let mut rows: Vec<Element<'a, Message>> = vec![];

    let fields_to_render = current_type
        .fields
        .as_ref()
        .or(current_type.input_fields.as_ref());

    let Some(fields) = fields_to_render else {
        return rows;
    };

    let current_type_name = current_type.name.as_deref().unwrap_or("Unknown");

    let indent_width = depth as f32 * 16.0;

    for field in fields {
        // Search filter
        if !search.is_empty() && !field.name.to_lowercase().contains(&search.to_lowercase()) {
            continue;
        }

        let base_type_name = field
            .field_type
            .get_base_name()
            .map(|s| s.as_str())
            .unwrap_or("Unknown");

        let field_path = if path.is_empty() {
            field.name.clone()
        } else {
            format!("{}.{}", path, field.name)
        };

        // Check expandability
        let target_type = types
            .iter()
            .find(|t| t.name.as_deref() == Some(base_type_name));

        let has_type_children = target_type.map_or(false, |t| {
            t.fields.as_ref().map_or(false, |f| !f.is_empty())
                || t.input_fields.as_ref().map_or(false, |f| !f.is_empty())
        });

        let has_args = !field.args.is_empty();
        let is_expandable = has_type_children || has_args;
        let field_path_clone = field_path.clone();
        let is_expanded = expanded.contains(&field_path);

        // UI Components
        let expander_width = 20.0;

        let expand_btn: Element<'a, Message> = if is_expandable {
            container(
                button(text(if is_expanded { "▼" } else { "▶" }).size(10))
                    .style(button::text)
                    .padding(0)
                    .on_press(Message::GraphqlTypeToggled(field_path_clone.clone())),
            )
            .width(expander_width)
            .center_x(expander_width)
            .into()
        } else {
            Space::new().width(expander_width).into()
        };

        let type_label =
            text(format!("{}", field.field_type))
                .size(13)
                .style(|theme: &iced::Theme| text::Style {
                    color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5)), // Grey out the types
                });

        let is_selected = selected_paths.contains(&field_path);
        let field_path_cloned = field_path.clone();
        let field_row = row![
            Space::new().width(indent_width),
            expand_btn,
            checkbox(is_selected)
                .on_toggle(move |_| Message::GraphqlFieldToggled(field_path_cloned.clone())),
            text(field.name.clone()).size(14),
            type_label,
        ]
        .spacing(4)
        .padding(Padding {
            top: 2.0,
            ..Default::default()
        })
        .align_y(Alignment::Center);

        rows.push(field_row.into());

        // Only recurse if expanded
        if is_expanded {
            // A. Render Arguments AND their nested fields
            for arg in &field.args {
                let arg_path = format!("{}.{}", field_path, arg.name);
                let arg_path_cloned = arg_path.clone();
                let is_arg_selected = selected_paths.contains(&arg_path);
                let is_arg_expanded = expanded.contains(&arg_path);

                // Find if the argument type has sub-fields (is it an Input Object?)
                let arg_base_name = arg
                    .arg_type
                    .get_base_name()
                    .map(|s| s.as_str())
                    .unwrap_or("");

                let arg_target_type = types
                    .iter()
                    .find(|t| t.name.as_deref() == Some(arg_base_name));

                // An argument is expandable if it has input_fields
                let is_arg_expandable = arg_target_type.map_or(false, |t| t.input_fields.is_some());

                // --- Render the Argument Row ---
                let arg_expander: Element<'a, Message> = if is_arg_expandable {
                    button(text(if is_arg_expanded { "▼" } else { "▶" }).size(10))
                        .style(button::text)
                        .padding(0.0)
                        .on_press(Message::GraphqlTypeToggled(arg_path.clone()))
                        .into()
                } else {
                    Space::new().width(20.0).into()
                };

                let t_name = current_type_name.to_string();
                let f_name = field.name.clone();
                let a_name = arg.name.clone();
                // --- Render the Argument Row ---
                let arg_row = row![
                    Space::new().width(indent_width + 16.0),
                    container(arg_expander).width(20.0).center_x(20.0),
                    checkbox(is_arg_selected)
                        .on_toggle(move |_| Message::GraphqlArgToggled(arg_path_cloned.clone())),
                    text(format!("arg: {}", arg.name))
                        .size(13)
                        .style(|_| text::Style {
                            color: Some(iced::Color::from_rgb(0.9, 0.5, 0.1))
                        }),
                    text(format!("({})", arg.arg_type))
                        .size(14)
                        .style(|_| text::Style {
                            color: Some(iced::Color::from_rgb(0.5, 0.5, 0.5))
                        }),
                ]
                .spacing(4)
                .padding(Padding {
                    top: 2.0,
                    ..Default::default()
                })
                .align_y(Alignment::Center);

                rows.push(arg_row.into());

                // --- Recurse into Argument Children (the 'code', 'eq' part) ---
                if is_arg_expanded && is_arg_expandable {
                    if let Some(arg_type_obj) = arg_target_type {
                        let arg_children = render_schema_tree(
                            types,
                            arg_type_obj,
                            expanded,
                            selected_paths,
                            search,
                            depth + 2, // Move children further right
                            &arg_path,
                        );
                        rows.extend(arg_children);
                    }
                }
            }

            // B. Recurse into child fields (Return Types)
            if let Some(child_type_obj) = target_type {
                let children = render_schema_tree(
                    types,
                    child_type_obj,
                    expanded,
                    selected_paths,
                    search,
                    depth + 1,
                    &field_path,
                );
                rows.extend(children);
            }
        }
    }

    rows
}

//! Collections sidebar and the "save request" modal.

use iced::{Alignment, Border, Element, Length};
use iced::widget::{
    button,
    column,
    container,
    pick_list,
    row,
    rule,
    scrollable,
    space,
    text,
    text_input,
    tooltip,
};

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::collection::CollectionItem;
use crate::domain::http::HttpMethod;

impl CrabiPie {
    pub(crate) fn render_sidebar(&self) -> Element<'_, Message> {
        // Header
        let header = row![
            text("Collection").size(14),
            space::horizontal(),
            tooltip(
                button(text("+📁").shaping(text::Shaping::Advanced).size(12))
                    .style(button::text)
                    .on_press(Message::CollectionFolderAdd(None)),
                "New folder at root",
                tooltip::Position::Bottom
            ),
            tooltip(
                button(text("💾").shaping(text::Shaping::Advanced).size(12))
                    .style(button::text)
                    .on_press(Message::OpenSaveModal),
                "Save current tab",
                tooltip::Position::Bottom
            ),
        ]
        .align_y(Alignment::Center)
        .spacing(4);

        // Render items recursively
        let items: Element<'_, Message> = if self.collection.items.is_empty() {
            container(
                text("No saved requests.\nClick 💾 to save current tab.")
                    .size(12)
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .padding(10)
            .into()
        } else {
            column(
                self.collection
                    .items
                    .iter()
                    .map(|item| self.render_collection_item(item, 0))
                    .collect::<Vec<_>>(),
            )
            .spacing(2)
            .into()
        };

        let content = column![
            header,
            rule::horizontal(1.0),
            scrollable(items).height(Length::Fill),
        ]
        .spacing(8);

        container(content)
            .width(Length::Fixed(240.0))
            .height(Length::Fill)
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.extended_palette().background.weak.color,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .padding(8)
            .into()
    }

    pub(crate) fn render_collection_item<'a>(
        &'a self,
        item: &'a CollectionItem,
        depth: u16,
    ) -> Element<'a, Message> {
        let indent = depth as f32 * 14.0;
        let is_selected = self.sidebar_selected_id == Some(item.id());

        match item {
            CollectionItem::Folder(folder) => {
                let arrow = if folder.expanded { "▾" } else { "▸" };

                let name_or_input: Element<'_, Message> =
                    if self.sidebar_editing_id == Some(folder.id) {
                        text_input("Folder name...", &self.sidebar_editing_name)
                            .id("sidebar_rename")
                            .on_input(Message::CollectionItemRenameInput)
                            .on_submit(Message::CollectionItemRenameConfirm(folder.id))
                            .size(13)
                            .into()
                    } else {
                        let row_btn = button(
                            row![
                                text(arrow).size(12),
                                text("📁").shaping(text::Shaping::Advanced).size(12),
                                text(&folder.name).size(13),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .style(move |theme: &iced::Theme, status| {
                            if is_selected {
                                button::Style {
                                    background: Some(iced::Background::Color(
                                        theme.extended_palette().primary.weak.color,
                                    )),
                                    text_color: theme.extended_palette().primary.weak.text,
                                    border: Border::default(),
                                    shadow: iced::Shadow::default(),
                                    snap: false,
                                }
                            } else {
                                button::text(theme, status)
                            }
                        })
                        .width(Length::Fill)
                        .on_press(Message::SidebarItemSelected(folder.id));

                        iced_aw::ContextMenu::new(row_btn, move || {
                            let context_items = column![
                                button(
                                    row![text("📁+").size(12), text(" Add Subfolder").size(13)]
                                        .spacing(4)
                                )
                                .style(button::text)
                                .width(Length::Fill)
                                .on_press(Message::CollectionFolderAdd(Some(folder.id))),
                                button(
                                    row![
                                        text("✏️").shaping(text::Shaping::Advanced).size(12),
                                        text(" Rename").size(13)
                                    ]
                                    .spacing(4)
                                )
                                .style(button::text)
                                .width(Length::Fill)
                                .on_press(Message::CollectionItemRename(folder.id)),
                                button(
                                    row![
                                        text("🗑").shaping(text::Shaping::Advanced).size(12),
                                        text(" Delete").size(13)
                                    ]
                                    .spacing(4)
                                )
                                .style(button::text)
                                .width(Length::Fill)
                                .on_press(Message::CollectionItemDelete(folder.id)),
                            ]
                            .padding(4);

                            container(context_items)
                                .style(|theme: &iced::Theme| container::Style {
                                    background: Some(iced::Background::Color(
                                        theme.palette().background,
                                    )),
                                    border: Border {
                                        width: 1.0,
                                        color: theme.extended_palette().background.weak.color,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                })
                                .width(Length::Fixed(160.0))
                                .into()
                        })
                        .into()
                    };

                let row_content = row![space::horizontal().width(indent), name_or_input,]
                    .align_y(Alignment::Center);

                if folder.expanded {
                    let children: Vec<Element<'_, Message>> = folder
                        .children
                        .iter()
                        .map(|child| self.render_collection_item(child, depth + 1))
                        .collect();

                    column![row_content, column(children).spacing(2)]
                        .spacing(2)
                        .into()
                } else {
                    row_content.into()
                }
            }

            CollectionItem::Request(req) => {
                let method_color = match req.method {
                    HttpMethod::GET => iced::Color::from_rgb(0.27, 0.73, 0.27),
                    HttpMethod::POST => iced::Color::from_rgb(0.98, 0.65, 0.14),
                    HttpMethod::PUT => iced::Color::from_rgb(0.14, 0.59, 0.98),
                    HttpMethod::DELETE => iced::Color::from_rgb(0.95, 0.26, 0.21),
                    HttpMethod::PATCH => iced::Color::from_rgb(0.61, 0.15, 0.69),
                    _ => iced::Color::from_rgb(0.5, 0.5, 0.5),
                };

                let name_or_input: Element<'_, Message> = if self.sidebar_editing_id == Some(req.id)
                {
                    text_input("Request name...", &self.sidebar_editing_name)
                        .id("sidebar_rename")
                        .on_input(Message::CollectionItemRenameInput)
                        .on_submit(Message::CollectionItemRenameConfirm(req.id))
                        .size(13)
                        .into()
                } else {
                    let row_btn = button(
                        row![
                            text(req.method.to_string()).size(10).color(method_color),
                            text(&req.name).size(13),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .style(move |theme: &iced::Theme, status| {
                        if is_selected {
                            button::Style {
                                background: Some(iced::Background::Color(
                                    theme.extended_palette().primary.weak.color,
                                )),
                                text_color: theme.extended_palette().primary.weak.text,
                                border: Border::default(),
                                shadow: iced::Shadow::default(),
                                snap: false,
                            }
                        } else {
                            button::text(theme, status)
                        }
                    })
                    .width(Length::Fill)
                    .on_press(Message::SidebarItemSelected(req.id));

                    let req_id = req.id;
                    iced_aw::ContextMenu::new(row_btn, move || {
                        let context_items = column![
                            button(
                                row![text("↗").size(12), text(" Open in new tab").size(13)]
                                    .spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionRequestOpen(req_id)),
                            button(
                                row![
                                    text("✏️").shaping(text::Shaping::Advanced).size(12),
                                    text(" Rename").size(13)
                                ]
                                .spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionItemRename(req_id)),
                            button(
                                row![text("⧉").size(12), text(" Duplicate").size(13)].spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionItemDuplicate(req_id)),
                            button(
                                row![
                                    text("🗑").shaping(text::Shaping::Advanced).size(12),
                                    text(" Delete").size(13)
                                ]
                                .spacing(4)
                            )
                            .style(button::text)
                            .width(Length::Fill)
                            .on_press(Message::CollectionItemDelete(req_id)),
                        ]
                        .padding(4);

                        container(context_items)
                            .style(|theme: &iced::Theme| container::Style {
                                background: Some(iced::Background::Color(
                                    theme.palette().background,
                                )),
                                border: Border {
                                    width: 1.0,
                                    color: theme.extended_palette().background.weak.color,
                                    radius: 4.0.into(),
                                },
                                ..Default::default()
                            })
                            .width(Length::Fixed(160.0))
                            .into()
                    })
                    .into()
                };

                row![space::horizontal().width(indent), name_or_input,]
                    .align_y(Alignment::Center)
                    .into()
            }
        }
    }

    pub(crate) fn render_save_modal(&self) -> Element<'_, Message> {
        // Build folder list for picker — flatten all folders
        let mut folder_options: Vec<(Option<usize>, String)> = vec![(None, "Root".to_string())];

        fn collect_folders(
            items: &[CollectionItem],
            depth: usize,
            out: &mut Vec<(Option<usize>, String)>,
        ) {
            for item in items {
                if let CollectionItem::Folder(f) = item {
                    let indent = "  ".repeat(depth);
                    out.push((Some(f.id), format!("{}📁 {}", indent, f.name)));
                    collect_folders(&f.children, depth + 1, out);
                }
            }
        }
        collect_folders(&self.collection.items, 0, &mut folder_options);

        let folder_labels: Vec<String> = folder_options.iter().map(|(_, l)| l.clone()).collect();

        let selected_label = folder_options
            .iter()
            .find(|(id, _)| *id == self.save_modal_folder_id)
            .map(|(_, l)| l.clone())
            .unwrap_or("Root".to_string());

        let modal_content = column![
            text("Save to Collection").size(16),
            rule::horizontal(1.0),
            column![
                text("Name").size(12),
                text_input("Request name...", &self.save_modal_name)
                    .on_input(Message::SaveModalNameChanged)
                    .on_submit(Message::SaveModalConfirm)
                    .padding(8),
            ]
            .spacing(4),
            column![
                text("Save into folder").size(12),
                pick_list(folder_labels, Some(selected_label), move |selected| {
                    let folder_id = folder_options
                        .iter()
                        .find(|(_, l)| l == &selected)
                        .map(|(id, _)| *id)
                        .unwrap_or(None);
                    Message::SaveModalFolderSelected(folder_id)
                },)
                .padding(8)
                .width(Length::Fill),
            ]
            .spacing(4),
            row![
                space::horizontal(),
                button("Cancel")
                    .style(button::secondary)
                    .on_press(Message::SaveModalCancel)
                    .padding(8),
                button("Save")
                    .style(button::primary)
                    .on_press(Message::SaveModalConfirm)
                    .padding(8),
            ]
            .spacing(8),
        ]
        .spacing(12);

        container(modal_content)
            .width(Length::Fixed(340.0))
            .padding(20)
            .style(|theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(theme.palette().background)),
                border: Border {
                    width: 1.0,
                    color: theme.palette().primary,
                    radius: 8.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}

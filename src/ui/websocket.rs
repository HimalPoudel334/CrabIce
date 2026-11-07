//! WebSocket panel.

use iced::{Alignment, Border, Element, Length, Padding};
use iced::widget::{button, column, container, pick_list, row, text, text_editor, text_input};

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::websocket::{WsBinaryMessageType, WsMessageType};
use crate::ui::json_highlighter;

impl CrabiPie {
    pub(crate) fn render_websocket_panel(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };
        let is_connected = tab.ws_connected;

        // Connection controls
        let url_input = text_input("wss://echo.websocket.org", &tab.url)
            .on_input(Message::UrlChanged)
            .padding(10)
            .width(Length::Fill);

        let connect_button = if is_connected {
            button(text("Disconnect").size(14))
                .on_press(Message::WsDisconnect)
                .padding(10)
                .style(button::danger)
        } else if tab.loading {
            button(text("Connecting...").size(14)).padding(10)
        } else {
            button(text("Connect").size(14))
                .on_press(Message::WsConnect)
                .padding(10)
                .style(button::primary)
        };

        let clear_button = button(text("Clear").size(14))
            .on_press(Message::WsClearMessages)
            .padding(10);

        let status_text = if is_connected {
            text("🟢 Connected").color(iced::Color::from_rgb(0.0, 0.8, 0.0))
        } else {
            text("🔴 Disconnected").color(iced::Color::from_rgb(0.8, 0.0, 0.0))
        };

        let connection_row = row![url_input, connect_button, clear_button, status_text,]
            .spacing(10)
            .padding(Padding::new(0.0).top(10.0))
            .align_y(Alignment::Center);

        let editor = text_editor(&tab.ws_messages_content)
            .on_action(Message::WsMessageEditorAction)
            .highlight_with::<json_highlighter::LogHighlighter>((), |color, _theme| {
                iced::advanced::text::highlighter::Format {
                    color: Some(*color),
                    font: None,
                }
            })
            .height(Length::Fill)
            .style(|theme: &iced::Theme, status| {
                let mut style = text_editor::Catalog::style(
                    theme,
                    &<iced::Theme as text_editor::Catalog>::default(),
                    status,
                );
                style.border.width = 0.0;
                // style.background = iced::Background::Color(iced::Color::TRANSPARENT);
                style
            });

        let messages_area = container(editor)
            .height(Length::Fill)
            .padding(10)
            .style(|th| container::Style {
                background: Some(iced::Background::Color(th.palette().background)),
                border: Border {
                    width: 1.0,
                    color: th.extended_palette().background.weak.color,
                    radius: 5.0.into(),
                },
                ..Default::default()
            });

        let mut input_row = iced::widget::Row::new().spacing(10);

        input_row = input_row.push(
            pick_list(
                &WsMessageType::ALL[..],
                Some(tab.ws_message_type),
                Message::WsMessageTypeSelected,
            )
            .padding(8),
        );

        if tab.ws_message_type == WsMessageType::Binary {
            input_row = input_row.push(
                pick_list(
                    &WsBinaryMessageType::ALL[..],
                    Some(tab.ws_binary_message_type),
                    Message::WsBinaryMessageTypeSelected,
                )
                .padding(8),
            );
        }

        // Message input area
        input_row = input_row.push(
            text_input("Type message...", &tab.ws_input)
                .on_input(Message::WsMessageInputChanged)
                .on_submit(Message::WsSendMessage)
                .padding(8)
                .width(Length::Fill),
        );

        input_row = input_row.push(
            button(text("Send").size(14))
                .on_press_maybe(if is_connected && !tab.ws_input.is_empty() {
                    Some(Message::WsSendMessage)
                } else {
                    None
                })
                .padding(10)
                .style(button::primary),
        );

        // Update stats to use the counters
        let stats = text(format!(
            "Sent: {} | Received: {}",
            tab.ws_count_sent, tab.ws_count_received
        ))
        .size(11)
        .color(iced::Color::from_rgb(0.5, 0.5, 0.5));

        column![
            connection_row,
            messages_area,
            container(stats).padding(5).center_x(Length::Fill),
            input_row,
        ]
        .spacing(10)
        .into()
    }
}

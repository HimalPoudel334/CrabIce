//! Find / replace dialog.

use iced::{Border, Element, Length};
use iced::widget::{Space, button, column, container, row, text, text_input, tooltip};

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::ui::json_highlighter;

impl CrabiPie {
    pub(crate) fn get_highlighter_settings(&self) -> json_highlighter::JsonHighlighterSettings {
        json_highlighter::JsonHighlighterSettings::new(self.json_theme).with_search(
            self.search_match_positions.clone(),
            self.current_match_line_col,
            self.search_match_length,
        )
    }

    pub(crate) fn view_find_replace(&self) -> Element<'_, Message> {
        let toggle: Element<'_, Message> = tooltip(
            button(
                text(if !self.find_replace_mode {
                    "⬇️"
                } else {
                    "⬆️"
                })
                .shaping(text::Shaping::Advanced),
            )
            .style(button::text)
            .on_press(Message::ToggleFindReplaceDialog),
            "Toggle between find and replace",
            tooltip::Position::Bottom,
        )
        .into();

        let find_input: Element<'_, Message> = text_input("Find...", &self.find_text)
            .id("find_input")
            .on_input(Message::FindTextChanged)
            .into();

        let find_btns_row = row![
            tooltip(
                button(text("🔍").shaping(text::Shaping::Advanced))
                    .style(button::text)
                    .on_press(Message::FindNext),
                "Find Next",
                tooltip::Position::Bottom
            ),
            tooltip(
                button(text("✖").shaping(text::Shaping::Advanced))
                    .style(button::text)
                    .on_press(Message::CloseFindDialog),
                "Close",
                tooltip::Position::Bottom
            )
        ];

        let replace_input_or_space: Element<'_, Message> = if self.find_replace_mode {
            text_input("Replace with...", &self.replace_text)
                .id("replace_input")
                .on_input(Message::ReplaceTextChanged)
                .into()
        } else {
            Space::new().into()
        };

        let replace_btns_or_space: Element<'_, Message> = if self.find_replace_mode {
            row![
                tooltip(
                    button(text("✏️").shaping(text::Shaping::Advanced))
                        .style(button::text)
                        .on_press(Message::FindNext),
                    "Replace Next",
                    tooltip::Position::Bottom
                ),
                tooltip(
                    button(text("🔁").shaping(text::Shaping::Advanced))
                        .style(button::text)
                        .on_press(Message::ReplaceAll),
                    "Replace All",
                    tooltip::Position::Bottom
                ),
            ]
            .align_y(iced::Alignment::End)
            .into()
        } else {
            Space::new().into()
        };

        let match_info: Element<'_, Message> = if !self.find_text.is_empty() {
            text(format!(
                "{} / {} matches",
                self.current_match, self.total_matches,
            ))
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            Space::new().into()
        };

        let find_mode_buttons = row![
            tooltip(
                button("Aa")
                    .style(if !self.case_sensitive {
                        button::text
                    } else {
                        button::subtle
                    })
                    .on_press(Message::ToggleCaseSensitive),
                "Match case",
                tooltip::Position::Bottom
            ),
            tooltip(
                button("[ab]")
                    .style(if !self.whole_word {
                        button::text
                    } else {
                        button::subtle
                    })
                    .on_press(Message::ToggleWholeWord),
                "Match whole word",
                tooltip::Position::Bottom
            ),
            match_info
        ];

        let find_replace_col = column![find_input, replace_input_or_space].spacing(5.0);

        let content = row![
            toggle,
            column![find_replace_col, find_mode_buttons].spacing(5.0),
            column![find_btns_row, replace_btns_or_space]
        ];

        container(content)
            .width(Length::Fixed(400.0))
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.palette().primary,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .padding(5.0)
            .into()
    }
}

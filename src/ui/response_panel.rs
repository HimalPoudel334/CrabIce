//! Response section: body, headers, status line.

use iced::{Alignment, Border, Element, Length, Padding};
use iced::widget::{
    Space,
    button,
    column,
    container,
    pick_list,
    rule,
    scrollable,
    space,
    text,
    text_editor,
    tooltip,
};

use crate::app::CrabiPie;
use crate::app::message::{JsonTarget, Message};
use crate::domain::view_state::ResponseTab;
use crate::ui::json_highlighter;

impl CrabiPie {
    pub(crate) fn format_duration(dur: std::time::Duration) -> String {
        let secs = dur.as_secs_f64();
        if secs < 1.0 {
            format!("{:.1}ms", secs * 1000.0)
        } else if secs < 60.0 {
            format!("{:.2}s", secs)
        } else if secs < 3600.0 {
            let m = secs as u64 / 60;
            let s = secs as u64 % 60;
            format!("{}m {}s", m, s)
        } else {
            let h = secs as u64 / 3600;
            let m = (secs as u64 % 3600) / 60;
            format!("{}h {}m", h, m)
        }
    }

    pub(crate) fn render_response_section(&self) -> Element<'_, Message> {
        fn status_color(status: &str) -> iced::Color {
            let code = status
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u16>().ok());

            match code {
                Some(200..=299) => iced::Color::from_rgb(0.2, 0.8, 0.2), // green
                Some(300..=399) => iced::Color::from_rgb(0.2, 0.6, 0.9), // blue
                Some(400..=499) => iced::Color::from_rgb(0.9, 0.6, 0.2), // orange
                Some(500..=599) => iced::Color::from_rgb(0.9, 0.2, 0.2), // red
                _ => iced::Color::WHITE,
            }
        }

        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let status_view: Element<'_, Message> = if tab.loading {
            text("Loading...").into()
        } else if !tab.response_status.is_empty() {
            text(&tab.response_status)
                .color(status_color(&tab.response_status))
                .into()
        } else {
            Space::new().into()
        };

        let mut header_row = iced::widget::Row::new()
            .spacing(10)
            .height(20)
            .align_y(Alignment::Center);

        header_row = header_row.push(text("Response"));
        header_row = header_row.push(status_view);

        if let Some(resp_time) = tab.response_time {
            header_row = header_row.push(
                text(format!("⏱️ {}", Self::format_duration(resp_time)))
                    .shaping(text::Shaping::Advanced),
            );
        }
        if tab.is_response_binary {
            header_row = header_row.push(
                text(format!(
                    "🗃️ {:.2} KB",
                    tab.response_bytes.len() as f32 / 1024.0
                ))
                .shaping(text::Shaping::Advanced),
            );
        }
        header_row = header_row.push(space::horizontal());
        header_row = header_row.push(text("Json Theme:"));
        header_row = header_row.push(pick_list(
            &json_highlighter::JsonThemeWrapper::ALL[..],
            Some(&self.json_theme),
            Message::JsonThemeChanged,
        ));
        if !tab.response_body.is_empty() || !tab.response_headers.is_empty() {
            header_row = header_row.push(tooltip(
                button(text(if tab.copied { "✅" } else { "📋" }).shaping(text::Shaping::Advanced))
                    .on_press(Message::CopyToClipboard)
                    .style(button::text),
                if tab.copied {
                    "Copied"
                } else {
                    "Copy to Clipboard"
                },
                tooltip::Position::Bottom,
            ));
        }
        let prettify_target = match tab.active_response_tab {
            ResponseTab::Body => {
                if tab.is_response_binary {
                    None
                } else {
                    Some(JsonTarget::ResponseBody)
                }
            }
            ResponseTab::Headers => Some(JsonTarget::ResponseHeaders),
        };

        let prettify_button =
            button(text("✨").shaping(text::Shaping::Advanced)).style(button::text);

        let prettify_button = if let Some(target) = prettify_target {
            prettify_button.on_press(Message::PrettifyJson(target))
        } else {
            prettify_button
        };

        header_row = header_row.push(tooltip(
            prettify_button,
            "Prettify",
            tooltip::Position::Bottom,
        ));
        header_row = header_row.push(tooltip(
            button(text("🧹").shaping(text::Shaping::Advanced))
                .on_press(Message::ClearResponseText)
                .style(button::text),
            "Clear",
            tooltip::Position::Bottom,
        ));

        let res_tabs: iced_aw::Tabs<Message, ResponseTab, iced::Theme, iced::Renderer> =
            iced_aw::Tabs::new(Message::ResponseTabSelected)
                .push(
                    ResponseTab::Body,
                    iced_aw::TabLabel::Text("Body".into()),
                    container(self.with_overlay(self.render_response_body())).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .push(
                    ResponseTab::Headers,
                    iced_aw::TabLabel::Text("Header".into()),
                    container(self.with_overlay(self.render_response_headers())).padding(Padding {
                        top: 10.0,
                        ..Default::default()
                    }),
                )
                .height(Length::Fill)
                .set_active_tab(&tab.active_response_tab)
                .tab_bar_position(iced_aw::TabBarPosition::Top)
                .into();

        container(column![header_row, rule::horizontal(1.0), res_tabs].spacing(10))
            .style(|theme: &iced::Theme| container::Style {
                border: Border {
                    width: 0.5,
                    color: theme.palette().primary,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .height(Length::Fill)
            .padding(5.0)
            .into()
    }

    pub(crate) fn render_response_body(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        if tab.is_response_binary {
            let mut body_column = iced::widget::Column::new().spacing(5);
            body_column = body_column.push(
                button(text("💾 Save").shaping(text::Shaping::Advanced))
                    .on_press(Message::SaveBinaryResponse)
                    .style(|_, _| button::Style {
                        text_color: iced::Color::from_rgb(1.0, 0.65, 0.0),
                        background: None,
                        ..Default::default()
                    }),
            );

            if tab.response_content_type.starts_with("image/") {
                if let Some(handle) = &tab.image_handle {
                    body_column = body_column.push(
                        scrollable(
                            iced::widget::image(handle.clone())
                                .content_fit(iced::ContentFit::Contain),
                        )
                        .height(Length::Fill)
                        .width(Length::Fill),
                    );
                }
            } else if tab.response_content_type.starts_with("application/pdf") {
                if let (Some(pdf), Some(page_image)) = (&tab.pdf_viewer, &tab.pdf_current_image) {
                    body_column = body_column.push(
                        iced::widget::row![
                            button(text("◀").shaping(text::Shaping::Advanced))
                                .on_press(Message::PdfPreviousPage)
                                .style(button::text),
                            text(format!(
                                "Page {} / {}",
                                pdf.current_page + 1,
                                pdf.page_count()
                            )),
                            button(text("▶").shaping(text::Shaping::Advanced))
                                .on_press(Message::PdfNextPage)
                                .style(button::text),
                        ]
                        .height(15)
                        .spacing(10),
                    );
                    body_column = body_column.push(
                        scrollable(
                            iced::widget::image(page_image.clone())
                                .content_fit(iced::ContentFit::Contain),
                        )
                        .height(Length::Fill)
                        .width(Length::Fill),
                    );
                } else {
                    body_column = body_column.push(
                        text("📄 Rendering PDF...")
                            .shaping(text::Shaping::Advanced)
                            .style(|_| text::Style {
                                color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                            }),
                    );
                }
            } else if tab.response_content_type.starts_with("video/") {
                // Video playback
                if let Some(video) = &tab.video_player {
                    let vs = tab.video_state.as_ref().unwrap();
                    body_column = body_column
                        .push(
                            container::Container::new(
                                iced_video_player::VideoPlayer::new(video)
                                    .width(iced::Length::Fill)
                                    .height(iced::Length::Fill)
                                    .content_fit(iced::ContentFit::Contain)
                                    .on_end_of_stream(Message::EndOfStream)
                                    .on_new_frame(Message::NewFrame),
                            )
                            .align_x(iced::Alignment::Center)
                            .align_y(iced::Alignment::Center)
                            .width(iced::Length::Fill)
                            .height(iced::Length::Fill),
                        )
                        .push(
                            container::Container::new(
                                iced::widget::Slider::new(
                                    0.0..=video.duration().as_secs_f64(),
                                    vs.position,
                                    Message::Seek,
                                )
                                .step(0.1)
                                .on_release(Message::SeekRelease),
                            )
                            .padding(Padding {
                                right: 10.0,
                                left: 10.0,
                                top: 5.0,
                                bottom: 5.0,
                            }),
                        )
                        .spacing(4)
                        .push(
                            iced::widget::Row::new()
                                .spacing(2)
                                .align_y(iced::alignment::Vertical::Center)
                                .padding(iced::Padding::new(10.0).top(0.0))
                                .push(
                                    button::Button::new(
                                        text::Text::new(if video.paused() {
                                            "▶️"
                                        } else {
                                            "⏸️"
                                        })
                                        .shaping(text::Shaping::Advanced),
                                    )
                                    .style(button::text)
                                    .on_press(Message::TogglePause),
                                )
                                .push(
                                    button::Button::new(
                                        text::Text::new(if video.looping() {
                                            "🔁❌"
                                        } else {
                                            "🔁"
                                        })
                                        .shaping(text::Shaping::Advanced),
                                    )
                                    .style(button::text)
                                    .on_press(Message::ToggleLoop),
                                )
                                .push(
                                    text::Text::new(format!(
                                        "{}:{:02}s / {}:{:02}s",
                                        vs.position as u64 / 60,
                                        vs.position as u64 % 60,
                                        video.duration().as_secs() / 60,
                                        video.duration().as_secs() % 60,
                                    ))
                                    .width(iced::Length::Fill)
                                    .align_x(iced::alignment::Horizontal::Right),
                                ),
                        );
                } else {
                    body_column = body_column.push(
                        text("🎬 Loading video...")
                            .shaping(text::Shaping::Advanced)
                            .style(|_| text::Style {
                                color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                            }),
                    );
                }
            } else {
                body_column = body_column.push(
                    text(format!(
                        "📄 Binary file received: {}",
                        tab.response_filename
                    ))
                    .shaping(text::Shaping::Advanced)
                    .style(|_| text::Style {
                        color: Some(iced::Color::from_rgb(1.0, 0.65, 0.0)),
                    }),
                );
                body_column =
                    body_column.push(text(format!("Size: {} bytes", tab.response_bytes.len())));
            }
            body_column.into()
        } else {
            let content: Element<'_, Message> = if tab.is_streaming {
                const LIVE_VIEW_CAP: usize = 64 * 1024; // ~64KB tail while streaming
                let buf = tab.stream_buffer.as_str();
                let visible: &str = if buf.len() > LIVE_VIEW_CAP {
                    // find a char boundary so we don't panic slicing mid-UTF8-codepoint
                    let start = buf.len() - LIVE_VIEW_CAP;
                    let safe_start = (start..buf.len())
                        .find(|&i| buf.is_char_boundary(i))
                        .unwrap_or(buf.len());
                    &buf[safe_start..]
                } else {
                    buf
                };
                scrollable(text(visible).font(iced::Font::MONOSPACE).size(13))
                    .height(Length::Fill)
                    .width(Length::Fill)
                    .id(format!("stream-scroll-{}", tab.id))
                    .into()
            } else if tab.response_body.is_empty() {
                space().into()
            } else {
                text_editor(&self.response_body_content)
                    .on_action(Message::ResponseBodyAction)
                    .highlight_with::<json_highlighter::JsonHighlighter>(
                        self.get_highlighter_settings(),
                        |highlight, _theme| {
                            let color = match highlight {
                                json_highlighter::HighlightType::Syntax(color) => *color,
                                json_highlighter::HighlightType::SearchMatch => {
                                    iced::Color::from_rgb(1.0, 1.0, 0.0)
                                }
                                json_highlighter::HighlightType::CurrentMatch => {
                                    iced::Color::from_rgb(1.0, 0.0, 1.0)
                                }
                            };

                            iced::advanced::text::highlighter::Format {
                                color: Some(color),
                                font: None,
                            }
                        },
                    )
                    .wrapping(iced::advanced::text::Wrapping::Glyph)
                    .style(Self::get_editor_style)
                    .into()
            };
            scrollable(content).height(Length::FillPortion(1)).into()
        }
    }

    pub(crate) fn get_editor_style(theme: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
        let mut style = text_editor::Catalog::style(
            theme,
            &<iced::Theme as text_editor::Catalog>::default(),
            status,
        );

        style.border.width = 0.0;

        style
    }

    pub(crate) fn render_response_headers(&self) -> Element<'_, Message> {
        let Some(tab) = self.current_tab() else {
            return iced::widget::text("Loading...").into();
        };

        let content: Element<'_, Message> = /*if tab.response_headers.is_empty() {
            space().into()
        } else {*/
            text_editor(&self.response_headers_content)
                .on_action(Message::ResponseHeadersAction)
                .height(Length::FillPortion(1))
                .highlight_with::<json_highlighter::JsonHighlighter>(
                    self.get_highlighter_settings(),
                    |highlight, _theme| {
                        let color = match highlight {
                            json_highlighter::HighlightType::Syntax(color) => *color,
                            json_highlighter::HighlightType::SearchMatch => {
                                iced::Color::from_rgb(1.0, 1.0, 0.0)
                            }
                            json_highlighter::HighlightType::CurrentMatch => {
                                iced::Color::from_rgb(1.0, 0.5, 0.0)
                            }
                        };
                        iced::advanced::text::highlighter::Format {
                            color: Some(color),
                            font: None,
                        }
                    },
                )
                .style(|theme: &iced::Theme, status| Self::get_editor_style(theme, status))
                .into();
        //};

        content
    }
}

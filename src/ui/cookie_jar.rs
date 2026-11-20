//! Cookie jar modal.

use iced::{Alignment, Element, Length};
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
    text_input,
    tooltip,
};

use crate::app::CrabiPie;
use crate::app::message::Message;

impl CrabiPie {
    pub(crate) fn render_cookie_jar_modal(&self) -> Element<'_, Message> {
        if !self.cookie_jar_open {
            return Space::new().into();
        }

        let content: Element<'_, Message> = if self.cookie_jar.is_empty() {
            text("No cookies stored yet.").into()
        } else {
            let domain_sections: Vec<Element<'_, Message>> = self
                .cookie_jar
                .iter()
                .map(|(domain, cookies)| {
                    let domain_header: Element<'_, Message> = row![
                        text(domain.clone()),
                        space::horizontal(),
                        tooltip(
                            button(text("➕").shaping(text::Shaping::Advanced))
                                .on_press(Message::CookieJarAdd(domain.clone())),
                            "Add",
                            tooltip::Position::Bottom
                        ),
                        tooltip(
                            button(text("🧹").shaping(text::Shaping::Advanced))
                                .on_press(Message::CookieJarClearDomain(domain.clone())),
                            "Clear",
                            tooltip::Position::Bottom
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into();

                    let col_headers: Element<'_, Message> = row![
                        text("").width(24), // checkbox space
                        text("Name").size(11).width(Length::FillPortion(2)),
                        text("Value").size(11).width(Length::FillPortion(3)),
                        text("Expires").size(11).width(Length::FillPortion(2)),
                        text("").width(30), // delete btn space
                    ]
                    .spacing(8)
                    .into();

                    let cookie_rows: Vec<Element<'_, Message>> = cookies
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let d1 = domain.clone();
                            let d2 = domain.clone();
                            let d3 = domain.clone();
                            let d4 = domain.clone();
                            row![
                                checkbox(c.enabled)
                                    .on_toggle(move |_| Message::CookieJarToggled(d1.clone(), i)),
                                text_input("", &c.name)
                                    .on_input(move |v| Message::CookieJarNameChanged(
                                        d2.clone(),
                                        i,
                                        v
                                    ))
                                    .width(Length::FillPortion(2)),
                                text_input("", &c.value)
                                    .on_input(move |v| Message::CookieJarValueChanged(
                                        d3.clone(),
                                        i,
                                        v
                                    ))
                                    .width(Length::FillPortion(3)),
                                text(c.expires.as_deref().unwrap_or("session")).size(11),
                                button(text("❌").shaping(text::Shaping::Advanced))
                                    .style(button::text)
                                    .on_press(Message::CookieJarRemove(d4.clone(), i)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .into()
                        })
                        .collect();

                    column![
                        domain_header,
                        col_headers,
                        Column::with_children(cookie_rows).spacing(6),
                    ]
                    .spacing(6)
                    .into()
                })
                .collect();

            scrollable(Column::with_children(domain_sections).spacing(16))
                .height(Length::Fixed(300.0))
                .into()
        };

        let add_row: Element<'_, Message> = row![
            text_input("domain (e.g. api.example.com)", &self.cookie_jar_new_domain)
                .on_input(Message::CookieJarDomainChanged)
                .width(Length::Fill),
            button("+ Add Cookie")
                .on_press(Message::CookieJarAdd(self.cookie_jar_new_domain.clone())),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        let error_el: Element<'_, Message> = match &self.cookie_jar_error {
            Some(err) => text(err.clone())
                .style(|theme| text::Style {
                    color: Some(iced::Color::from_rgb(1.0, 0.3, 0.3)),
                })
                .into(),
            None => Space::new().into(),
        };

        let modal_body = column![
            column![
                row![
                    text("Cookie Jar"),
                    space::horizontal(),
                    button("✕")
                        .on_press(Message::CookieJarClose)
                        .style(button::text),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                rule::horizontal(1),
            ],
            content,
            column![error_el, add_row]
        ]
        .spacing(12)
        .padding(20);

        let modal = container(modal_body)
            .width(Length::Fixed(640.0))
            .max_height(520)
            .style(container::rounded_box);

        let overlay = container(
            container(modal)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme| container::Style {
            background: Some(iced::Background::Color(iced::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            ..container::Style::default()
        });

        iced::widget::stack![
            Space::new().width(Length::Fill).height(Length::Fill),
            overlay,
        ]
        .into()
    }
}

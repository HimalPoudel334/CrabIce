//! Top-level `view` and the window chrome (title row, tab bar, overlays).

use iced::{Alignment, Border, Element, Length, Padding};
use iced::widget::{button, column, container, pick_list, row, rule, scrollable, space, text};

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::TabLoadState;
use crate::domain::http::RequestType;

impl CrabiPie {
    pub(crate) fn view_tab_content(&self) -> Element<'_, Message> {
        match &self.tabs[self.active_tab] {
            TabLoadState::Loaded(_) => self.render_active_tab_content(),
            TabLoadState::Loading(_) | TabLoadState::Unloaded(_) => {
                iced::widget::container(iced::widget::text("Loading..."))
                    .center(iced::Fill)
                    .into()
            }
        }
    }

    pub(crate) fn render_tabs(&self) -> Element<'_, Message> {
        let mut tab_bar = iced::widget::Row::new().spacing(2);

        for (index, tab) in self.tabs.iter().enumerate() {
            let is_active = index == self.active_tab;
            let tab_title = tab.metadata().title.clone();

            // Build the tab label (title + close button, as before)
            let close_btn: Element<'_, Message> = if self.tabs.len() > 1 {
                button(text("❌").size(8).shaping(text::Shaping::Advanced))
                    .style(button::text)
                    .on_press(Message::CloseTab(index))
                    .into()
            } else {
                space::horizontal().width(Length::Shrink).into()
            };

            let tab_button = button(
                row![text(tab_title), close_btn]
                    .spacing(5)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::TabSelected(index))
            .style(if is_active {
                button::primary
            } else {
                button::secondary
            });

            // Right-click context menu
            let tab_button = iced_aw::ContextMenu::new(tab_button, move || {
                let menu = column![
                    button(row![text("⧉").size(12), text(" Duplicate").size(13)].spacing(4))
                        .style(button::text)
                        .width(Length::Fill)
                        .on_press(Message::TabDuplicate(index)),
                    button(row![text("❌").size(12), text(" Close").size(13)].spacing(4))
                        .style(button::text)
                        .width(Length::Fill)
                        .on_press(Message::CloseTab(index)),
                ]
                .padding(4);

                container(menu)
                    .style(|theme: &iced::Theme| container::Style {
                        background: Some(iced::Background::Color(theme.palette().background)),
                        border: Border {
                            width: 1.0,
                            color: theme.extended_palette().background.weak.color,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    })
                    .width(Length::Fixed(160.0))
                    .into()
            });

            tab_bar = tab_bar.push(tab_button);
        }

        // Add the "+" button
        let add_button = button(text("+").size(20))
            .on_press(Message::AddNewTab)
            .style(button::text);

        tab_bar = tab_bar.push(add_button);

        let scrolling_tabs = scrollable(tab_bar).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default().width(0).scroller_width(0),
        ));

        container(scrolling_tabs)
            .width(Length::Fill)
            .style(|theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(theme.palette().background)),
                ..Default::default()
            })
            .into()
    }

    pub(crate) fn render_active_tab_content(&self) -> Element<'_, Message> {
        // Render the content using the active tab's data
        column![
            self.render_request_row(),
            row![
                self.render_request_section(),
                self.render_response_section()
            ]
            .spacing(10)
        ]
        .spacing(10)
        .into()
    }

    pub(crate) fn render_title_row(&self) -> Element<'_, Message> {
        let env_names: Vec<String> = std::iter::once("No Environment".to_string())
            .chain(self.environments.iter().map(|e| e.name.clone()))
            .collect();

        // `active_environment()` already resolves against the *current tab's*
        // selection, so this picker is naturally per-tab.
        let selected_env_name = self
            .active_environment()
            .map(|e| e.name.clone())
            .unwrap_or_else(|| "No Environment".to_string());

        let env_picker = row![
            pick_list(env_names.clone(), Some(selected_env_name), {
                let envs = self.environments.clone();
                move |selected| {
                    if selected == "No Environment" {
                        Message::EnvironmentSelected(None)
                    } else {
                        let id = envs.iter().find(|e| e.name == selected).map(|e| e.id);
                        Message::EnvironmentSelected(id)
                    }
                }
            }),
            button(text("⚙️").shaping(text::Shaping::Advanced))
                .style(button::text)
                .height(20)
                .padding(Padding::new(0.0).top(0.0))
                .on_press(Message::EnvPanelOpen),
        ]
        .spacing(4)
        .height(20)
        .align_y(Alignment::Center);

        row![
            button(text(if self.sidebar_open { "◀" } else { "▶" }).size(14))
                .style(button::text)
                .on_press(Message::ToggleSidebar),
            text("CrabiPie HTTP Client").size(16),
            space::horizontal(),
            env_picker,
            text("App theme"),
            pick_list(
                &iced::Theme::ALL[..],
                Some(&self.app_theme),
                Message::AppThemeChanged,
            )
            .padding(4),
            button(text("📂").shaping(text::Shaping::Advanced).size(14))
                .style(button::text)
                .on_press(Message::LoadRequest),
            button(text("💾").shaping(text::Shaping::Advanced).size(14))
                .style(button::text)
                .on_press(Message::SaveRequest)
        ]
        .spacing(10)
        .into()
    }

    pub(crate) fn loading_overlay(&self) -> Option<Element<'_, Message>> {
        let Some(tab) = self.current_tab() else {
            return Some(iced::widget::text("Loading...").into());
        };

        if !tab.loading {
            return None;
        }

        Some(
            container(column![
                iced::widget::svg(iced::advanced::svg::Handle::from_memory(include_bytes!(
                    "../assets/ring-with-bg.svg"
                )))
                .width(80)
                .height(80)
                .rotation(iced::Radians::from(
                    self.svg_rotation * std::f32::consts::PI / 180.0,
                )),
                text("📤 Sending...").shaping(text::Shaping::Advanced)
            ])
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .style(|theme: &iced::Theme| {
                container::Style {
                    background: Some(iced::Background::Color(
                        iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5), // Semi-transparent overlay
                    )),
                    ..Default::default()
                }
            })
            .into(),
        )
    }

    pub(crate) fn with_overlay<'a>(
        &'a self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let content = content.into();
        if let Some(overlay) = self.loading_overlay() {
            iced::widget::stack![content, overlay].into()
        } else {
            content
        }
    }
}

pub(crate) fn view(app: &CrabiPie) -> Element<'_, Message> {
    let tab_content = match app.current_tab() {
        None => iced::widget::container(iced::widget::text("Loading..."))
            .center(iced::Fill)
            .into(),
        Some(tab) => {
            if tab.request_type == RequestType::WebSocket {
                app.render_websocket_panel()
            } else {
                app.render_active_tab_content()
            }
        }
    };

    let main_content = column![
        app.render_title_row(),
        app.render_tabs(),
        rule::horizontal(1.0),
        tab_content
    ]
    .height(Length::Fill);

    let main_content: Element<'_, Message> = if app.find_dialog_open {
        let overlay = container(app.view_find_replace())
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill);
        iced::widget::stack![main_content, overlay].into()
    } else {
        main_content.into()
    };

    // Wrap with sidebar if open
    let body: Element<'_, Message> = if app.sidebar_open {
        row![app.render_sidebar(), rule::vertical(1.0), main_content,]
            .height(Length::Fill)
            .into()
    } else {
        main_content
    };

    // Save modal overlay
    let body: Element<'_, Message> = if app.save_modal_open {
        let overlay = container(app.render_save_modal())
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgba(
                    0.0, 0.0, 0.0, 0.5,
                ))),
                ..Default::default()
            });
        iced::widget::stack![body, overlay].into()
    } else {
        body
    };

    let body: Element<'_, Message> = if app.cookie_jar_open {
        iced::widget::stack![body, app.render_cookie_jar_modal()].into()
    } else {
        body
    };

    let body: Element<'_, Message> = if app.env_panel_open {
        iced::widget::stack![body, app.render_env_panel()].into()
    } else {
        body
    };

    container(body).padding(10).height(Length::Fill).into()
}

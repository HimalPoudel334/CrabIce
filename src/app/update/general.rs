//! `update` handlers for the **general** messages.

use iced::Event;

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::{TabLoadState, TabState};
use crate::domain::persistence::SavedState;
use crate::ui::theme::{json_theme_from_str, theme_from_str};

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::NoOp => iced::Task::none(),
        Message::Tick => {
            app.svg_rotation = (app.svg_rotation + 4.0) % 360.0;
            iced::Task::none()
        }
        Message::JsonThemeChanged(theme) => {
            app.json_theme = theme;
            iced::Task::none()
        }
        Message::AppThemeChanged(theme) => {
            app.app_theme = theme;
            iced::Task::none()
        }
        Message::EventOccurred(event) => {
            use iced::keyboard::{Event as KeyEvent, Key, Modifiers};
            if let Event::Window(window_event) = &event {
                match window_event {
                    iced::window::Event::CloseRequested => {
                        // Save first, then close
                        return app.save_task().chain(iced::exit());
                    }
                    iced::window::Event::Unfocused => {
                        println!("window was unfocused");
                        return iced::Task::none();
                    }
                    _ => {}
                }
            }
            if let Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Left | iced::mouse::Button::Right,
            )) = &event
            {
                if let Some(editing_id) = app.sidebar_editing_id {
                    return iced::widget::operation::is_focused("sidebar_rename").then(
                        move |focused| {
                            if !focused {
                                iced::Task::done(Message::CollectionItemRenameConfirm(editing_id))
                            } else {
                                iced::Task::none()
                            }
                        },
                    );
                }
            }
            if let Event::Keyboard(key_event) = event {
                let Some(tab) = app.current_tab() else {
                    return iced::Task::none();
                };
                match key_event {
                    KeyEvent::KeyPressed { key, modifiers, .. } if modifiers.control() => {
                        if matches!(key, Key::Named(iced::keyboard::key::Named::Tab)) {
                            if app.save_modal_open || app.env_panel_open || app.cookie_jar_open {
                                return iced::Task::none();
                            }
                            let n = app.tabs.len();
                            if n > 1 {
                                let delta: isize = if modifiers.shift() { -1 } else { 1 };
                                let next = (app.active_tab as isize + delta).rem_euclid(n as isize)
                                    as usize;
                                return iced::Task::done(Message::TabSelected(next));
                            }
                            return iced::Task::none();
                        }
                        if let Key::Character(c) = &key {
                            if c.as_str() == "l" {
                                return iced::widget::operation::focus(tab.url_id.clone()).chain(
                                    iced::widget::operation::select_all(tab.url_id.clone()),
                                );
                            } else if c.as_str() == "f" {
                                return iced::Task::done(Message::ToggleFindDialog);
                            } else if c.as_str() == "h" {
                                return iced::Task::done(Message::ToggleFindReplaceDialog);
                            }
                        }
                        if matches!(key, Key::Named(iced::keyboard::key::Named::Enter)) {
                            println!("Key event: ctrl + Enter");
                            return iced::Task::done(Message::SendRequest);
                        }
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Enter),
                        ..
                    } => {
                        let tasks = vec![
                            iced::widget::operation::is_focused("find_input").then(|f| {
                                if f {
                                    iced::Task::done(Message::FindNext)
                                } else {
                                    iced::Task::none()
                                }
                            }),
                            iced::widget::operation::is_focused("replace_input").then(|f| {
                                if f {
                                    iced::Task::done(Message::Replace)
                                } else {
                                    iced::Task::none()
                                }
                            }),
                            iced::widget::operation::is_focused(tab.url_id.clone()).then(|f| {
                                if f {
                                    iced::Task::done(Message::SendRequest)
                                } else {
                                    iced::Task::none()
                                }
                            }),
                        ];
                        return iced::Task::batch(tasks);
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Tab),
                        modifiers,
                        ..
                    } if modifiers.shift() => {
                        println!("Key event: shift + tab");
                        return iced::widget::operation::focus_previous();
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Tab),
                        ..
                    } => {
                        println!("Key event: Tab");
                        return iced::widget::operation::focus_next();
                    }
                    KeyEvent::KeyPressed {
                        key: Key::Named(iced::keyboard::key::Named::Escape),
                        ..
                    } => {
                        if app.sidebar_editing_id.is_some() {
                            app.sidebar_editing_id = None;
                            app.sidebar_editing_name = String::new();
                            return iced::Task::none();
                        }
                        if app.save_modal_open {
                            app.save_modal_open = false;
                            app.save_modal_name = String::new();
                            app.save_modal_folder_id = None;
                            return iced::Task::none();
                        }
                    }
                    _ => {}
                }
            }
            iced::Task::none()
        }
        Message::StateLoaded(maybe_session, metadata) => {
            if let Some(session) = maybe_session {
                app.json_theme = json_theme_from_str(&session.json_theme);
                app.app_theme = theme_from_str(&session.app_theme);
                app.next_tab_id = session.next_tab_id;
                app.cookie_jar = session.cookie_jar;
                app.active_tab = session.active_tab.min(metadata.len().saturating_sub(1));
                app.environments = session.environments;
                app.active_environment_id = session.active_environment_id;
                app.next_environment_id = session.next_environment_id.max(1);
            }

            // build tab list — all Unloaded first
            app.tabs = metadata
                .into_iter()
                .map(|m| TabLoadState::Unloaded(m))
                .collect();

            // no saved tabs — create a fresh one
            if app.tabs.is_empty() {
                app.tabs.push(TabLoadState::Loaded(Box::new(TabState::new(
                    0,
                    app.active_environment_id,
                ))));
                app.next_tab_id = 1;
                app.active_tab = 0;
                return iced::Task::none();
            }

            // load active tab immediately
            let active = app.active_tab;
            if let Some(TabLoadState::Unloaded(meta)) = app.tabs.get(active) {
                let id = meta.id;
                app.tabs[active] = TabLoadState::Loading(meta.clone());
                return iced::Task::perform(SavedState::load(id), move |s| {
                    Message::TabBodyLoaded { id, saved: s }
                });
            }
            iced::Task::none()
        }
        Message::SaveComplete => {
            println!("SaveComplete received");
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

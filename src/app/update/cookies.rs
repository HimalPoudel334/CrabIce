//! `update` handlers for the **cookies** messages.

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::domain::cookies::CookieEntry;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::CookieJarOpen => {
            app.cookie_jar_open = true;
            iced::Task::none()
        }
        Message::CookieJarClose => {
            for cookies in app.cookie_jar.values_mut() {
                cookies.retain(|c| !c.name.trim().is_empty());
            }
            app.cookie_jar.retain(|_, cookies| !cookies.is_empty());
            app.cookie_jar_open = false;
            iced::Task::none()
        }
        Message::CookieJarAdd(domain) => {
            if domain.trim().is_empty() {
                app.cookie_jar_error = Some("Domain cannot be empty".to_string());
            } else {
                app.cookie_jar.entry(domain).or_default().push(CookieEntry {
                    name: String::new(),
                    value: String::new(),
                    enabled: true,
                    domain: String::new(),
                    path: "/".to_string(),
                    expires: None,
                });
                app.cookie_jar_new_domain = String::new();
                app.cookie_jar_error = None;
            }
            iced::Task::none()
        }
        Message::CookieJarRemove(domain, idx) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if idx < cookies.len() {
                    cookies.remove(idx);
                }
                if cookies.is_empty() {
                    app.cookie_jar.remove(&domain);
                }
            }
            iced::Task::none()
        }
        Message::CookieJarToggled(domain, idx) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if let Some(c) = cookies.get_mut(idx) {
                    c.enabled = !c.enabled;
                }
            }
            iced::Task::none()
        }
        Message::CookieJarNameChanged(domain, idx, val) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if let Some(c) = cookies.get_mut(idx) {
                    c.name = val;
                }
            }
            iced::Task::none()
        }
        Message::CookieJarValueChanged(domain, idx, val) => {
            if let Some(cookies) = app.cookie_jar.get_mut(&domain) {
                if let Some(c) = cookies.get_mut(idx) {
                    c.value = val;
                }
            }
            iced::Task::none()
        }
        Message::CookieJarDomainChanged(val) => {
            app.cookie_jar_new_domain = val;
            app.cookie_jar_error = None;
            iced::Task::none()
        }
        Message::CookieJarClearDomain(domain) => {
            app.cookie_jar.remove(&domain);
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

//! Iced subscriptions (websocket, spinner tick, window/keyboard events).

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::websocket::WebSocketRecipe;
use crate::domain::http::RequestType;

impl CrabiPie {
    pub(crate) fn subscription(&self) -> iced::Subscription<Message> {
        let mut subscriptions = vec![self.svg_rotation_subscription(), self.event_subscription()];

        if let Some(tab) = self.current_tab()
            && tab.request_type == RequestType::WebSocket
            && tab.ws_connection_id > 0
        {
            let url = tab.url.clone();
            subscriptions.push(Self::websocket_subscription(tab.ws_connection_id, url));
        }

        iced::Subscription::batch(subscriptions)
    }

    pub(crate) fn websocket_subscription(ws_connection_id: usize, url: String) -> iced::Subscription<Message> {
        iced::advanced::subscription::from_recipe(WebSocketRecipe {
            ws_connection_id,
            url,
        })
    }

    pub(crate) fn svg_rotation_subscription(&self) -> iced::Subscription<Message> {
        if let Some(tab) = self.current_tab()
            && tab.loading
        {
            iced::time::every(std::time::Duration::from_millis(5)).map(|_| Message::Tick)
        } else {
            iced::Subscription::none()
        }
    }

    pub(crate) fn event_subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen().map(Message::EventOccurred)
    }
}

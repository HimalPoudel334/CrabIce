//! Building the save-session task from the current app state.

use crate::app::CrabiPie;
use crate::app::message::Message;
use crate::app::tab::TabLoadState;
use crate::domain::persistence::{SavedState, SessionState, TabMetadata};

impl CrabiPie {
    pub(crate) fn save_task(&self) -> iced::Task<Message> {
        let json_theme_str = self.json_theme.to_string();
        let app_theme_str = self.app_theme.to_string();
        let session = SessionState {
            active_tab: self.active_tab,
            json_theme: json_theme_str.clone(),
            app_theme: app_theme_str.clone(),
            next_tab_id: self.next_tab_id,
            cookie_jar: self.cookie_jar.clone(),
            environments: self.environments.clone(),
            active_environment_id: self.active_environment_id,
            next_environment_id: self.next_environment_id,
        };

        let metadata: Vec<TabMetadata> = self.tabs.iter().map(|t| t.metadata().clone()).collect();

        let dirty_tabs: Vec<SavedState> = self
            .tabs
            .iter()
            .filter_map(|t| match t {
                TabLoadState::Loaded(s) if s.dirty => {
                    Some(s.to_saved(&json_theme_str, &app_theme_str))
                }
                _ => None,
            })
            .collect();
        println!("Saving {} dirty tabs", dirty_tabs.len());

        iced::Task::perform(
            async move {
                SessionState::save(&session).await;
                TabMetadata::save_all(&metadata).await;
                for saved in dirty_tabs {
                    saved.save().await;
                }
            },
            |_| Message::SaveComplete,
        )
    }
}

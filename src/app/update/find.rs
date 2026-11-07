//! `update` handlers for the **find** messages.

use crate::app::CrabiPie;
use crate::app::message::Message;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::ToggleFindDialog => {
            app.find_dialog_open = !app.find_dialog_open;
            iced::widget::operation::focus("find_input")
        }
        Message::ToggleFindReplaceDialog => {
            app.find_replace_mode = !app.find_replace_mode;
            iced::widget::operation::focus(if app.find_replace_mode {
                "replace_input"
            } else {
                "find_input"
            })
        }
        Message::CloseFindDialog => {
            app.find_dialog_open = false;

            // Clear search highlights
            app.search_match_positions = Vec::new();
            app.current_match_line_col = None;
            app.search_match_length = 0;
            app.current_match = 0;
            app.total_matches = 0;

            iced::Task::none()
        }
        Message::FindTextChanged(find_text) => {
            app.current_match = 0;
            app.total_matches = 0;
            app.current_match_pos = None;
            app.search_match_positions = Vec::new();
            app.current_match_line_col = None;
            app.search_match_length = 0;
            app.find_text = find_text;

            // Automatically find first match when text changes
            if !app.find_text.is_empty() {
                app.find_next();
            }

            iced::Task::none()
        }
        Message::ReplaceTextChanged(replace_text) => {
            app.replace_text = replace_text;
            iced::Task::none()
        }
        Message::ToggleCaseSensitive => {
            app.case_sensitive = !app.case_sensitive;

            // Re-search with new settings
            if !app.find_text.is_empty() {
                app.current_match = 0;
                app.find_next();
            }

            iced::Task::none()
        }
        Message::ToggleWholeWord => {
            app.whole_word = !app.whole_word;

            // Re-search with new settings
            if !app.find_text.is_empty() {
                app.current_match = 0;
                app.find_next();
            }

            iced::Task::none()
        }
        Message::FindNext => {
            app.find_next();

            iced::Task::none()
        }
        Message::FindPrevious => iced::Task::none(),
        Message::Replace => {
            println!("Event fired");
            iced::Task::none()
        }
        Message::ReplaceAll => {
            println!("Event fired");
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

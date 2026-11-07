//! PDF page rendering/prefetch helpers.

use crate::app::CrabiPie;
use crate::app::message::Message;

impl CrabiPie {
    pub(crate) fn render_and_prefetch(&mut self) -> iced::Task<Message> {
        let Some(tab) = self.current_tab_mut() else {
            return iced::Task::none();
        };
        let Some(pdf) = &mut tab.pdf_viewer else {
            return iced::Task::none();
        };

        let current = pdf.current_page;
        tab.pdf_current_image = pdf.render_current_page().ok();

        for neighbor in [current.checked_sub(1), Some(current + 1)]
            .into_iter()
            .flatten()
        {
            if neighbor < pdf.page_count() && !pdf.is_cached(neighbor) {
                let _ = pdf.render_page(neighbor);
            }
        }

        iced::Task::none()
    }
}

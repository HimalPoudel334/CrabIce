//! `update` handlers for the **media** messages.

use crate::app::CrabiPie;
use crate::app::message::Message;

pub(crate) fn handle(app: &mut CrabiPie, message: Message) -> iced::Task<Message> {
    match message {
        Message::PdfNextPage => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(pdf) = &mut tab.pdf_viewer {
                    pdf.next_page();
                }
            }
            app.render_and_prefetch()
        }
        Message::PdfPreviousPage => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(pdf) = &mut tab.pdf_viewer {
                    pdf.previous_page();
                    tab.pdf_current_image = pdf.render_current_page().ok();
                }
            }
            iced::Task::none()
        }
        Message::TogglePause => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_paused(!vp.paused());
                }
            }
            iced::Task::none()
        }
        Message::ToggleLoop => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_looping(!vp.looping());
                }
            }
            iced::Task::none()
        }
        Message::VideoVolume(vol) => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_volume(vol);
                }
            }
            iced::Task::none()
        }
        Message::Seek(secs) => {
            if let Some(tab) = app.current_tab_mut() {
                if let Some(vs) = tab.video_state.as_mut() {
                    vs.dragging = true;
                    vs.position = secs;
                }
                if let Some(vp) = tab.video_player.as_mut() {
                    vp.set_paused(true);
                }
            }
            iced::Task::none()
        }
        Message::SeekRelease => {
            if let Some(tab) = app.current_tab_mut() {
                if let (Some(vs), Some(vp)) = (tab.video_state.as_mut(), tab.video_player.as_mut())
                {
                    vs.dragging = false;
                    vp.seek(std::time::Duration::from_secs_f64(vs.position), false)
                        .expect("seek");
                    vp.set_paused(false);
                }
            }
            iced::Task::none()
        }
        Message::EndOfStream => {
            println!("end of stream");
            iced::Task::none()
        }
        Message::NewFrame => {
            let Some(tab) = app.current_tab_mut() else {
                return iced::Task::none();
            };
            if let (Some(vs), Some(vp)) = (tab.video_state.as_mut(), tab.video_player.as_mut()) {
                if !vs.dragging {
                    vs.position = vp.position().as_secs_f64();
                }
            }
            iced::Task::none()
        }
        other => unreachable!("message routed to the wrong handler: {other:?}"),
    }
}

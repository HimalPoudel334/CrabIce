#![allow(unused)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod domain;
mod ui;

use app::{CrabiPie, update::update};
use ui::view::view;

fn main() -> iced::Result {
    let icon_bytes = include_bytes!("../CrabiPie.ico");
    iced::application(CrabiPie::new, update, view)
        .theme(|app: &CrabiPie| app.app_theme.clone())
        .subscription(|app| app.subscription())
        .title(|app: &CrabiPie| app.title())
        .window(iced::window::Settings {
            size: iced::Size::new(1500.0, 800.0),
            icon: iced::window::icon::from_file_data(icon_bytes, None).ok(),
            ..Default::default()
        })
        .exit_on_close_request(false)
        .run()
}

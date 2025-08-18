mod analog_clock;
mod analog_clock_relm4;
mod app;
mod commands;
mod settings;
mod simple_messages;
mod notifications;
mod services;
mod style;
mod tiles;
mod utils;
mod widgets;
mod wifi_menu;

pub mod tests;

use relm4::RelmApp;

use crate::app::MuseShellModel;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();

    // Initialize configuration system
    if let Err(e) = settings::init() {
        log::error!("failed to initialize settings: {}", e);
    }

    RelmApp::new("com.musicaloft.muse-shell").run::<MuseShellModel>(());
    gtk4::glib::ExitCode::FAILURE
}

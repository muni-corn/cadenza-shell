mod analog_clock;
mod app;
mod commands;
mod notifications;
mod services;
mod settings;
mod style;
mod tiles;
mod utils;
mod widgets;
mod wifi_menu;

mod icon_names {
    pub use shipped::*; // Include all shipped icons by default
    include!(concat!(env!("OUT_DIR"), "/icon_names.rs"));
}

pub mod tests;

use relm4::RelmApp;

use crate::app::MuseShellModel;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();

    relm4_icons::initialize_icons(icon_names::GRESOURCE_BYTES, icon_names::RESOURCE_PREFIX);

    // Initialize configuration system
    if let Err(e) = settings::init() {
        log::error!("failed to initialize settings: {}", e);
    }

    RelmApp::new("com.musicaloft.muse-shell").run::<MuseShellModel>(());
    gtk4::glib::ExitCode::FAILURE
}

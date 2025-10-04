#![feature(trait_alias)]

mod analog_clock;
mod app;
mod commands;
mod network;
mod notifications;
mod services;
mod settings;
mod style;
mod tiles;
mod tray;
mod utils;
mod weather;
mod widgets;
mod wifi_menu;

mod icon_names {
    pub use shipped::*; // Include all shipped icons by default
    include!(concat!(env!("OUT_DIR"), "/icon_names.rs"));
}

use relm4::{RELM_THREADS, RelmApp};

use crate::{app::CadenzaShellModel, style::compile_styles};

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();

    RELM_THREADS.set(16).unwrap();

    relm4_icons::initialize_icons(icon_names::GRESOURCE_BYTES, icon_names::RESOURCE_PREFIX);

    // Initialize configuration system
    if let Err(e) = settings::init() {
        log::error!("failed to initialize settings: {}", e);
    }

    match compile_styles() {
        Ok(css) => relm4::set_global_css(&css),
        Err(e) => log::error!("couldn't load scss: {e}"),
    }

    RelmApp::new("com.musicaloft.cadenza-shell")
        .visible_on_activate(cfg!(debug_assertions))
        .run::<CadenzaShellModel>(());

    gtk4::glib::ExitCode::FAILURE
}

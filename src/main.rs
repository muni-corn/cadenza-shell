#![feature(result_option_map_or_default)]
#![feature(never_type)]

mod analog_clock;
mod app;
mod battery;
mod bluetooth;
mod bluetooth_menu;
mod brightness;
mod commands;
mod mpris;
mod network;
mod network_menu;
mod niri;
mod notifications;
mod pulseaudio;
mod settings;
mod sleep_monitor;
mod sound;
mod style;
mod tiles;
mod utils;
mod weather;
mod widgets;

mod icon_names {
    pub use shipped::*; // Include all shipped icons by default
    include!(concat!(env!("OUT_DIR"), "/icon_names.rs"));
}

use relm4::{RELM_THREADS, RelmApp};
use tracing_subscriber::{EnvFilter, fmt};

use crate::{app::CadenzaShellModel, style::compile_styles};

#[tokio::main]
async fn main() -> glib::ExitCode {
    // default to info; bump notifications to debug for richer diagnostics
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "info,cadenza_shell::notifications=debug,cadenza_shell::tiles::notifications=debug",
        )
    });

    fmt().with_env_filter(filter).with_target(true).init();

    RELM_THREADS.set(16).unwrap();

    relm4_icons::initialize_icons(icon_names::GRESOURCE_BYTES, icon_names::RESOURCE_PREFIX);

    // initialize configuration system
    if let Err(e) = settings::init() {
        tracing::error!("failed to initialize settings: {}", e);
    }

    match compile_styles() {
        Ok(css) => relm4::set_global_css(&css),
        Err(e) => tracing::error!("couldn't load scss: {e}"),
    }

    RelmApp::new("com.musicaloft.cadenza-shell")
        .visible_on_activate(false)
        .run_async::<CadenzaShellModel>(());

    gtk4::glib::ExitCode::FAILURE
}

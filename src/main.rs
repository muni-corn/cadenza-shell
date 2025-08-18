mod analog_clock;
mod app;
mod commands;
mod simple_messages;
mod notifications;
mod services;
mod settings;
mod style;
mod tiles;
mod utils;
mod widgets;
mod wifi_menu;

use app::run;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();
    
    log::info!("Starting Muse Shell with Relm4");
    run()
}

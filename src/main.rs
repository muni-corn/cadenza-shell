mod analog_clock;
mod app;
mod notifications;
mod services;
mod settings;
mod style;
mod tiles;
mod utils;
mod widgets;

use app::MuseShell;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();
    MuseShell::new().run()
}
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

fn main() -> glib::ExitCode {
    env_logger::init();

    let app = MuseShell::new();
    app.run()
}
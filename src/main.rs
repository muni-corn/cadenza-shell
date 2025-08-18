mod analog_clock;
mod app;
mod app_relm4;
mod simple_app;
mod messages;
mod simple_messages;
mod notifications;
mod services;
mod settings;
mod style;
mod tiles;
mod utils;
mod widgets;

// Old GTK implementation
use app::MuseShell;
// New Relm4 implementation
use app_relm4::MuseShellRelm4;
use simple_app::run_simple_app;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();
    
    // Use feature flag or environment variable to choose implementation
    if std::env::var("MUSE_SHELL_SIMPLE").is_ok() {
        log::info!("Starting Muse Shell with Simple Relm4");
        run_simple_app()
    } else if std::env::var("MUSE_SHELL_RELM4").is_ok() {
        log::info!("Starting Muse Shell with Full Relm4");
        MuseShellRelm4::run()
    } else {
        log::info!("Starting Muse Shell with traditional GTK");
        MuseShell::new().run()
    }
}

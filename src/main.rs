mod analog_clock;
mod app;
mod simple_app;
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
// Simple Relm4 implementation
use simple_app::run_simple_app;

// Commented out problematic modules for now
// mod app_relm4;
// mod messages;
// use app_relm4::MuseShellRelm4;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();
    
    // Use feature flag or environment variable to choose implementation
    if std::env::var("MUSE_SHELL_GTK").is_ok() {
        log::info!("Starting Muse Shell with traditional GTK");
        MuseShell::new().run()
    } else {
        log::info!("Starting Muse Shell with Simple Relm4 (default)");
        run_simple_app()
    }
}

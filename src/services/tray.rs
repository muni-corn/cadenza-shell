use std::sync::Arc;

use anyhow::Result;
use relm4::{SharedState, Worker};
use tokio::sync::RwLock;
use zbus::Connection;

use crate::tray::{TrayState, status_notifier::watcher::StatusNotifierWatcher};

pub static TRAY_STATE: SharedState<TrayState> = SharedState::<TrayState>::new();

pub struct TrayService {
    connection: Arc<RwLock<Option<Connection>>>,
}

impl Worker for TrayService {
    type Init = ();
    type Input = ();
    type Output = ();

    fn init(_init: Self::Init, _sender: relm4::ComponentSender<Self>) -> Self {
        // Initialize with empty state
        *TRAY_STATE.write() = TrayState {
            items: Vec::new(),
            expanded: false,
        };

        let service = Self {
            connection: Arc::new(RwLock::new(None)),
        };

        // Spawn the tray watcher task
        let connection_clone = service.connection.clone();
        relm4::spawn(async move {
            match setup_tray_watcher(connection_clone).await {
                Ok(_) => log::info!("tray service started successfully"),
                Err(e) => log::error!("failed to start tray service: {}", e),
            }
        });

        service
    }

    fn update(&mut self, _message: Self::Input, _sender: relm4::ComponentSender<Self>) {}
}

async fn setup_tray_watcher(connection: Arc<RwLock<Option<Connection>>>) -> Result<()> {
    // Connect to the session bus
    let conn = Connection::session().await?;

    // Create our watcher
    let watcher = StatusNotifierWatcher::new();

    // Register our watcher on the bus
    conn.object_server()
        .at("/StatusNotifierWatcher", watcher)
        .await?;

    // Request the name for our watcher
    conn.request_name("org.kde.StatusNotifierWatcher").await?;

    log::info!("StatusNotifierWatcher registered on D-Bus");

    // Store the connection
    *connection.write().await = Some(conn.clone());

    Ok(())
}

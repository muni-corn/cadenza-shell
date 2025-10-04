use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use zbus::{Connection, fdo, interface};

use crate::{services::tray::TRAY_STATE, tray::TrayItem};

#[derive(Debug)]
pub struct StatusNotifierWatcher {
    registered_items: Arc<RwLock<HashMap<String, TrayItem>>>,
    connection: Arc<RwLock<Option<Connection>>>,
}

impl StatusNotifierWatcher {
    pub fn new() -> Self {
        Self {
            registered_items: Arc::new(RwLock::new(HashMap::new())),
            connection: Arc::new(RwLock::new(None)),
        }
    }

    async fn update_tray_state(&self) {
        let items = self.registered_items.read().await;
        let tray_items: Vec<TrayItem> = items.values().cloned().collect();

        let mut state = TRAY_STATE.write();
        state.items = tray_items;

        log::debug!("updated tray state with {} items", state.items.len());
    }
}

#[interface(name = "org.freedesktop.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    async fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    async fn protocol_version(&self) -> i32 {
        0
    }

    async fn registered_status_notifier_items(&self) -> Vec<String> {
        let items = self.registered_items.read().await;
        items.keys().cloned().collect()
    }

    async fn register_status_notifier_item(&self, service: &str) -> fdo::Result<()> {
        log::info!("registering status notifier item: {}", service);

        // parse the service name - it can be either a bus name or a path
        let (service_name, object_path) = if service.starts_with('/') {
            // if it starts with '/', it's an object path on our connection.
            // for now, we'll use a placeholder - in reality we'd need to get the sender's
            // name
            ("unknown".to_string(), service.to_string())
        } else {
            // otherwise, it's a bus name, use standard object path
            (service.to_string(), "/StatusNotifierItem".to_string())
        };

        // TODO

        // Update the global state
        self.update_tray_state().await;

        log::info!("tray item registered: {}", service);
        Ok(())
    }

    async fn register_status_notifier_host(&self, _service: &str) -> fdo::Result<()> {
        // We are the host, so we don't need to do anything here
        Ok(())
    }
}

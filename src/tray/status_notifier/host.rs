use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use zbus::{Connection, fdo, interface};

use crate::tray::{TrayItem, status_notifier::item::StatusNotifierItemProxy};

/// StatusNotifierHost implementation according to freedesktop.org specification
/// A host is the system tray that displays StatusNotifierItem items
#[derive(Debug)]
pub struct StatusNotifierHost {
    /// Registered items that this host is displaying
    items: Arc<RwLock<HashMap<String, StatusNotifierItemProxy<'static>>>>,
    /// The service name of this host
    service_name: String,
    /// The object path where this host is registered
    object_path: String,
}

impl StatusNotifierHost {
    pub fn new(service_name: String, object_path: String) -> Self {
        Self {
            items: Arc::new(RwLock::new(HashMap::new())),
            service_name,
            object_path,
        }
    }

    /// Register a new StatusNotifierItem with this host
    pub async fn register_item(&self, item_service: &str, item_path: &str) -> fdo::Result<()> {
        log::info!(
            "registering StatusNotifierItem: {} at path: {}",
            item_service,
            item_path
        );

        // Create a proxy to the item to query its properties
        let connection = Connection::session()
            .await
            .map_err(|e| fdo::Error::Failed(format!("failed to connect to session bus: {}", e)))?;

        // TODO: Store the item
        // {
        //     let mut items = self.items.write().await;
        //     items.insert(item_service.to_string(), tray_item.clone());
        // }

        log::info!(
            "successfully registered StatusNotifierItem: {}",
            item_service
        );
        Ok(())
    }

    /// Unregister a StatusNotifierItem from this host
    pub async fn unregister_item(&self, item_service: &str) -> fdo::Result<()> {
        log::info!("unregistering StatusNotifierItem: {}", item_service);

        let mut items = self.items.write().await;
        items.remove(item_service);

        log::info!(
            "successfully unregistered StatusNotifierItem: {}",
            item_service
        );
        Ok(())
    }

    /// Get all registered items
    pub async fn get_items(&self) -> Vec<TrayItem> {
        // TODO
        Vec::new()
    }
}

#[interface(name = "org.kde.StatusNotifierHost")]
impl StatusNotifierHost {
    /// Called by StatusNotifierWatcher when a new item is registered
    async fn item_registered(&self, service: &str) -> fdo::Result<()> {
        log::info!("StatusNotifierHost: Item registered: {}", service);

        // Parse the service name - it can be either a bus name or a path
        let (service_name, object_path) = if service.starts_with('/') {
            // if it starts with '/', it's an object path on our connection
            ("unknown".to_string(), service.to_string())
        } else {
            // otherwise, it's a bus name, use standard object path
            (service.to_string(), "/StatusNotifierItem".to_string())
        };

        self.register_item(&service_name, &object_path).await
    }

    /// Called by StatusNotifierWatcher when an item is unregistered
    async fn item_unregistered(&self, service: &str) -> fdo::Result<()> {
        log::info!("StatusNotifierHost: Item unregistered: {}", service);
        self.unregister_item(service).await
    }

    /// Get information about this host
    async fn host_info(&self) -> String {
        format!(
            "cadenza-shell StatusNotifierHost at {}:{}",
            self.service_name, self.object_path
        )
    }

    /// Get the version of the StatusNotifier protocol this host supports
    async fn protocol_version(&self) -> i32 {
        0 // Version 0 is the current version
    }

    /// Check if this host is still active
    async fn is_host_registered(&self) -> bool {
        true
    }
}

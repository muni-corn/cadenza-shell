pub mod card;
pub mod center;
pub mod daemon;
pub mod fresh;
pub mod types;

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use relm4::{ComponentSender, Worker};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use zbus::{
    Connection,
    object_server::InterfaceRef,
    zvariant::{
        OwnedValue, Type,
        as_value::{self, optional},
    },
};

use crate::notifications::{
    daemon::{NotificationsDaemon, NotificationsDaemonSignals},
    types::{Notification, NotificationUrgency},
};

#[derive(Debug, Clone)]
pub enum NotificationServiceMsg {
    GetNotifications,
    ClearAll,
    CloseNotification(u32),
    StoreNotification(Notification),
    ActionInvoked(u32, String),
}

#[derive(Debug, Clone)]
pub enum NotificationWorkerOutput {
    NotificationReceived(Notification),
    NotificationClosed { id: u32, reason: u32 },
    ActionInvoked { id: u32, action_key: String },
    Notifications(HashMap<u32, Notification>),
    AllCleared,
    Error(String),
}

#[derive(Deserialize, Serialize, Type, Default)]
#[zvariant(signature = "dict")]
#[serde(default, rename_all = "kebab-case")]
pub struct NotificationHints {
    #[serde(with = "as_value")]
    action_icons: bool,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    category: Option<String>,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    pub desktop_entry: Option<String>,

    #[serde(with = "as_value")]
    resident: bool,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    sound_file: Option<String>,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    sound_name: Option<String>,

    #[serde(with = "as_value")]
    suppress_sound: bool,

    #[serde(with = "as_value")]
    transient: bool,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    pub urgency: Option<NotificationUrgency>,

    #[serde(flatten)]
    others: HashMap<String, OwnedValue>,
}

pub struct NotificationService {
    connection: Arc<RwLock<Option<Connection>>>,
    interface: Arc<RwLock<Option<InterfaceRef<NotificationsDaemon>>>>,
    notifications: HashMap<u32, Notification>,
}

impl std::fmt::Debug for NotificationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationService")
            .field("connection", &self.connection)
            .field("notifications", &self.notifications)
            .finish()
    }
}

impl Worker for NotificationService {
    type Init = ();
    type Input = NotificationServiceMsg;
    type Output = NotificationWorkerOutput;

    fn init(_init: Self::Init, sender: ComponentSender<Self>) -> Self {
        let sender_clone = sender.clone();

        let connection = Arc::new(RwLock::new(None));
        let connection_clone = Arc::clone(&connection);

        let interface = Arc::new(RwLock::new(None));
        let interface_clone = Arc::clone(&interface);

        relm4::spawn(async move {
            match initialize_notifications_daemon(sender_clone.clone()).await {
                Ok(connection) => {
                    log::info!("notifications daemon initialized successfully");
                    *interface_clone.write().await = Some(
                        connection
                            .object_server()
                            .interface::<_, NotificationsDaemon>("/org/freedesktop/Notifications")
                            .await
                            .unwrap(),
                    );
                    *connection_clone.write().await = Some(connection);
                }
                Err(e) => {
                    log::error!("failed to initialize notifications daemon: {}", e);
                    sender_clone
                        .output(NotificationWorkerOutput::Error(e.to_string()))
                        .unwrap_or_else(|_| log::error!("failed to send error output"));
                }
            }
        });

        Self {
            connection,
            interface,
            notifications: HashMap::new(),
        }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            NotificationServiceMsg::GetNotifications => {
                sender
                    .output(NotificationWorkerOutput::Notifications(
                        self.notifications.clone(),
                    ))
                    .unwrap_or_else(|_| log::error!("failed to send output"));
            }
            NotificationServiceMsg::ClearAll => {
                self.notifications.clear();
                sender
                    .output(NotificationWorkerOutput::AllCleared)
                    .unwrap_or_else(|_| log::error!("failed to send output"));
            }
            NotificationServiceMsg::CloseNotification(id) => {
                self.notifications.remove(&id);
            }
            NotificationServiceMsg::StoreNotification(notification) => {
                self.notifications.insert(notification.id, notification);
            }
            NotificationServiceMsg::ActionInvoked(id, action) => {
                let interface_clone = Arc::clone(&self.interface);
                relm4::spawn(async move {
                    interface_clone
                        .read()
                        .await
                        .as_ref()
                        .unwrap()
                        .action_invoked(id, action)
                        .await
                        .unwrap_or_else(|e| {
                            log::error!("couldn't send action_invoked signal: {}", e)
                        });
                });
            }
        }
    }
}

async fn initialize_notifications_daemon(
    sender: ComponentSender<NotificationService>,
) -> Result<Connection> {
    Ok(zbus::connection::Builder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at(
            "/org/freedesktop/Notifications",
            NotificationsDaemon::new(sender),
        )?
        .build()
        .await?)
}

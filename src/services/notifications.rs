use anyhow::Result;
use gtk4::glib;
use gtk4::subclass::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use zbus::{
    Connection, interface,
    object_server::SignalEmitter,
    zvariant::{
        OwnedValue, Type,
        as_value::{self, optional},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub desktop_entry: String,
    pub image: String,
    pub summary: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    pub timeout: i32,
    pub timestamp: i64,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum NotificationServiceMsg {
    GetNotifications,
    ClearAll,
    CloseNotification(u32),
    StoreNotification(Notification),
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
    desktop_entry: Option<String>,

    #[serde(with = "as_value")]
    resident: bool,

mod imp {
    use super::{Notification, NotificationsDaemonProxy};
    use anyhow::Result;
    use futures_lite::StreamExt;
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::RefCell;
    use zbus::Connection;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::NotificationService)]
    pub struct NotificationService {
        #[property(get, set)]
        notification_count: std::cell::Cell<u32>,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    urgency: Option<NotificationUrgency>,

    #[serde(flatten)]
    others: HashMap<String, OwnedValue>,
}

    #[glib::object_subclass]
    impl ObjectSubclass for NotificationService {
        const NAME: &'static str = "MuseShellNotificationService";
        type Type = super::NotificationService;
        type ParentType = glib::Object;
    }

static NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

/// Implements https://specifications.freedesktop.org/notification-spec/latest/protocol.html.
#[derive(Debug)]
pub struct NotificationsDaemon {
    notifications: Mutex<HashMap<u32, Notification>>,
    sender: ComponentSender<NotificationService>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationsDaemon {
    // commands

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: NotificationHints,
        expire_timeout: i32,
        #[zbus(signal_emitter)] _emitter: SignalEmitter<'_>,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst)
        };

        let urgency = hints.urgency.unwrap_or(NotificationUrgency::Normal);

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let notification = Notification {
            id,
            app_name: app_name.clone(),
            app_icon: app_icon.clone(),
            desktop_entry: hints.desktop_entry.unwrap_or_default(),
            image: String::new(), // TODO: Extract from hints if available
            summary: summary.clone(),
            body: body.clone(),
            urgency,
            timeout: expire_timeout,
            timestamp,
            actions: actions.clone(),
        };

        dbg!("new notification received: {}", &notification);

        // store notification in worker
        self.sender
            .input(NotificationServiceMsg::StoreNotification(notification.clone()));

        // notify worker about new notification
        if let Err(e) = self
            .sender
            .output(NotificationWorkerOutput::NotificationReceived(
                notification.clone(),
            ))
        {
            log::error!("failed to send notification to worker: {:?}", e);
        }

        // store notification
        {
            let mut notifications = self.notifications.lock().await;

            notifications.insert(id, notification.clone());
        }

        // handle timeout (note: we can't emit signals after the method ends, so we just
        // track it internally)
        if expire_timeout > 0 {
            let sender_clone = self.sender.clone();
            relm4::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(expire_timeout as u64)).await;
                if let Err(e) = sender_clone
                    .output(NotificationWorkerOutput::NotificationClosed { id, reason: 1 })
                {
                    log::error!("failed to send notification closed to worker: {:?}", e);
                }
            });
        }

        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        // remove from storage
        {
            let mut notifications = self.notifications.lock().await;
            notifications.remove(&id);
        }

        // emit signal
        if let Err(e) = emitter.notification_closed(id, 2).await {
            log::error!("failed to emit notification_closed signal: {}", e);
        }

        // send to worker
        if let Err(e) = self
            .sender
            .output(NotificationWorkerOutput::NotificationClosed { id, reason: 2 })
        {
            log::error!("failed to send notification closed to worker: {:?}", e);
        }
    }

    async fn get_capabilities(&self) -> Vec<String> {
        vec![
            "action-icons".to_string(),
            "actions".to_string(),
            "body".to_string(),
            "body-hyperlinks".to_string(),
            "body-images".to_string(),
            "body-markup".to_string(),
            "icon-static".to_string(),
            "persistence".to_string(),
            "sound".to_string(),
        ]
    }

    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            "muse-shell".to_string(),
            "municorn".to_string(),
            "1.0.0".to_string(),
            "1.3".to_string(),
        )
    }

    // signals

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: String,
    ) -> zbus::Result<()>;
}

impl NotificationsDaemon {
    fn new(sender: ComponentSender<NotificationService>) -> Self {
        Self {
            notifications: Mutex::new(HashMap::new()),
            sender,
        }
    }

    pub async fn get_notifications(&self) -> HashMap<u32, Notification> {
        self.notifications.lock().await.clone()
    }

    pub async fn clear_all(&self) {
        let mut notifications = self.notifications.lock().await;
        notifications.clear();
    }
}

#[derive(Debug)]
pub struct NotificationService {
    connection: Arc<RwLock<Option<Connection>>>,
    notifications: HashMap<u32, Notification>,
}

impl Worker for NotificationService {
    type Init = ();
    type Input = NotificationServiceMsg;
    type Output = NotificationWorkerOutput;

    fn init(_init: Self::Init, sender: ComponentSender<Self>) -> Self {
        let sender_clone = sender.clone();

        let connection = Arc::new(RwLock::new(None));
        let connection_clone = Arc::clone(&connection);

        relm4::spawn(async move {
            match initialize_notifications_daemon(sender_clone.clone()).await {
                Ok(connection) => {
                    log::info!("notifications daemon initialized successfully");
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

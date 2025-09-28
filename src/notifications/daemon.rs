use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
    time::SystemTime,
};

use relm4::ComponentSender;
use tokio::sync::Mutex;
use zbus::{interface, object_server::SignalEmitter};

use crate::{
    notifications::types::{Notification, NotificationUrgency},
    services::notifications::{
        NotificationHints, NotificationService, NotificationServiceMsg, NotificationWorkerOutput,
    },
};

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

        let actions = {
            let evens = actions.iter().step_by(2).cloned();
            let odds = actions.iter().skip(1).step_by(2).cloned();
            evens.zip(odds).collect()
        };

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
            actions,
        };

        dbg!("new notification received: {}", &notification);

        // store notification in worker
        self.sender.input(NotificationServiceMsg::StoreNotification(
            notification.clone(),
        ));

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
            "cadenza-shell".to_string(),
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
    pub fn new(sender: ComponentSender<NotificationService>) -> Self {
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

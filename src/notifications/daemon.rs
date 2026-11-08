use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use zbus::{interface, object_server::SignalEmitter, zvariant::OwnedValue};

use crate::notifications::{
    NOTIFICATIONS_STATE, NotificationEvent, event_tx,
    hints::Hints,
    lifecycle::{self, CloseReason},
    types::{Notification, NotificationUrgency},
};

static NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

/// Implements https://specifications.freedesktop.org/notification-spec/latest/protocol.html.
#[derive(Debug)]
pub struct NotificationsDaemon;

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
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            // a client may pick an id we haven't assigned yet; keep the
            // counter past it so it's never handed out to someone else
            NOTIFICATION_ID.fetch_max(replaces_id.saturating_add(1), Ordering::SeqCst);
            replaces_id
        } else {
            NOTIFICATION_ID.fetch_add(1, Ordering::SeqCst)
        };

        let is_replacement = replaces_id != 0;
        let hints = Hints::from(hints);
        let urgency = hints.urgency();

        let timestamp = chrono::Local::now().timestamp();

        let actions = {
            let evens = actions.iter().step_by(2).cloned();
            let odds = actions.iter().skip(1).step_by(2).cloned();
            evens.zip(odds).collect::<Vec<_>>()
        };

        tracing::info!(
            notification.id = id,
            notification.app = %app_name,
            notification.summary = %summary,
            ?urgency,
            expire_timeout,
            action_count = actions.len(),
            is_replacement,
            "notify accepted"
        );

        let notification = Notification {
            id,
            app_name,
            app_icon,
            desktop_entry: hints.string("desktop-entry").unwrap_or_default(),
            image: String::new(), // TODO: Extract from hints if available
            summary,
            body,
            urgency,
            timeout: expire_timeout,
            timestamp,
            actions,
        };

        // write to the global state
        NOTIFICATIONS_STATE
            .write()
            .notifications
            .insert(id, notification.clone());

        // broadcast the event to all subscribers
        let _ = event_tx().send(NotificationEvent::Received(notification));

        // handle timeout — auto-close non-persistent notifications
        // a replaced notification must not be closed by its predecessor's
        // timer, and the spec forbids expiring critical notifications
        lifecycle::cancel_expiry(id);
        if expire_timeout > 0 && urgency != NotificationUrgency::Critical {
            tracing::debug!(notification.id = id, expire_timeout, "scheduling expiry");
            lifecycle::schedule_expiry(id, Duration::from_millis(expire_timeout as u64));
        }

        id
    }

    /// Closes a notification on a client's request; the spec requires an
    /// error reply for ids that don't exist.
    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        tracing::debug!(
            notification.id = id,
            "close_notification invoked over D-Bus"
        );

        if lifecycle::close(id, CloseReason::Closed).await {
            Ok(())
        } else {
            Err(zbus::fdo::Error::Failed(format!(
                "there's no notification with id {id}"
            )))
        }
    }

    async fn get_capabilities(&self) -> Vec<String> {
        tracing::trace!("get_capabilities called");
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
        tracing::trace!("get_server_information called");
        (
            "cadenza-shell".to_string(),
            "municorn".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.3".to_string(),
        )
    }

    // signals

    #[zbus(signal)]
    pub async fn notification_closed(
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

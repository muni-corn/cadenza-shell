// temporary until we figure things out
#![allow(dead_code)]

pub mod card;
pub mod center;
pub mod daemon;
pub mod fresh;
pub mod types;

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use anyhow::Result;
use relm4::{ComponentSender, SharedState, Worker};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast, mpsc};
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

/// Global snapshot of all current notifications.
///
/// Consumers subscribe via `NOTIFICATIONS_STATE.subscribe(sender, mapper)` for
/// reactive updates, or read the current value with
/// `NOTIFICATIONS_STATE.read()`.
pub static NOTIFICATIONS_STATE: SharedState<NotificationsState> = SharedState::new();

/// Snapshot of the current notification state.
#[derive(Debug, Clone, Default)]
pub struct NotificationsState {
    pub notifications: HashMap<u32, Notification>,
}

/// A discrete notification event broadcast to all subscribers.
///
/// Use [`subscribe_events`] to obtain a receiver for this stream.
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    Received(Notification),
    Closed { id: u32, reason: u32 },
    ActionInvoked { id: u32, action_key: String },
    AllCleared,
}

// capacity of 64 events; lagging receivers miss old events but never block
// the producer, matching the pattern used in sleep_monitor
static EVENT_TX: OnceLock<broadcast::Sender<NotificationEvent>> = OnceLock::new();

pub(crate) fn event_tx() -> &'static broadcast::Sender<NotificationEvent> {
    EVENT_TX.get_or_init(|| broadcast::channel(64).0)
}

/// Subscribe to notification events.
///
/// Returns a receiver that yields a [`NotificationEvent`] for each change.
/// Multiple consumers can call this independently to each get their own
/// receiver.
pub fn subscribe_events() -> broadcast::Receiver<NotificationEvent> {
    event_tx().subscribe()
}

/// Commands that consumers can send to the notification service.
pub(crate) enum NotificationCommand {
    Dismiss(u32),
    ClearAll,
    InvokeAction { id: u32, action_key: String },
}

static COMMAND_TX: OnceLock<mpsc::UnboundedSender<NotificationCommand>> = OnceLock::new();

fn command_tx() -> &'static mpsc::UnboundedSender<NotificationCommand> {
    COMMAND_TX
        .get()
        .expect("notification service not yet started")
}

/// Dismiss a notification by ID.
///
/// Removes the notification from state and emits a `NotificationClosed` event.
/// Has no effect if the service has not been started.
pub fn dismiss(id: u32) {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::Dismiss(id));
    }
}

/// Clear all notifications.
///
/// Removes all notifications from state and emits an `AllCleared` event.
/// Has no effect if the service has not been started.
pub fn clear_all() {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::ClearAll);
    }
}

/// Invoke a notification action, emitting the D-Bus `ActionInvoked` signal.
///
/// Has no effect if the service has not been started.
pub fn invoke_action(id: u32, action_key: String) {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::InvokeAction { id, action_key });
    }
}

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
            match initialize_notifications_daemon().await {
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

async fn initialize_notifications_daemon() -> Result<Connection> {
    Ok(zbus::connection::Builder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at(
            "/org/freedesktop/Notifications",
            NotificationsDaemon::new(event_tx().clone()),
        )?
        .build()
        .await?)
}

/// Runs the notification service.
///
/// Registers `org.freedesktop.Notifications` on the session D-Bus, then drives
/// a command loop that handles [`dismiss`], [`clear_all`], and
/// [`invoke_action`] calls from UI components. Writes all state changes to
/// [`NOTIFICATIONS_STATE`] and broadcasts [`NotificationEvent`]s to every
/// subscriber obtained via [`subscribe_events`].
///
/// Must be started exactly once, from `app.rs`, before any UI component
/// subscribes to the state or issues commands.
pub async fn run_notifications_service() {
    // initialize the broadcast sender so subscribers can call subscribe_events()
    // before the first event arrives
    let _ = event_tx();

    let connection = match initialize_notifications_daemon().await {
        Ok(c) => {
            log::info!("notifications service started");
            c
        }
        Err(e) => {
            log::error!("failed to start notifications service: {}", e);
            return;
        }
    };

    // look up the interface ref so we can emit D-Bus signals for commands
    let interface_ref = match connection
        .object_server()
        .interface::<_, NotificationsDaemon>("/org/freedesktop/Notifications")
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::error!("couldn't look up notifications daemon interface: {}", e);
            return;
        }
    };

    // install the command sender so free functions can push commands
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<NotificationCommand>();
    if COMMAND_TX.set(cmd_tx).is_err() {
        log::warn!("notifications service started more than once; extra instance exiting");
        return;
    }

    // drive the command loop
    loop {
        let Some(cmd) = cmd_rx.recv().await else {
            log::warn!("notification command channel closed; service stopping");
            break;
        };

        match cmd {
            NotificationCommand::Dismiss(id) => {
                NOTIFICATIONS_STATE.write().notifications.remove(&id);
                let _ = event_tx().send(NotificationEvent::Closed { id, reason: 2 });

                // also emit the D-Bus signal so external clients are notified
                if let Err(e) = interface_ref.notification_closed(id, 2).await {
                    log::error!("couldn't emit notification_closed signal: {}", e);
                }
            }
            NotificationCommand::ClearAll => {
                NOTIFICATIONS_STATE.write().notifications.clear();
                let _ = event_tx().send(NotificationEvent::AllCleared);
            }
            NotificationCommand::InvokeAction { id, action_key } => {
                let _ = event_tx().send(NotificationEvent::ActionInvoked {
                    id,
                    action_key: action_key.clone(),
                });

                if let Err(e) = interface_ref.action_invoked(id, action_key).await {
                    log::error!("couldn't emit action_invoked signal: {}", e);
                }
            }
        }
    }
}

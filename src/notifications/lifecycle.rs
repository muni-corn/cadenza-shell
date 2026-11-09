//! Closing and expiring notifications.
//!
//! Every way a notification can go away (expiry, the user dismissing it,
//! `CloseNotification`, clearing all) goes through [`close`], so state,
//! subscribers and D-Bus clients always agree.

use std::{
    collections::HashMap,
    sync::{
        LazyLock, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::task::AbortHandle;
use zbus::{Connection, object_server::SignalEmitter};

use crate::notifications::{
    EVENTS, NOTIFICATIONS_STATE, NotificationEvent, daemon::NotificationsDaemon,
};

/// Object path the daemon is served at.
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

/// The daemon's bus connection, used to emit `NotificationClosed`.
static CONNECTION: OnceLock<Connection> = OnceLock::new();

/// Pending expiry timers by notification id, tagged with a generation so a
/// timer that already fired can't remove its replacement's entry.
static EXPIRY_TIMERS: LazyLock<Mutex<HashMap<u32, (u64, AbortHandle)>>> =
    LazyLock::new(Default::default);

static NEXT_TIMER_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Why a notification was closed, as reported in `NotificationClosed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    /// Its expire timeout elapsed.
    Expired = 1,
    /// The user dismissed it.
    Dismissed = 2,
    /// A client called `CloseNotification`.
    Closed = 3,
}

/// Remembers the daemon's connection so closes can be signalled on D-Bus.
pub fn set_connection(connection: Connection) {
    let _ = CONNECTION.set(connection);
}

/// Closes a notification: cancels its expiry timer, removes it from state,
/// tells subscribers, and emits `NotificationClosed`.
///
/// Returns `false` if no notification with this id exists.
pub async fn close(id: u32, reason: CloseReason) -> bool {
    cancel_expiry(id);

    // check first, since every write notifies subscribers
    if !NOTIFICATIONS_STATE.read().notifications.contains_key(&id) {
        return false;
    }
    NOTIFICATIONS_STATE.write().notifications.remove(&id);
    tracing::debug!(notification.id = id, ?reason, "notification closed");

    EVENTS.send(NotificationEvent::Closed { id });

    if let Some(connection) = CONNECTION.get() {
        let emitted = match SignalEmitter::new(connection, OBJECT_PATH) {
            Ok(emitter) => {
                NotificationsDaemon::notification_closed(&emitter, id, reason as u32).await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = emitted {
            tracing::error!(
                notification.id = id,
                "couldn't emit NotificationClosed: {e}"
            );
        }
    }

    true
}

/// Closes the notification after `timeout`, replacing any earlier timer for
/// the same id.
pub fn schedule_expiry(id: u32, timeout: Duration) {
    let generation = NEXT_TIMER_GENERATION.fetch_add(1, Ordering::Relaxed);
    let handle = relm4::spawn(async move {
        tokio::time::sleep(timeout).await;

        // a replacement may have taken over this id in the meantime
        if take_timer_if_current(id, generation) {
            close(id, CloseReason::Expired).await;
        }
    })
    .abort_handle();

    if let Some((_, previous)) = timers().insert(id, (generation, handle)) {
        previous.abort();
    }
}

/// Cancels the expiry timer for a notification, if it has one.
pub fn cancel_expiry(id: u32) {
    if let Some((_, handle)) = timers().remove(&id) {
        handle.abort();
    }
}

/// Forgets the timer for `id` if it's still the one from `generation`.
fn take_timer_if_current(id: u32, generation: u64) -> bool {
    let mut timers = timers();
    let is_current = timers
        .get(&id)
        .is_some_and(|(current, _)| *current == generation);
    if is_current {
        timers.remove(&id);
    }
    is_current
}

fn timers() -> MutexGuard<'static, HashMap<u32, (u64, AbortHandle)>> {
    EXPIRY_TIMERS.lock().unwrap_or_else(PoisonError::into_inner)
}

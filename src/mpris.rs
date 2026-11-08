mod proxy;
mod selection;

use std::{collections::HashMap, time::Duration};

use futures_lite::StreamExt;
use relm4::SharedState;
use tokio::{sync::mpsc, task::JoinSet};
use zbus::{fdo::DBusProxy, zvariant::OwnedValue};

use crate::{
    mpris::{proxy::PlayerProxy, selection::Players},
    utils::{backoff::Backoff, shared_state::set_if_changed},
};

pub static MPRIS_STATE: SharedState<Option<MprisState>> = SharedState::new();

/// Bus name prefix shared by every mpris media player.
const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";

#[derive(Debug, Clone, PartialEq)]
pub struct MprisState {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub status: PlaybackStatus,
}

/// Playback status as reported by the `PlaybackStatus` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    fn parse(status: &str) -> Self {
        match status {
            "Playing" => Self::Playing,
            "Paused" => Self::Paused,
            _ => Self::Stopped,
        }
    }
}

/// Messages from per-player watcher tasks to the service loop.
enum PlayerEvent {
    Updated { name: String, state: MprisState },
    Gone { name: String },
}

/// Follows every media player on the session bus and publishes the one the
/// tile should show, reconnecting with a backoff if the bus goes away.
pub async fn run_mpris_service() {
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));
    loop {
        if let Err(e) = run_session().await {
            tracing::warn!("mpris service stopped: {e}");
        }
        set_if_changed(&MPRIS_STATE, None);
        tokio::time::sleep(backoff.next()).await;
    }
}

async fn run_session() -> anyhow::Result<()> {
    let conn = zbus::Connection::session().await?;
    let dbus = DBusProxy::new(&conn).await?;

    // subscribe before listing so players appearing in between aren't missed
    let mut owner_changes = dbus.receive_name_owner_changed().await?;

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let mut watchers = JoinSet::new();
    let mut watched = HashMap::new();
    let mut players = Players::default();

    for name in dbus.list_names().await? {
        if name.starts_with(MPRIS_PREFIX) {
            let name = name.to_string();
            let handle =
                watchers.spawn(watch_player(conn.clone(), name.clone(), events_tx.clone()));
            watched.insert(name, handle);
        }
    }

    loop {
        tokio::select! {
            Some(change) = owner_changes.next() => {
                let args = change.args()?;
                let name = args.name().to_string();
                if !name.starts_with(MPRIS_PREFIX) {
                    continue;
                }

                // a new owner replaces the old watcher; no owner means the
                // player quit
                if let Some(handle) = watched.remove(&name) {
                    handle.abort();
                }
                players.remove(&name);
                if args.new_owner().is_some() {
                    let handle =
                        watchers.spawn(watch_player(conn.clone(), name.clone(), events_tx.clone()));
                    watched.insert(name, handle);
                }
            }
            Some(event) = events_rx.recv() => match event {
                PlayerEvent::Updated { name, state } => players.update(&name, state),
                PlayerEvent::Gone { name } => players.remove(&name),
            },
            // reap finished watchers so the set doesn't grow forever
            Some(_) = watchers.join_next() => continue,
            else => anyhow::bail!("session bus signal stream ended"),
        }

        set_if_changed(&MPRIS_STATE, players.selected().cloned());
    }
}

/// Reports one player's state whenever its status or metadata changes.
async fn watch_player(
    conn: zbus::Connection,
    name: String,
    events: mpsc::UnboundedSender<PlayerEvent>,
) {
    if let Err(e) = report_player_changes(&conn, &name, &events).await {
        tracing::debug!("stopped watching mpris player {name}: {e}");
    }
    let _ = events.send(PlayerEvent::Gone { name });
}

async fn report_player_changes(
    conn: &zbus::Connection,
    name: &str,
    events: &mpsc::UnboundedSender<PlayerEvent>,
) -> anyhow::Result<()> {
    let player = PlayerProxy::builder(conn)
        .destination(name.to_owned())?
        .build()
        .await?;

    let mut status_changes = player.receive_playback_status_changed().await;
    let mut metadata_changes = player.receive_metadata_changed().await;

    loop {
        let status = player.playback_status().await?;
        let metadata = player.metadata().await.unwrap_or_default();
        let state = state_from_properties(&status, &metadata);
        events.send(PlayerEvent::Updated {
            name: name.to_owned(),
            state,
        })?;

        // the property cache is already updated when a change arrives, so
        // the next iteration simply re-reads it
        tokio::select! {
            Some(_) = status_changes.next() => {}
            Some(_) = metadata_changes.next() => {}
            else => anyhow::bail!("property streams ended"),
        }
    }
}

/// Builds the tile state from raw mpris properties.
fn state_from_properties(status: &str, metadata: &HashMap<String, OwnedValue>) -> MprisState {
    let title = metadata
        .get("xesam:title")
        .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
        .filter(|title| !title.is_empty());
    let artist = metadata
        .get("xesam:artist")
        .and_then(|value| Vec::<String>::try_from(value.try_clone().ok()?).ok())
        .and_then(|artists| artists.into_iter().find(|artist| !artist.is_empty()));

    MprisState {
        title,
        artist,
        status: PlaybackStatus::parse(status),
    }
}

#[cfg(test)]
mod tests {
    use zbus::zvariant::Value;

    use super::*;

    fn owned(value: Value<'_>) -> OwnedValue {
        value.try_to_owned().unwrap()
    }

    #[test]
    fn reads_title_and_first_artist() {
        let metadata = HashMap::from([
            ("xesam:title".to_owned(), owned(Value::from("Song"))),
            (
                "xesam:artist".to_owned(),
                owned(Value::from(vec!["Artist", "Other"])),
            ),
        ]);

        let state = state_from_properties("Playing", &metadata);

        assert_eq!(state.title.as_deref(), Some("Song"));
        assert_eq!(state.artist.as_deref(), Some("Artist"));
        assert_eq!(state.status, PlaybackStatus::Playing);
    }

    #[test]
    fn missing_metadata_is_none() {
        let state = state_from_properties("Paused", &HashMap::new());

        assert_eq!(state.title, None);
        assert_eq!(state.artist, None);
        assert_eq!(state.status, PlaybackStatus::Paused);
    }

    #[test]
    fn unknown_status_is_stopped() {
        assert_eq!(PlaybackStatus::parse("Buffering"), PlaybackStatus::Stopped);
    }
}

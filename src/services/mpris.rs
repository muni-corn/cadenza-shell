use std::time::Duration;

use anyhow::Result;
use mpris::{PlaybackStatus, Player, PlayerFinder};
use relm4::Worker;

#[derive(Debug, Clone, PartialEq)]
pub struct MprisState {
    pub title: String,
    pub artist: String,
    pub status: MprisPlaybackStatus,
    pub has_player: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum MprisPlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl From<PlaybackStatus> for MprisPlaybackStatus {
    fn from(status: PlaybackStatus) -> Self {
        match status {
            PlaybackStatus::Playing => Self::Playing,
            PlaybackStatus::Paused => Self::Paused,
            PlaybackStatus::Stopped => Self::Stopped,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MprisWorkerOutput {
    StateChanged(MprisState),
    Error(String),
}

#[derive(Debug)]
pub struct MprisService;

impl Worker for MprisService {
    type Init = ();
    type Input = ();
    type Output = MprisWorkerOutput;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        let sender_clone = sender.clone();

        // spawn async task to poll for mpris state
        relm4::spawn(async move {
            let mut last_state = MprisState {
                title: String::new(),
                artist: String::new(),
                status: MprisPlaybackStatus::Stopped,
                has_player: false,
            };

            loop {
                let current_state = match find_active_player() {
                    Ok(player) => get_player_state(&player),
                    Err(_) => MprisState {
                        title: String::new(),
                        artist: String::new(),
                        status: MprisPlaybackStatus::Stopped,
                        has_player: false,
                    },
                };

                if current_state != last_state {
                    let _ =
                        sender_clone.output(MprisWorkerOutput::StateChanged(current_state.clone()));
                    last_state = current_state;
                }

                // poll every 2 seconds
                // TODO react immediately to changes
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        Self
    }

    fn update(&mut self, _message: Self::Input, _sender: relm4::ComponentSender<Self>) {
        // no inputs to handle
    }
}

fn find_active_player() -> Result<Player> {
    Ok(PlayerFinder::new()?.find_active()?)
}

fn get_player_state(player: &Player) -> MprisState {
    let metadata = player.get_metadata().unwrap_or_default();

    let title = metadata.title().unwrap_or("").to_string();
    let artist = metadata
        .artists()
        .and_then(|artists| artists.first().map(|s| s.to_string()))
        .unwrap_or_default();

    let status = player
        .get_playback_status()
        .map(MprisPlaybackStatus::from)
        .unwrap_or(MprisPlaybackStatus::Stopped);

    MprisState {
        title,
        artist,
        status,
        has_player: true,
    }
}

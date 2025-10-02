use anyhow::Result;
use mpris::{Event, PlaybackStatus, Player, PlayerFinder};
use relm4::{SharedState, Worker};

pub static MPRIS_STATE: SharedState<Option<MprisState>> = SharedState::new();

#[derive(Debug, Clone, PartialEq)]
pub struct MprisState {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub status: PlaybackStatus,
}

#[derive(Debug)]
pub struct MprisService;

impl Worker for MprisService {
    type Init = ();
    type Input = ();
    type Output = ();

    fn init(_init: Self::Init, _sender: relm4::ComponentSender<Self>) -> Self {
        // spawn blocking thread for event-driven mpris monitoring
        // we need to use std::thread because MPRIS Player is not Send + Sync
        std::thread::spawn(move || {
            loop {
                // find an active player
                match find_active_player() {
                    Ok(player) => {
                        // emit initial state
                        let current_state = get_player_state(&player);
                        if current_state != *MPRIS_STATE.read() {
                            *MPRIS_STATE.write() = current_state.clone();
                        }

                        // subscribe to player events
                        match player.events() {
                            Ok(events) => {
                                // listen for events from this player
                                for event in events {
                                    match event {
                                        Ok(event) => {
                                            // update state based on event
                                            let new_state = handle_player_event(
                                                &event,
                                                MPRIS_STATE.read().clone(),
                                            );

                                            if new_state != *MPRIS_STATE.read() {
                                                *MPRIS_STATE.write() = new_state;
                                            }

                                            // if player shuts down, break out of event loop to find
                                            // a new player
                                            if matches!(event, Event::PlayerShutDown) {
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            // error getting event - player may have disconnected
                                            log::error!(
                                                "error receiving event for mpris player: {}",
                                                e
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                // failed to get events - player may not support it or be
                                // disconnected wait a bit before
                                // trying to find another player
                                log::error!("error starting event stream for mpris player: {}", e);
                                std::thread::sleep(std::time::Duration::from_secs(2));
                            }
                        }
                    }
                    Err(_) => {
                        // no players found - silently emit no-player state and wait before retrying
                        if MPRIS_STATE.read().is_some() {
                            *MPRIS_STATE.write() = None;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(3));
                    }
                }
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

fn get_player_state(player: &Player) -> Option<MprisState> {
    let metadata = player.get_metadata().unwrap_or_default();

    let title = metadata.title().map(String::from);
    let artist = metadata
        .artists()
        .and_then(|artists| artists.first().map(|s| s.to_string()));

    let status = player
        .get_playback_status()
        .unwrap_or(PlaybackStatus::Stopped);

    Some(MprisState {
        title,
        artist,
        status,
    })
}

fn handle_player_event(event: &Event, mut current_state: Option<MprisState>) -> Option<MprisState> {
    match current_state.as_mut() {
        Some(MprisState {
            title,
            artist,
            status,
        }) => {
            match event {
                Event::Playing => *status = PlaybackStatus::Playing,
                Event::Paused => *status = PlaybackStatus::Paused,
                Event::Stopped => *status = PlaybackStatus::Stopped,
                Event::TrackChanged(metadata) => {
                    *title = metadata.title().map(String::from);
                    *artist = metadata
                        .album_artists()
                        .and_then(|artists| artists.first().map(|s| s.to_string()));
                }
                Event::PlayerShutDown => return None,

                // for other events that don't affect our displayed state, return current state
                _ => (),
            }

            current_state
        }
        None => match event {
            Event::Paused => Some(MprisState {
                title: None,
                artist: None,
                status: PlaybackStatus::Paused,
            }),
            Event::Stopped => Some(MprisState {
                title: None,
                artist: None,
                status: PlaybackStatus::Stopped,
            }),
            Event::Playing => Some(MprisState {
                title: None,
                artist: None,
                status: PlaybackStatus::Playing,
            }),
            Event::TrackChanged(metadata) => Some(MprisState {
                title: metadata.title().map(String::from),
                artist: metadata
                    .album_artists()
                    .and_then(|a| a.first().map(|s| s.to_string())),
                status: PlaybackStatus::Paused,
            }),
            _ => current_state,
        },
    }
}

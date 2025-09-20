use anyhow::Result;
use mpris::{Event, PlaybackStatus, Player, PlayerFinder};
use relm4::Worker;

#[derive(Debug, Clone, PartialEq)]
pub enum MprisState {
    Unavailable,
    Info {
        title: Option<String>,
        artist: Option<String>,
        status: PlaybackStatus,
    },
}

#[derive(Debug)]
pub struct MprisService;

impl Worker for MprisService {
    type Init = ();
    type Input = ();
    type Output = MprisState;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        let sender_clone = sender.clone();

        // spawn blocking thread for event-driven mpris monitoring
        // we need to use std::thread because MPRIS Player is not Send + Sync
        std::thread::spawn(move || {
            let mut last_state = MprisState::Unavailable;

            loop {
                // find an active player
                match find_active_player() {
                    Ok(player) => {
                        // emit initial state
                        let current_state = get_player_state(&player);
                        if current_state != last_state {
                            let _ = sender_clone.output(current_state.clone());
                            last_state = current_state;
                        }

                        // subscribe to player events
                        match player.events() {
                            Ok(events) => {
                                // listen for events from this player
                                for event in events {
                                    match event {
                                        Ok(event) => {
                                            // update state based on event
                                            let new_state =
                                                handle_player_event(&event, last_state.clone());

                                            if new_state != last_state {
                                                let _ = sender_clone.output(new_state.clone());
                                                last_state = new_state;
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
                        let no_player_state = MprisState::Unavailable;
                        if no_player_state != last_state {
                            let _ = sender_clone.output(no_player_state.clone());
                            last_state = no_player_state;
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

fn get_player_state(player: &Player) -> MprisState {
    let metadata = player.get_metadata().unwrap_or_default();

    let title = metadata.title().map(String::from);
    let artist = metadata
        .artists()
        .and_then(|artists| artists.first().map(|s| s.to_string()));

    let status = player
        .get_playback_status()
        .unwrap_or(PlaybackStatus::Stopped);

    MprisState::Info {
        title,
        artist,
        status,
    }
}

fn handle_player_event(event: &Event, mut current_state: MprisState) -> MprisState {
    match &mut current_state {
        MprisState::Info {
            title,
            artist,
            status,
        } => {
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
                Event::PlayerShutDown => return MprisState::Unavailable,

                // for other events that don't affect our displayed state, return current state
                _ => (),
            }

            current_state
        }
        MprisState::Unavailable => match event {
            Event::Paused => MprisState::Info {
                title: None,
                artist: None,
                status: PlaybackStatus::Paused,
            },
            Event::Stopped => MprisState::Info {
                title: None,
                artist: None,
                status: PlaybackStatus::Stopped,
            },
            Event::Playing => MprisState::Info {
                title: None,
                artist: None,
                status: PlaybackStatus::Playing,
            },
            Event::TrackChanged(metadata) => MprisState::Info {
                title: metadata.title().map(String::from),
                artist: metadata
                    .album_artists()
                    .and_then(|a| a.first().map(|s| s.to_string())),
                status: PlaybackStatus::Paused,
            },
            _ => current_state,
        },
    }
}

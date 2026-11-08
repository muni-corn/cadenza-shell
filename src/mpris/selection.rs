use std::collections::HashMap;

use super::{MprisState, PlaybackStatus};

/// Tracks every running media player and picks the one the tile should show.
///
/// The choice prefers whichever player most recently started playing; if
/// nothing is playing, the player that changed most recently wins. A logical
/// clock instead of wall time keeps the ordering deterministic and testable.
#[derive(Debug, Default)]
pub struct Players {
    players: HashMap<String, Entry>,
    clock: u64,
}

#[derive(Debug)]
struct Entry {
    state: MprisState,
    started_playing: u64,
    last_changed: u64,
}

impl Players {
    /// Records the latest state reported by the player at `name`.
    pub fn update(&mut self, name: &str, state: MprisState) {
        self.clock += 1;
        let now = self.clock;

        match self.players.get_mut(name) {
            Some(entry) => {
                if state.status == PlaybackStatus::Playing
                    && entry.state.status != PlaybackStatus::Playing
                {
                    entry.started_playing = now;
                }
                entry.state = state;
                entry.last_changed = now;
            }
            None => {
                let started_playing = if state.status == PlaybackStatus::Playing {
                    now
                } else {
                    0
                };
                self.players.insert(name.to_owned(), Entry {
                    state,
                    started_playing,
                    last_changed: now,
                });
            }
        }
    }

    /// Forgets a player that left the bus.
    pub fn remove(&mut self, name: &str) {
        self.players.remove(name);
    }

    /// Returns the state of the player the tile should follow, if any.
    pub fn selected(&self) -> Option<&MprisState> {
        let playing = self
            .players
            .values()
            .filter(|entry| entry.state.status == PlaybackStatus::Playing)
            .max_by_key(|entry| entry.started_playing);

        playing
            .or_else(|| self.players.values().max_by_key(|entry| entry.last_changed))
            .map(|entry| &entry.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(title: &str, status: PlaybackStatus) -> MprisState {
        MprisState {
            title: Some(title.to_owned()),
            artist: None,
            status,
        }
    }

    fn selected_title(players: &Players) -> Option<&str> {
        players.selected().and_then(|s| s.title.as_deref())
    }

    #[test]
    fn empty_selects_nothing() {
        assert!(Players::default().selected().is_none());
    }

    #[test]
    fn prefers_the_most_recently_started_player() {
        let mut players = Players::default();
        players.update("a", state("a", PlaybackStatus::Playing));
        players.update("b", state("b", PlaybackStatus::Playing));
        assert_eq!(selected_title(&players), Some("b"));

        // a track change on the older player doesn't steal the selection
        players.update("a", state("a2", PlaybackStatus::Playing));
        assert_eq!(selected_title(&players), Some("b"));
    }

    #[test]
    fn playing_beats_a_more_recent_paused_player() {
        let mut players = Players::default();
        players.update("a", state("a", PlaybackStatus::Playing));
        players.update("b", state("b", PlaybackStatus::Paused));
        assert_eq!(selected_title(&players), Some("a"));
    }

    #[test]
    fn falls_back_to_the_most_recently_changed_player() {
        let mut players = Players::default();
        players.update("a", state("a", PlaybackStatus::Playing));
        players.update("b", state("b", PlaybackStatus::Paused));
        players.update("a", state("a", PlaybackStatus::Paused));
        assert_eq!(selected_title(&players), Some("a"));
    }

    #[test]
    fn removed_players_are_forgotten() {
        let mut players = Players::default();
        players.update("a", state("a", PlaybackStatus::Paused));
        players.update("b", state("b", PlaybackStatus::Playing));
        players.remove("b");
        assert_eq!(selected_title(&players), Some("a"));
    }
}

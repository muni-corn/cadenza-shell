use anyhow::Result;
use futures_lite::StreamExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation};
use gtk4::{Label, Orientation};
use std::cell::RefCell;
use std::collections::HashMap;
use zbus::{Connection, proxy, Result as ZbusResult};

const MPRIS_PLAYING_ICON: &str = "󰐊";
const MPRIS_PAUSED_ICON: &str = "󰏤";
const MPRIS_STOPPED_ICON: &str = "󰓛";

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl From<&str> for PlaybackStatus {
    fn from(s: &str) -> Self {
        match s {
            "Playing" => PlaybackStatus::Playing,
            "Paused" => PlaybackStatus::Paused,
            "Stopped" => PlaybackStatus::Stopped,
            _ => PlaybackStatus::Stopped,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
}

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer2Player {
    async fn play(&self) -> ZbusResult<()>;
    async fn pause(&self) -> ZbusResult<()>;
    async fn play_pause(&self) -> ZbusResult<()>;
    async fn stop(&self) -> ZbusResult<()>;
    async fn next(&self) -> ZbusResult<()>;
    async fn previous(&self) -> ZbusResult<()>;

    #[zbus(property)]
    async fn playback_status(&self) -> ZbusResult<String>;

    #[zbus(property)]
    async fn metadata(&self) -> ZbusResult<HashMap<String, zbus::zvariant::OwnedValue>>;

    #[zbus(property)]
    async fn can_play(&self) -> ZbusResult<bool>;

    #[zbus(property)]
    async fn can_pause(&self) -> ZbusResult<bool>;

    #[zbus(property)]
    async fn can_go_next(&self) -> ZbusResult<bool>;

    #[zbus(property)]
    async fn can_go_previous(&self) -> ZbusResult<bool>;
}

pub struct MediaWidget {
    container: Box,
    icon_label: Label,
    title_label: Label,
    artist_label: Label,
    current_player: RefCell<Option<String>>,
    players: RefCell<HashMap<String, MediaPlayer2PlayerProxy<'static>>>,
}

impl MediaWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .visible(false)
            .build();

        let icon_label = Label::builder()
            .css_classes(vec!["icon"])
            .text(MPRIS_STOPPED_ICON)
            .width_request(16)
            .build();

        let content_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .valign(gtk4::Align::Center)
            .build();

        let title_label = Label::builder()
            .css_classes(vec!["primary"])
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(30)
            .build();

        let artist_label = Label::builder()
            .css_classes(vec!["secondary"])
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(30)
            .build();

        content_box.append(&title_label);
        content_box.append(&artist_label);

        container.append(&icon_label);
        container.append(&content_box);

        let widget = Self {
            container,
            icon_label,
            title_label,
            artist_label,
            current_player: RefCell::new(None),
            players: RefCell::new(HashMap::new()),
        };

        widget.setup_click_handler();
        widget.initialize_mpris();

        widget
    }

    fn setup_click_handler(&self) {
        let current_player = self.current_player.clone();
        let players = self.players.clone();

        let click_controller = gtk4::GestureClick::new();
        click_controller.connect_pressed(move |_, _, _, _| {
            if let Some(player_name) = current_player.borrow().as_ref() {
                if let Some(player) = players.borrow().get(player_name) {
                    let player = player.clone();
                    glib::spawn_future_local(async move {
                        if let Err(e) = player.play_pause().await {
                            log::warn!("Failed to toggle playback: {}", e);
                        }
                    });
                }
            }
        });

        self.container.add_controller(click_controller);
    }

    fn initialize_mpris(&self) {
        let container = self.container.clone();
        let icon_label = self.icon_label.clone();
        let title_label = self.title_label.clone();
        let artist_label = self.artist_label.clone();
        let current_player = self.current_player.clone();
        let players = self.players.clone();

        glib::spawn_future_local(async move {
            if let Err(e) = Self::setup_mpris_monitoring(
                container,
                icon_label,
                title_label,
                artist_label,
                current_player,
                players,
            )
            .await
            {
                log::warn!("Failed to initialize MPRIS: {}", e);
            }
        });
    }

    async fn setup_mpris_monitoring(
        container: Box,
        icon_label: Label,
        title_label: Label,
        artist_label: Label,
        current_player: RefCell<Option<String>>,
        players: RefCell<HashMap<String, MediaPlayer2PlayerProxy<'static>>>,
    ) -> Result<()> {
        let connection = Connection::session().await?;

        // Discover existing MPRIS players
        let dbus_proxy = zbus::fdo::DBusProxy::new(&connection).await?;
        let names = dbus_proxy.list_names().await?;

        for name in names {
            if name.starts_with("org.mpris.MediaPlayer2.") {
                Self::add_player(
                    &connection,
                    &name,
                    &container,
                    &icon_label,
                    &title_label,
                    &artist_label,
                    &current_player,
                    &players,
                ).await;
            }
        }

        // Monitor for new players
        let mut name_owner_changed_stream = dbus_proxy.receive_name_owner_changed().await?;
        glib::spawn_future_local(async move {
            while let Some(signal) = name_owner_changed_stream.next().await {
                if let Ok(args) = signal.args() {
                    let name = &args.name;
                    let new_owner = &args.new_owner;

                    if name.starts_with("org.mpris.MediaPlayer2.") {
                        if new_owner.is_empty() {
                            // Player removed
                            Self::remove_player(name, &container, &current_player, &players);
                        } else {
                            // Player added
                            Self::add_player(
                                &connection,
                                name,
                                &container,
                                &icon_label,
                                &title_label,
                                &artist_label,
                                &current_player,
                                &players,
                            )
                            .await;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn add_player(
        connection: &Connection,
        player_name: &str,
        container: &Box,
        icon_label: &Label,
        title_label: &Label,
        artist_label: &Label,
        current_player: &RefCell<Option<String>>,
        players: &RefCell<HashMap<String, MediaPlayer2PlayerProxy<'static>>>,
    ) {
        let player_proxy = match MediaPlayer2PlayerProxy::builder(connection)
            .destination(player_name)
            .unwrap()
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(e) => {
                log::warn!("Failed to create proxy for player {}: {}", player_name, e);
                return;
            }
        };

        players.borrow_mut().insert(player_name.to_string(), player_proxy.clone());

        // If this is the first player or there's no current player, make it active
        if current_player.borrow().is_none() {
            *current_player.borrow_mut() = Some(player_name.to_string());
            Self::update_display(&player_proxy, container, icon_label, title_label, artist_label).await;
        }

        // Monitor property changes for this player
        let player_name_clone = player_name.to_string();
        let container_clone = container.clone();
        let icon_label_clone = icon_label.clone();
        let title_label_clone = title_label.clone();
        let artist_label_clone = artist_label.clone();
        let current_player_clone = current_player.clone();

        let mut properties_changed_stream = player_proxy.receive_properties_changed().await.unwrap();
        glib::spawn_future_local(async move {
            while let Some(_signal) = properties_changed_stream.next().await {
                // Only update if this is the current active player
                if let Some(active_player) = current_player_clone.borrow().as_ref() {
                    if *active_player == player_name_clone {
                        if let Some(player) = players.borrow().get(&player_name_clone) {
                            Self::update_display(
                                player,
                                &container_clone,
                                &icon_label_clone,
                                &title_label_clone,
                                &artist_label_clone,
                            ).await;
                        }
                    }
                }
            }
        });
    }

    fn remove_player(
        player_name: &str,
        container: &Box,
        current_player: &RefCell<Option<String>>,
        players: &RefCell<HashMap<String, MediaPlayer2PlayerProxy<'static>>>,
    ) {
        players.borrow_mut().remove(player_name);

        // If this was the current player, find another one or hide the widget
        if let Some(active_player) = current_player.borrow().as_ref() {
            if *active_player == player_name {
                let remaining_players: Vec<String> = players.borrow().keys().cloned().collect();
                if let Some(new_player) = remaining_players.first() {
                    *current_player.borrow_mut() = Some(new_player.clone());
                    // TODO: Update display for new player
                } else {
                    *current_player.borrow_mut() = None;
                    container.set_visible(false);
                }
            }
        }
    }

    async fn update_display(
        player: &MediaPlayer2PlayerProxy<'_>,
        container: &Box,
        icon_label: &Label,
        title_label: &Label,
        artist_label: &Label,
    ) {
        // Get playback status
        let status = match player.playback_status().await {
            Ok(status) => PlaybackStatus::from(status.as_str()),
            Err(_) => PlaybackStatus::Stopped,
        };

        // Get metadata
        let metadata = player.metadata().await.unwrap_or_default();
        let title = Self::extract_metadata_string(&metadata, "xesam:title")
            .unwrap_or_else(|| "Unknown".to_string());
        let artist = Self::extract_metadata_string(&metadata, "xesam:artist")
            .unwrap_or_else(|| "Unknown Artist".to_string());

        // Update icon based on status
        let icon = match status {
            PlaybackStatus::Playing => MPRIS_PLAYING_ICON,
            PlaybackStatus::Paused => MPRIS_PAUSED_ICON,
            PlaybackStatus::Stopped => MPRIS_STOPPED_ICON,
        };

        icon_label.set_text(icon);
        title_label.set_text(&Self::truncate_text(&title, 30));
        artist_label.set_text(&Self::truncate_text(&artist, 30));

        // Show widget if not stopped
        container.set_visible(status != PlaybackStatus::Stopped);
    }

    fn extract_metadata_string(
        metadata: &HashMap<String, zbus::zvariant::OwnedValue>,
        key: &str,
    ) -> Option<String> {
        metadata.get(key).and_then(|value| {
            // Handle both string and array of strings
            if let Ok(s) = value.try_to::<String>() {
                Some(s)
            } else if let Ok(arr) = value.try_to::<Vec<String>>() {
                arr.first().cloned()
            } else {
                None
            }
        })
    }

    fn truncate_text(text: &str, max_chars: usize) -> String {
        if text.chars().count() > max_chars {
            format!("{}…", text.chars().take(max_chars - 1).collect::<String>())
        } else {
            text.to_string()
        }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}

impl Default for MediaWidget {
    fn default() -> Self {
        Self::new()
    }
}
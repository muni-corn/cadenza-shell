use std::collections::HashMap;

use zbus::{proxy, zvariant::OwnedValue};

/// The `org.mpris.MediaPlayer2.Player` interface, limited to the properties
/// the media tile shows.
#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
pub trait Player {
    /// One of `Playing`, `Paused` or `Stopped`.
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    /// Track metadata keyed by xesam and mpris field names.
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

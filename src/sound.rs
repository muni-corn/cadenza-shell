use tokio::process::Command;

/// Play an XDG sound theme event using `canberra-gtk-play`.
///
/// Spawns `canberra-gtk-play --id <event_id>` as a background process. Logs a
/// warning if the process fails to start or exits with a non-zero status, but
/// never panics.
pub fn play(event_id: &str) {
    if let Err(e) = Command::new("canberra-gtk-play")
        .arg("--id")
        .arg(event_id)
        .spawn()
    {
        log::error!("couldn't spawn canberra-gtk-play: {e}");
    }
}

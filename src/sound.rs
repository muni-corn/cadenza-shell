use tokio::process::Command;

/// Play an XDG sound theme event using `canberra-gtk-play`.
///
/// Spawns `canberra-gtk-play --id <event_id>` in the background and waits
/// for it on a separate task, logging if it fails to start or exits with a
/// non-zero status. Never panics.
pub fn play(event_id: &str) {
    let mut command = Command::new("canberra-gtk-play");
    command.arg("--id").arg(event_id);
    spawn_and_reap(command);
}

/// Play a sound file using `canberra-gtk-play`, in the background like
/// [`play`].
pub fn play_file(path: &str) {
    let mut command = Command::new("canberra-gtk-play");
    command.arg("--file").arg(path);
    spawn_and_reap(command);
}

/// Starts `command` and waits for it in the background, so finished players
/// don't linger as zombie processes.
fn spawn_and_reap(mut command: Command) {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!("couldn't start canberra-gtk-play; is libcanberra installed? {e}");
            return;
        }
    };

    relm4::spawn(async move {
        match child.wait().await {
            Ok(status) if !status.success() => {
                tracing::warn!("canberra-gtk-play exited with {status}");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("couldn't wait for canberra-gtk-play: {e}"),
        }
    });
}

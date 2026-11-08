use niri_ipc::{
    Event, Reply, Request, Workspace as NiriWorkspace,
    state::{EventStreamState, EventStreamStatePart},
};
use relm4::SharedState;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::utils::shared_state::set_if_changed;

pub static NIRI_STATE: SharedState<Option<NiriState>> = SharedState::new();

#[derive(Debug, Clone, PartialEq)]
pub struct NiriState {
    pub workspaces: Vec<NiriWorkspace>,
    pub focused_window_title: String,
    pub focused_output: String,
}

impl NiriState {
    /// Extracts what the bar shows from niri's tracked event stream state.
    fn from_event_stream(state: &EventStreamState) -> Self {
        let mut workspaces: Vec<NiriWorkspace> =
            state.workspaces.workspaces.values().cloned().collect();
        workspaces.sort_by_key(|workspace| workspace.idx);

        let focused_window_title = state
            .windows
            .windows
            .values()
            .find(|window| window.is_focused)
            .and_then(|window| window.title.clone())
            .unwrap_or_default();

        let focused_output = workspaces
            .iter()
            .find(|workspace| workspace.is_focused)
            .and_then(|workspace| workspace.output.clone())
            .unwrap_or_default();

        Self {
            workspaces,
            focused_window_title,
            focused_output,
        }
    }
}

pub async fn start_event_listener() {
    let Ok(socket_path) = std::env::var("NIRI_SOCKET") else {
        tracing::warn!("NIRI_SOCKET env var is not available; niri service won't start");
        return;
    };

    loop {
        if let Err(e) = run_event_stream(&socket_path).await {
            tracing::warn!("niri event stream ended: {e}");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Reads niri's event stream until the socket closes or errors.
async fn run_event_stream(socket_path: &str) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path).await?;
    let json = serde_json::to_string(&Request::EventStream)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?; // close write end

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    // niri answers the request itself before streaming events
    if reader.read_line(&mut line).await? == 0 {
        anyhow::bail!("niri closed the socket before replying");
    }
    if let Err(e) = serde_json::from_str::<Reply>(line.trim())? {
        anyhow::bail!("niri refused the event stream request: {e}");
    }

    // niri replays its full current state as the first events, so a fresh
    // state becomes complete without separate requests
    let mut state = EventStreamState::default();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            anyhow::bail!("niri closed the event stream");
        }

        let event = match serde_json::from_str::<Event>(line.trim()) {
            Ok(event) => event,
            Err(e) => {
                tracing::error!("error parsing niri message: {e}");
                continue;
            }
        };

        tracing::trace!("niri event received: {:?}", event);
        // events for parts of the state we don't track are handed back
        let _ = state.apply(event);
        set_if_changed(&NIRI_STATE, Some(NiriState::from_event_stream(&state)));
    }
}

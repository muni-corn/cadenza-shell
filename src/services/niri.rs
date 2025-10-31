use niri_ipc::{Event, Reply, Request, Response, Window as NiriWindow, Workspace as NiriWorkspace};
use relm4::SharedState;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

pub static NIRI_STATE: SharedState<Option<NiriState>> = SharedState::new();

#[derive(Debug, Clone)]
pub struct NiriState {
    pub workspaces: Vec<NiriWorkspace>,
    pub focused_window_title: String,
}

async fn send_request(socket_path: &str, request: Request) -> anyhow::Result<Reply> {
    let json = serde_json::to_string(&request)?;
    let mut stream = UnixStream::connect(socket_path).await?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?; // close write end

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    let reply: Reply = serde_json::from_str(response.trim())?;
    Ok(reply)
}

async fn fetch_and_update(socket_path: &str) -> anyhow::Result<()> {
    let workspaces = if let Ok(reply) = send_request(socket_path, Request::Workspaces).await
        && let Ok(Response::Workspaces(mut ws)) = reply
    {
        ws.sort_by_cached_key(|w| w.id);
        ws
    } else {
        Vec::new()
    };

    let focused_window_title =
        if let Ok(Ok(Response::FocusedWindow(Some(NiriWindow { title, .. })))) =
            send_request(socket_path, Request::FocusedWindow).await
        {
            title.unwrap_or_default()
        } else {
            Default::default()
        };

    *NIRI_STATE.write() = Some(NiriState {
        workspaces,
        focused_window_title,
    });

    Ok(())
}

pub async fn start_event_listener() {
    if let Ok(socket_path) = std::env::var("NIRI_SOCKET") {
        // initial fetch
        fetch_and_update(&socket_path)
            .await
            .unwrap_or_else(|e| log::error!("error getting initial niri state: {e}"));

        loop {
            if let Ok(mut stream) = UnixStream::connect(&socket_path).await {
                let json = serde_json::to_string(&Request::EventStream).unwrap();
                if stream.write_all(json.as_bytes()).await.is_ok() {
                    stream.write_all(b"\n").await.ok();
                    stream.shutdown().await.ok(); // close write end

                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    while reader.read_line(&mut line).await.is_ok() {
                        match serde_json::from_str::<Event>(line.trim()) {
                            Ok(event) => {
                                log::debug!("niri event received: {:?}", event);
                                match event {
                                    Event::WorkspacesChanged { .. }
                                    | Event::WorkspaceActivated { .. }
                                    | Event::WorkspaceActiveWindowChanged { .. }
                                    | Event::WindowsChanged { .. }
                                    | Event::WindowOpenedOrChanged { .. }
                                    | Event::WindowClosed { .. }
                                    | Event::WindowFocusChanged { .. } => {
                                        fetch_and_update(&socket_path).await.unwrap_or_else(|e| {
                                            log::error!("couldn't update niri state: {}", e)
                                        });
                                    }
                                    _ => (),
                                }
                            }
                            Err(e) => log::error!("error parsing niri message: {}", e),
                        }
                        line.clear();
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    } else {
        log::warn!("NIRI_SOCKET env var is not available; niri service won't start");
    }
}

use niri_ipc::{Event, Reply, Request, Response, Window as NiriWindow, Workspace as NiriWorkspace};
use relm4::{SharedState, Worker};
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

#[derive(Debug, Default)]
pub struct NiriService {
    socket_path: Option<String>,
}

impl Worker for NiriService {
    type Init = ();
    type Input = ();
    type Output = ();

    fn init(_init: Self::Init, _sender: relm4::ComponentSender<Self>) -> Self {
        let socket_path = std::env::var("NIRI_SOCKET").ok();
        let service = Self { socket_path };

        if let Some(socket_path) = &service.socket_path {
            let socket_path = socket_path.clone();
            relm4::spawn(async move {
                if let Err(e) = initialize_and_stream(&socket_path).await {
                    log::warn!("niri service error: {}", e);
                    *NIRI_STATE.write() = None;
                }
            });
        } else {
            *NIRI_STATE.write() = None;
        }

        service
    }

    fn update(&mut self, _msg: Self::Input, _sender: relm4::ComponentSender<Self>) {}
}

async fn initialize_and_stream(socket_path: &str) -> anyhow::Result<()> {
    // initial state
    fetch_and_update(socket_path).await?;

    // event stream
    start_event_listener(socket_path).await;
    Ok(())
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
    let mut workspaces: Vec<NiriWorkspace> = Vec::new();
    if let Ok(reply) = send_request(socket_path, Request::Workspaces).await
        && let Ok(Response::Workspaces(ws)) = reply
    {
        workspaces = ws;
    }

    let mut focused_window_title = String::new();
    if let Ok(reply) = send_request(socket_path, Request::FocusedWindow).await
        && let Ok(Response::FocusedWindow(Some(NiriWindow { title, .. }))) = reply
    {
        focused_window_title = title.unwrap_or_default();
    }

    *NIRI_STATE.write() = Some(NiriState {
        workspaces,
        focused_window_title,
    });

    Ok(())
}

async fn start_event_listener(socket_path: &str) {
    loop {
        if let Ok(mut stream) = UnixStream::connect(socket_path).await {
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
                                    fetch_and_update(socket_path).await.unwrap_or_else(|e| {
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
}

use niri_ipc::{Event, Reply, Request, Response, Window as NiriWindow, Workspace as NiriWorkspace};
use relm4::Worker;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

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
    type Output = Option<NiriState>;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        let socket_path = std::env::var("NIRI_SOCKET").ok();
        let service = Self { socket_path };

        if let Some(socket_path) = &service.socket_path {
            let socket_path = socket_path.clone();
            let sender_clone = sender.clone();
            relm4::spawn(async move {
                if let Err(e) = initialize_and_stream(&socket_path, sender_clone.clone()).await {
                    log::warn!("niri service error: {}", e);
                    let _ = sender_clone.output(None);
                }
            });
        } else {
            let _ = sender.output(None);
        }

        service
    }

    fn update(&mut self, _msg: Self::Input, _sender: relm4::ComponentSender<Self>) {}
}

async fn initialize_and_stream(
    socket_path: &str,
    sender: relm4::ComponentSender<NiriService>,
) -> anyhow::Result<()> {
    // initial state
    fetch_and_emit(socket_path, &sender).await?;

    // event stream
    start_event_listener(socket_path, sender).await;
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

async fn fetch_and_emit(
    socket_path: &str,
    sender: &relm4::ComponentSender<NiriService>,
) -> anyhow::Result<()> {
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

    sender
        .output(Some(NiriState {
            workspaces,
            focused_window_title,
        }))
        .unwrap_or_else(|_| log::error!("failed to send niri update"));

    Ok(())
}

async fn start_event_listener(socket_path: &str, sender: relm4::ComponentSender<NiriService>) {
    loop {
        if let Ok(mut stream) = UnixStream::connect(socket_path).await {
            let json = serde_json::to_string(&Request::EventStream).unwrap();
            if stream.write_all(json.as_bytes()).await.is_ok() {
                stream.write_all(b"\n").await.ok();
                stream.shutdown().await.ok(); // close write end

                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                while reader.read_line(&mut line).await.is_ok() {
                    if let Ok(event) = serde_json::from_str::<Event>(line.trim())
                        && let Event::WorkspacesChanged { .. }
                        | Event::WorkspaceActivated { .. }
                        | Event::WorkspaceActiveWindowChanged { .. }
                        | Event::WindowsChanged { .. }
                        | Event::WindowOpenedOrChanged { .. }
                        | Event::WindowClosed { .. }
                        | Event::WindowFocusChanged { .. } = event
                    {
                        let _ = fetch_and_emit(socket_path, &sender).await;
                    }
                    line.clear();
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

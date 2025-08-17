use gtk4::glib;
use gtk4::subclass::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub windows: u32,
    pub hasfullscreen: bool,
    pub lastwindow: String,
    pub lastwindowtitle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub address: String,
    pub mapped: bool,
    pub hidden: bool,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub workspace: WorkspaceRef,
    pub floating: bool,
    pub monitor: i32,
    pub class: String,
    pub title: String,
    pub initialclass: String,
    pub initialtitle: String,
    pub pid: i32,
    pub xwayland: bool,
    pub pinned: bool,
    pub fullscreen: bool,
    pub fullscreenmode: i32,
    pub fakeFullscreen: bool,
    pub grouped: Vec<String>,
    pub swallowing: String,
    pub focusHistoryID: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRef {
    pub id: i32,
    pub name: String,
}

mod imp {
    use super::{Client, Workspace};
    use anyhow::Result;
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::HyprlandService)]
    pub struct HyprlandService {
        #[property(get, set)]
        available: RefCell<bool>,

        #[property(get, set)]
        active_workspace_id: RefCell<i32>,

        #[property(get, set)]
        active_workspace_name: RefCell<String>,

        #[property(get, set)]
        focused_window_title: RefCell<String>,

        #[property(get, set)]
        focused_window_class: RefCell<String>,

        workspaces: RefCell<HashMap<i32, Workspace>>,
        clients: RefCell<HashMap<String, Client>>,
        socket_path: RefCell<String>,
        event_socket_path: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HyprlandService {
        const NAME: &'static str = "MuseShellHyprlandService";
        type Type = super::HyprlandService;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for HyprlandService {
        fn constructed(&self) {
            self.parent_constructed();

            // Initialize Hyprland connection
            if let Ok(signature) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") {
                let socket_path = format!("/tmp/hypr/{}/.socket.sock", signature);
                let event_socket_path = format!("/tmp/hypr/{}/.socket2.sock", signature);

                self.socket_path.replace(socket_path);
                self.event_socket_path.replace(event_socket_path);
                self.available.replace(true);

                // Initialize state asynchronously
                let obj = self.obj().clone();
                glib::spawn_future_local(async move {
                    if let Err(e) = obj.imp().initialize_state().await {
                        log::warn!("Failed to initialize Hyprland state: {}", e);
                        obj.imp().available.replace(false);
                    } else {
                        // Start event listener
                        obj.imp().start_event_listener().await;
                    }
                });
            } else {
                log::warn!("HYPRLAND_INSTANCE_SIGNATURE not found, Hyprland service unavailable");
                self.available.replace(false);
            }
        }
    }

    impl HyprlandService {
        async fn initialize_state(&self) -> Result<()> {
            // Get initial workspaces
            if let Ok(workspaces) = self.get_workspaces().await {
                let mut workspace_map = HashMap::new();
                for workspace in workspaces {
                    workspace_map.insert(workspace.id, workspace);
                }
                self.workspaces.replace(workspace_map);
            }

            // Get initial clients
            if let Ok(clients) = self.get_clients().await {
                let mut client_map = HashMap::new();
                for client in clients {
                    client_map.insert(client.address.clone(), client);
                }
                self.clients.replace(client_map);
            }

            // Get active workspace
            if let Ok(active_workspace) = self.get_active_workspace().await {
                self.active_workspace_id.replace(active_workspace.id);
                self.active_workspace_name.replace(active_workspace.name);
            }

            // Get active window
            if let Ok(active_window) = self.get_active_window().await {
                if let Some(client) = active_window {
                    self.focused_window_title.replace(client.title);
                    self.focused_window_class.replace(client.class);
                }
            }

            Ok(())
        }

        async fn send_command(&self, command: &str) -> Result<String> {
            let socket_path = self.socket_path.borrow().clone();
            let mut stream = UnixStream::connect(&socket_path).await?;
            stream.write_all(command.as_bytes()).await?;

            let mut response = String::new();
            stream.read_to_string(&mut response).await?;

            Ok(response)
        }

        async fn get_workspaces(&self) -> Result<Vec<Workspace>> {
            let response = self.send_command("j/workspaces").await?;
            let workspaces: Vec<Workspace> = serde_json::from_str(&response)?;
            Ok(workspaces)
        }

        async fn get_clients(&self) -> Result<Vec<Client>> {
            let response = self.send_command("j/clients").await?;
            let clients: Vec<Client> = serde_json::from_str(&response)?;
            Ok(clients)
        }

        async fn get_active_workspace(&self) -> Result<Workspace> {
            let response = self.send_command("j/activeworkspace").await?;
            let workspace: Workspace = serde_json::from_str(&response)?;
            Ok(workspace)
        }

        async fn get_active_window(&self) -> Result<Option<Client>> {
            let response = self.send_command("j/activewindow").await?;
            if response.trim().is_empty() || response.trim() == "null" {
                return Ok(None);
            }
            let client: Client = serde_json::from_str(&response)?;
            Ok(Some(client))
        }

        async fn start_event_listener(&self) {
            let event_socket_path = self.event_socket_path.borrow().clone();
            let obj = self.obj().clone();

            glib::spawn_future_local(async move {
                loop {
                    if let Ok(mut stream) = UnixStream::connect(&event_socket_path).await {
                        let mut buffer = vec![0; 4096];

                        loop {
                            match stream.read(&mut buffer).await {
                                Ok(0) => break, // Connection closed
                                Ok(n) => {
                                    let event = String::from_utf8_lossy(&buffer[..n]);
                                    obj.imp().handle_event(&event).await;
                                }
                                Err(e) => {
                                    log::warn!("Error reading from Hyprland event socket: {}", e);
                                    break;
                                }
                            }
                        }
                    }

                    // Wait before reconnecting
                    glib::timeout_future(std::time::Duration::from_secs(1)).await;
                }
            });
        }

        async fn handle_event(&self, event: &str) {
            for line in event.lines() {
                if let Some((event_type, data)) = line.split_once(">>") {
                    match event_type {
                        "workspace" => {
                            if let Ok(workspace_id) = data.parse::<i32>() {
                                self.active_workspace_id.replace(workspace_id);

                                // Update workspace name if we have it
                                if let Some(workspace) = self.workspaces.borrow().get(&workspace_id)
                                {
                                    self.active_workspace_name.replace(workspace.name.clone());
                                }
                            }
                        }
                        "activewindow" => {
                            let parts: Vec<&str> = data.splitn(2, ',').collect();
                            if parts.len() == 2 {
                                self.focused_window_class.replace(parts[0].to_string());
                                self.focused_window_title.replace(parts[1].to_string());
                            }
                        }
                        "createworkspace" => {
                            // Refresh workspaces when a new one is created
                            let obj = self.obj().clone();
                            glib::spawn_future_local(async move {
                                if let Ok(workspaces) = obj.imp().get_workspaces().await {
                                    let mut workspace_map = HashMap::new();
                                    for workspace in workspaces {
                                        workspace_map.insert(workspace.id, workspace);
                                    }
                                    obj.imp().workspaces.replace(workspace_map);
                                }
                            });
                        }
                        "destroyworkspace" => {
                            if let Ok(workspace_id) = data.parse::<i32>() {
                                self.workspaces.borrow_mut().remove(&workspace_id);
                            }
                        }
                        _ => {
                            // Handle other events as needed
                        }
                    }
                }
            }
        }

        pub fn get_workspaces_list(&self) -> Vec<Workspace> {
            self.workspaces.borrow().values().cloned().collect()
        }

        pub fn get_workspace_by_id(&self, id: i32) -> Option<Workspace> {
            self.workspaces.borrow().get(&id).cloned()
        }

        pub fn get_socket_path(&self) -> String {
            self.socket_path.borrow().clone()
        }

        pub fn get_active_workspace_id(&self) -> i32 {
            *self.active_workspace_id.borrow()
        }
    }
}

glib::wrapper! {
    pub struct HyprlandService(ObjectSubclass<imp::HyprlandService>);
}

impl Default for HyprlandService {
    fn default() -> Self {
        Self::new()
    }
}

impl HyprlandService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn workspaces(&self) -> Vec<Workspace> {
        self.imp().get_workspaces_list()
    }

    pub fn workspace_by_id(&self, id: i32) -> Option<Workspace> {
        self.imp().get_workspace_by_id(id)
    }

    pub fn switch_to_workspace(&self, id: i32) {
        let socket_path = self.imp().get_socket_path();
        glib::spawn_future_local(async move {
            if let Ok(mut stream) = UnixStream::connect(&socket_path).await {
                let command = format!("dispatch workspace {}", id);
                if let Err(e) = stream.write_all(command.as_bytes()).await {
                    log::warn!("Failed to switch workspace: {}", e);
                }
            }
        });
    }

    pub fn is_workspace_active(&self, id: i32) -> bool {
        self.imp().get_active_workspace_id() == id
    }

    pub fn has_focused_window(&self) -> bool {
        !self.focused_window_title().is_empty()
    }
}

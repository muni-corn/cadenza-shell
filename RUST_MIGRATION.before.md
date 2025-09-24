# Cadenza Shell Migration Plan: TypeScript/AGS to Rust/gtk-rs

## Executive Summary

This document outlines a comprehensive plan to migrate the Cadenza Shell desktop
environment from its current TypeScript/AGS/Astal implementation to a Rust-based
solution using gtk-rs, GTK4, and gtk4-layer-shell. The migration will maintain
all existing functionality while providing better type safety, performance, and
a more maintainable codebase.

## Current Architecture Analysis

### Technology Stack

- **Language**: TypeScript with JSX-style components
- **Framework**: AGS v3 (Aylur's GTK Shell) with Astal libraries
- **UI Library**: GTK4 via GObject Introspection bindings
- **Services**: Astal service bindings (Hyprland, NetworkManager, WirePlumber,
  etc.)
- **Build System**: Nix flakes
- **Layer Shell**: Implicit through AGS's window management

### Core Components

1. **Bar** (`src/bar.tsx`): Top panel with workspaces, clock, system tiles
1. **Tiles**: Modular status indicators (battery, network, volume, brightness,
   etc.)
1. **Notifications**: Notification popups and panel
1. **Services**: Custom brightness service, integration with system services
1. **Utilities**: Reactive state management, icon mapping, UI helpers

## Target Architecture

### Technology Stack

- **Language**: Rust
- **UI Framework**: gtk4-rs (GTK4 Rust bindings)
- **Layer Shell**: gtk4-layer-shell-rs
- **Async Runtime**: tokio with glib integration
- **IPC/Services**: zbus for D-Bus, custom implementations for non-D-Bus
  services
- **Build System**: Cargo and Nix
- **State Management**: Custom reactive system using channels and Arc\<Mutex\<>>

## Migration Strategy

### Phase 1: Project Setup and Foundation

#### 1.1 Initialize Rust Project Structure

```
cadenza-shell/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── app.rs
│   ├── app/
│   │   └── application.rs
│   ├── widgets.rs
│   ├── widgets/
│   │   ├── bar.rs
│   │   ├── window.rs
│   ├── tiles.rs
│   ├── tiles/
│   │   ├── battery.rs
│   │   ├── bluetooth.rs
│   │   ├── brightness.rs
│   │   ├── clock.rs
│   │   ├── hyprland.rs
│   │   ├── mpris.rs
│   │   ├── network.rs
│   │   ├── notifications.rs
│   │   ├── tray.rs
│   │   ├── volume.rs
│   │   └── weather.rs
│   ├── services.rs
│   ├── services/
│   │   ├── brightness.rs
│   │   ├── dbus.rs
│   │   ├── hyprland.rs
│   │   ├── network.rs
│   │   ├── niri.rs
│   │   ├── notifications.rs
│   │   └── wireplumber.rs
│   ├── utils.rs
│   ├── utils/
│   │   ├── icons.rs
│   ├── style.rs
│   └── style/
│       └── theme.rs
├── resources/
│   └── style.css
└── build.rs
```

#### 1.2 Core Dependencies (Cargo.toml)

```toml
[package]
name = "cadenza-shell"
version = "0.1.0"
edition = "2021"

[dependencies]
# GTK and UI
gtk4 = { version = "0.9", features = ["v4_14"] }
gtk4-layer-shell = "0.4"
glib = "0.20"
gio = "0.20"
gdk4 = { version = "0.9", features = ["v4_14"] }
pango = "0.20"

# Async and concurrency
tokio = { version = "1.40", features = ["full"] }
async-channel = "2.3"
futures = "0.3"

# System integration
zbus = { version = "4.0", features = ["tokio"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Utilities
anyhow = "1.0"
thiserror = "1.0"
log = "0.4"
env_logger = "0.11"
once_cell = "1.19"
chrono = "0.4"

# System monitoring
sysinfo = "0.31"
procfs = "0.16"

# Network management
nm-rs = { version = "0.1", optional = true }

# Audio
pipewire = { version = "0.8", optional = true }
wireplumber = { version = "0.2", optional = true }

[build-dependencies]
glib-build-tools = "0.20"

[features]
default = ["networkmanager", "audio"]
networkmanager = ["nm-rs"]
audio = ["pipewire", "wireplumber"]
```

### Phase 2: Core Infrastructure Implementation

#### 2.1 Reactive State Management System

Create a custom reactive state system that mimics AGS's `createBinding` and
`createState`:

```rust
// src/utils/reactive.rs
use std::sync::{Arc, Mutex};
use async_channel::{Sender, Receiver};
use glib::clone;

pub struct Binding<T: Clone + Send + 'static> {
    value: Arc<Mutex<T>>,
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T: Clone + Send + 'static> Binding<T> {
    pub fn new(initial: T) -> Self {
        let (sender, receiver) = async_channel::unbounded();
        Self {
            value: Arc::new(Mutex::new(initial)),
            sender,
            receiver,
        }
    }

    pub fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }

    pub fn set(&self, value: T) {
        *self.value.lock().unwrap() = value.clone();
        let _ = self.sender.send_blocking(value);
    }

    pub fn subscribe<F>(&self, callback: F) -> glib::SignalHandlerId
    where
        F: Fn(T) + 'static,
    {
        let receiver = self.receiver.clone();
        glib::spawn_future_local(async move {
            while let Ok(value) = receiver.recv().await {
                callback(value);
            }
        });
        // Return a dummy handler for API compatibility
        glib::SignalHandlerId::new(0)
    }

    pub fn map<U, F>(&self, mapper: F) -> Binding<U>
    where
        U: Clone + Send + 'static,
        F: Fn(&T) -> U + Send + 'static,
    {
        let mapped = Binding::new(mapper(&self.get()));
        self.subscribe(move |value| {
            mapped.set(mapper(&value));
        });
        mapped
    }
}
```

#### 2.2 Application Foundation

```rust
// src/app/application.rs
use gtk4::{prelude::*, Application, ApplicationWindow};
use gtk4_layer_shell::{LayerShell, Layer, Edge};
use gdk4::Display;

pub struct CadenzaShell {
    app: Application,
    bars: Vec<ApplicationWindow>,
}

impl CadenzaShell {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("com.cadenza.shell")
            .build();

        Self {
            app,
            bars: Vec::new(),
        }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.connect_activate(clone!(@weak self as this => move |app| {
            this.setup_ui(app);
        }));

        self.app.run()
    }

    fn setup_ui(&mut self, app: &Application) {
        let display = Display::default().expect("Could not get default display");
        let monitors = display.monitors();

        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            self.create_bar(app, &monitor);
        }

        self.setup_notifications();
    }

    fn create_bar(&mut self, app: &Application, monitor: &gdk4::Monitor) {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Cadenza Shell Bar")
            .build();

        // Configure layer shell
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_exclusive_zone(32);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_monitor(monitor);

        // Create bar content
        let bar = crate::widgets::bar::Bar::new(monitor);
        window.set_child(Some(&bar.widget()));

        window.present();
        self.bars.push(window);
    }
}
```

### Phase 3: Service Layer Implementation

#### 3.1 Brightness Service (Custom Implementation)

Migrate the custom brightness service from TypeScript to Rust:

```rust
// src/services/brightness.rs
use std::fs;
use std::path::Path;
use anyhow::Result;
use crate::utils::reactive::Binding;

pub struct BrightnessService {
    available: bool,
    interface: String,
    min: u32,
    max: u32,
    current: Binding<f64>,
}

impl BrightnessService {
    pub fn new() -> Result<Self> {
        let interface = Self::detect_interface()?;
        let min = Self::read_min_brightness()?;
        let max = Self::read_max_brightness()?;
        let current_raw = Self::read_current_brightness(&interface)?;
        let current_normalized = (current_raw - min) as f64 / (max - min) as f64;

        let service = Self {
            available: true,
            interface,
            min,
            max,
            current: Binding::new(current_normalized),
        };

        service.start_monitoring();
        Ok(service)
    }

    fn detect_interface() -> Result<String> {
        let backlight_path = Path::new("/sys/class/backlight");
        let entries = fs::read_dir(backlight_path)?;
        
        for entry in entries {
            let entry = entry?;
            return Ok(entry.file_name().to_string_lossy().to_string());
        }
        
        anyhow::bail!("No backlight interface found")
    }

    fn read_current_brightness(interface: &str) -> Result<u32> {
        let path = format!("/sys/class/backlight/{}/brightness", interface);
        let content = fs::read_to_string(path)?;
        Ok(content.trim().parse()?)
    }

    pub fn set_brightness(&self, percent: f64) -> Result<()> {
        let raw_value = self.min + ((self.max - self.min) as f64 * percent) as u32;
        let clamped = raw_value.clamp(self.min, self.max);
        
        // Use pkexec or similar for privileged write
        std::process::Command::new("brillo")
            .args(&["-Sr", &clamped.to_string()])
            .output()?;
        
        self.current.set(percent);
        Ok(())
    }

    fn start_monitoring(&self) {
        // Use inotify or polling to monitor brightness changes
        let interface = self.interface.clone();
        let current = self.current.clone();
        let min = self.min;
        let max = self.max;

        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if let Ok(raw) = Self::read_current_brightness(&interface) {
                let normalized = (raw - min) as f64 / (max - min) as f64;
                current.set(normalized);
            }
            glib::ControlFlow::Continue
        });
    }
}
```

#### 3.2 D-Bus Services Integration

```rust
// src/services/dbus.rs
use zbus::{Connection, proxy};
use serde::{Deserialize, Serialize};

// NetworkManager proxy
#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    async fn get_devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
    async fn get_primary_connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
    #[zbus(property)]
    async fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    async fn connectivity(&self) -> zbus::Result<u32>;
}

// WirePlumber/PipeWire integration
pub struct AudioService {
    connection: Connection,
    volume: Binding<f64>,
    muted: Binding<bool>,
}

impl AudioService {
    pub async fn new() -> Result<Self> {
        let connection = Connection::session().await?;
        // Initialize WirePlumber connection
        // Set up volume and mute bindings
        Ok(Self {
            connection,
            volume: Binding::new(0.5),
            muted: Binding::new(false),
        })
    }
}

// Notification daemon
#[derive(Debug, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub icon: String,
    pub urgency: u8,
    pub timeout: i32,
}

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    async fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<&str>,
        hints: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}
```

#### 3.3 Hyprland IPC Service

```rust
// src/services/hyprland.rs
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use crate::utils::reactive::Binding;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i32,
    pub name: String,
    pub monitor: String,
    pub windows: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub address: String,
    pub title: String,
    pub class: String,
    pub workspace: i32,
}

pub struct HyprlandService {
    socket_path: String,
    workspaces: Binding<Vec<Workspace>>,
    focused_client: Binding<Option<Client>>,
    focused_workspace: Binding<Option<Workspace>>,
}

impl HyprlandService {
    pub async fn new() -> Result<Self> {
        let socket_path = format!(
            "/tmp/hypr/{}/.socket.sock",
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?
        );

        let service = Self {
            socket_path,
            workspaces: Binding::new(Vec::new()),
            focused_client: Binding::new(None),
            focused_workspace: Binding::new(None),
        };

        service.start_event_listener().await?;
        service.refresh_state().await?;
        
        Ok(service)
    }

    async fn send_command(&self, command: &str) -> Result<String> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;
        stream.write_all(command.as_bytes()).await?;
        
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        
        Ok(response)
    }

    pub async fn focus_workspace(&self, id: i32) -> Result<()> {
        self.send_command(&format!("dispatch workspace {}", id)).await?;
        Ok(())
    }

    async fn start_event_listener(&self) -> Result<()> {
        let event_socket = format!(
            "/tmp/hypr/{}/.socket2.sock",
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE")?
        );

        tokio::spawn(async move {
            let mut stream = UnixStream::connect(&event_socket).await?;
            let mut buffer = vec![0; 4096];

            loop {
                let n = stream.read(&mut buffer).await?;
                let event = String::from_utf8_lossy(&buffer[..n]);
                
                // Parse and handle events
                if event.starts_with("workspace>>") {
                    // Update workspace state
                } else if event.starts_with("activewindow>>") {
                    // Update active window state
                }
            }
        });

        Ok(())
    }
}
```

### Phase 4: Widget Implementation

#### 4.1 Bar Widget

```rust
// src/widgets/bar.rs
use gtk4::prelude::*;
use gtk4::{Box, Orientation};
use gdk4::Monitor;

pub struct Bar {
    container: Box,
    monitor: Monitor,
}

impl Bar {
    pub fn new(monitor: &Monitor) -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["bar"])
            .height_request(32)
            .build();

        let left = Self::create_left_section(monitor);
        let center = Self::create_center_section();
        let right = Self::create_right_section();

        container.append(&left);
        container.set_center_widget(Some(&center));
        container.append(&right);

        Self {
            container,
            monitor: monitor.clone(),
        }
    }

    fn create_left_section(monitor: &Monitor) -> Box {
        let section = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(20)
            .css_classes(vec!["workspaces"])
            .build();

        let workspaces = crate::tiles::hyprland::Workspaces::new(monitor);
        let focused_client = crate::tiles::hyprland::FocusedClient::new(monitor);

        section.append(&workspaces.widget());
        section.append(&focused_client.widget());

        section
    }

    fn create_center_section() -> Box {
        let section = Box::builder()
            .orientation(Orientation::Horizontal)
            .halign(gtk4::Align::Start)
            .build();

        let clock = crate::tiles::clock::Clock::new();
        let weather = crate::tiles::weather::Weather::new();
        let media = crate::tiles::mpris::Media::new();

        section.append(&clock.widget());
        section.append(&weather.widget());
        section.append(&media.widget());

        section
    }

    fn create_right_section() -> Box {
        let section = Box::builder()
            .orientation(Orientation::Horizontal)
            .build();

        let brightness = crate::tiles::brightness::Brightness::new();
        let volume = crate::tiles::volume::Volume::new();
        let bluetooth = crate::tiles::bluetooth::Bluetooth::new();
        let network = crate::tiles::network::Network::new();
        let battery = crate::tiles::battery::Battery::new();
        let tray = crate::tiles::tray::SysTray::new();
        let notifications = crate::tiles::notifications::NotificationTile::new();

        section.append(&brightness.widget());
        section.append(&volume.widget());
        section.append(&bluetooth.widget());
        section.append(&network.widget());
        section.append(&battery.widget());
        section.append(&tray.widget());
        section.append(&notifications.widget());

        section
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}
```

#### 4.2 Tile Widgets

```rust
// src/tiles/brightness.rs
use gtk4::prelude::*;
use gtk4::{Box, Label, ProgressBar, Orientation};
use crate::services::brightness::BrightnessService;
use crate::utils::icons::BRIGHTNESS_ICONS;

pub struct Brightness {
    container: Box,
    service: Arc<BrightnessService>,
}

impl Brightness {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = Arc::new(BrightnessService::new().unwrap_or_else(|_| {
            // Return a dummy service if brightness is not available
            BrightnessService::unavailable()
        }));

        if service.is_available() {
            let icon_label = Label::builder()
                .css_classes(vec!["icon", "dim"])
                .width_request(16)
                .build();

            let progress_bar = ProgressBar::builder()
                .css_classes(vec!["dim"])
                .valign(gtk4::Align::Center)
                .width_request(16)
                .build();

            // Set up reactive bindings
            let icon_label_clone = icon_label.clone();
            let progress_bar_clone = progress_bar.clone();
            
            service.current.subscribe(move |value| {
                let icon_index = (value * BRIGHTNESS_ICONS.len() as f64) as usize;
                let icon = BRIGHTNESS_ICONS[icon_index.min(BRIGHTNESS_ICONS.len() - 1)];
                icon_label_clone.set_text(icon);
                progress_bar_clone.set_fraction(value);
                
                // Add fade effect
                icon_label_clone.add_css_class("bright");
                progress_bar_clone.add_css_class("bright");
                
                glib::timeout_add_local(std::time::Duration::from_secs(3), {
                    let icon_label = icon_label_clone.clone();
                    let progress_bar = progress_bar_clone.clone();
                    move || {
                        icon_label.remove_css_class("bright");
                        icon_label.add_css_class("dim");
                        progress_bar.remove_css_class("bright");
                        progress_bar.add_css_class("dim");
                        glib::ControlFlow::Break
                    }
                });
            });

            container.append(&icon_label);
            container.append(&progress_bar);
        }

        Self { container, service }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}
```

### Phase 5: Style and Theming

#### 5.1 CSS Integration

We want to be able to use SCSS, so let's compile SCSS to CSS at build-time or
run-time.

```rust
// src/style/theme.rs
use gtk4::prelude::*;
use gtk4::CssProvider;
use gdk4::Display;

pub fn load_css() {
    let provider = CssProvider::new();
    let css = compile_css(); // convert scss to css
    provider.load_from_string(CSS);

    gtk4::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn compile_css() -> Result<String, SomeError> {
    // figure out how to implement this
}
```

### Phase 6: Build and Deployment

#### 6.1 Build Script

```rust
// build.rs
fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/resources.gresource.xml",
        "compiled.gresource",
    );

    // we should also compile our SCSS here
}
```

#### 6.2 Nix Integration

Migrate flake.nix to use flake-parts and rust-flake:
https://flake.parts/options/rust-flake.html.

## Migration Steps

### Step 1: Initial Setup

1. Create new Rust project structure
1. Set up Cargo.toml with dependencies
1. Implement basic GTK4 application with layer shell
1. Create simple bar window on all monitors

### Step 2: Core Infrastructure

1. Implement reactive state management system
1. Create service abstraction layer
1. Set up D-Bus connection infrastructure
1. Implement CSS theming system

### Step 3: Service Migration

1. Migrate brightness service
1. Implement NetworkManager integration
1. Add WirePlumber/PipeWire audio service
1. Create Hyprland IPC client
1. Implement notification daemon client
1. Add system tray support

### Step 4: Widget Implementation

1. Create tile widget base class
1. Implement all tile widgets (battery, bluetooth, etc.)
1. Create notification popup windows
1. Implement notification center
1. Add weather widget with API integration
1. Create MPRIS media player widget

### Step 5: Polish and Testing

1. Implement all animations and transitions
1. Add configuration file support
1. Create comprehensive error handling
1. Performance optimization
1. Memory leak testing
1. Multi-monitor testing

### Step 6: Deployment

1. Create installation scripts
1. Write user documentation
1. Set up CI/CD pipeline
1. Create Nix package
1. Final testing and bug fixes

## Key Considerations

### Performance Optimizations

- Use `Arc<Mutex<>>` sparingly, prefer channels for communication
- Implement lazy loading for widgets
- Use GTK4's built-in caching mechanisms
- Profile and optimize render paths

### Error Handling

- Graceful degradation when services unavailable
- Comprehensive logging with `log` crate
- User-friendly error messages
- Automatic recovery mechanisms

### Testing Strategy

- Unit tests for services and utilities
- Integration tests for D-Bus interactions
- UI tests using GTK4 testing framework
- Manual testing on multiple window managers

### Documentation

- Inline documentation for all public APIs
- Architecture documentation
- User configuration guide
- Developer contribution guide

## Conclusion

This migration plan provides a structured approach to converting the Cadenza
Shell from TypeScript/AGS to Rust/gtk-rs. The new implementation will maintain
feature parity while providing improved performance, type safety, and
maintainability. The modular architecture allows for incremental migration and
testing, reducing risk and ensuring a smooth transition.

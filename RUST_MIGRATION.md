# Muse Shell Migration Plan: TypeScript/AGS to Rust/gtk-rs

## Executive Summary

This document outlines a comprehensive plan to migrate the Muse Shell desktop
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
- **State Management**: GTK4's native property bindings, GObject signals, and
  glib::Properties

## Migration Strategy

You may consult https://gtk-rs.org/gtk4-rs/stable/latest/book/ for guidance on
developing GTK GUIs in Rust.

### Phase 1: Project Setup and Foundation

#### 1.1 Initialize Rust Project Structure

```
muse-shell/
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
name = "muse-shell"
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

#### 2.1 Native GTK State Management

Instead of creating a custom reactive system, we'll leverage GTK4's native
property bindings and GObject signals for state management. This approach is
more idiomatic and integrates better with the GTK ecosystem.

```rust
// Example using glib::Properties for reactive state
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::RefCell;

// Define a GObject with reactive properties
mod imp {
    use super::*;
    
    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::BrightnessModel)]
    pub struct BrightnessModel {
        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        brightness: RefCell<f64>,
        
        #[property(get, set)]
        available: RefCell<bool>,
    }
    
    #[glib::object_subclass]
    impl ObjectSubclass for BrightnessModel {
        const NAME: &'static str = "MuseShellBrightnessModel";
        type Type = super::BrightnessModel;
        type ParentType = glib::Object;
    }
    
    #[glib::derived_properties]
    impl ObjectImpl for BrightnessModel {
        fn constructed(&self) {
            self.parent_constructed();
            
            // Start monitoring brightness changes
            glib::timeout_add_local(std::time::Duration::from_millis(500), {
                let obj = self.obj().clone();
                move || {
                    // Update brightness from system
                    if let Ok(brightness) = read_system_brightness() {
                        obj.set_brightness(brightness);
                    }
                    glib::ControlFlow::Continue
                }
            });
        }
    }
}

glib::wrapper! {
    pub struct BrightnessModel(ObjectSubclass<imp::BrightnessModel>);
}

impl BrightnessModel {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

// Using property bindings in widgets
impl BrightnessWidget {
    pub fn new(model: &BrightnessModel) -> Self {
        let progress_bar = gtk4::ProgressBar::new();
        
        // Bind model property to widget property
        model.bind_property("brightness", &progress_bar, "fraction")
            .sync_create()
            .build();
        
        // Connect to property change notifications
        model.connect_brightness_notify(|model| {
            println!("Brightness changed to: {}", model.brightness());
        });
        
        Self { progress_bar }
    }
}
```

For simpler cases where full GObject subclassing isn't needed, we can use GTK's
expression and binding APIs:

```rust
// Using gtk4::Expression for computed values
use gtk4::{Expression, PropertyExpression};

pub fn create_brightness_tile(brightness_value: &glib::Value) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let icon_label = gtk4::Label::new(None);
    let progress_bar = gtk4::ProgressBar::new();
    
    // Create an expression that maps brightness to icon
    let icon_expression = PropertyExpression::new(
        glib::Value::static_type(),
        None::<&Expression>,
        "value",
    ).chain_closure::<String>(closure!(|_: Option<glib::Object>, brightness: f64| {
        let icons = ["󰃞", "󰃟", "󰃠"];
        let index = (brightness * icons.len() as f64) as usize;
        icons[index.min(icons.len() - 1)].to_string()
    }));
    
    // Bind the expression to the label
    icon_expression.bind(&icon_label, "label", None::<&glib::Object>);
    
    container.append(&icon_label);
    container.append(&progress_bar);
    container
}
```

#### 2.2 Application Foundation

```rust
// src/app/application.rs
use gtk4::{prelude::*, Application, ApplicationWindow};
use gtk4_layer_shell::{LayerShell, Layer, Edge};
use gdk4::Display;

pub struct MuseShell {
    app: Application,
    bars: Vec<ApplicationWindow>,
}

impl MuseShell {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("com.muse.shell")
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
            .title("Muse Shell Bar")
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

#### 3.1 Brightness Service (Native GObject Implementation)

Migrate the custom brightness service using GObject subclassing for native GTK
integration:

```rust
// src/services/brightness.rs
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::Path;
use anyhow::Result;

mod imp {
    use super::*;
    
    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::BrightnessService)]
    pub struct BrightnessService {
        #[property(get, set)]
        available: Cell<bool>,
        
        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        brightness: Cell<f64>,
        
        interface: RefCell<String>,
        min: Cell<u32>,
        max: Cell<u32>,
    }
    
    #[glib::object_subclass]
    impl ObjectSubclass for BrightnessService {
        const NAME: &'static str = "MuseShellBrightnessService";
        type Type = super::BrightnessService;
        type ParentType = glib::Object;
    }
    
    #[glib::derived_properties]
    impl ObjectImpl for BrightnessService {
        fn constructed(&self) {
            self.parent_constructed();
            
            // Initialize brightness monitoring
            if let Ok(interface) = self.detect_interface() {
                self.interface.replace(interface.clone());
                
                if let Ok((min, max)) = self.read_brightness_range() {
                    self.min.set(min);
                    self.max.set(max);
                    self.available.set(true);
                    
                    // Start monitoring
                    self.start_monitoring();
                }
            }
        }
    }
    
    impl BrightnessService {
        fn detect_interface(&self) -> Result<String> {
            let backlight_path = Path::new("/sys/class/backlight");
            let entries = fs::read_dir(backlight_path)?;
            
            for entry in entries {
                let entry = entry?;
                return Ok(entry.file_name().to_string_lossy().to_string());
            }
            
            anyhow::bail!("No backlight interface found")
        }
        
        fn read_brightness_range(&self) -> Result<(u32, u32)> {
            // Read min/max brightness using brillo or direct sysfs access
            let min = std::process::Command::new("brillo")
                .args(&["-rc"])
                .output()?;
            let max = std::process::Command::new("brillo")
                .args(&["-rm"])
                .output()?;
                
            Ok((
                String::from_utf8_lossy(&min.stdout).trim().parse()?,
                String::from_utf8_lossy(&max.stdout).trim().parse()?
            ))
        }
        
        fn start_monitoring(&self) {
            let obj = self.obj().clone();
            
            glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                if let Ok(brightness) = obj.imp().read_current_brightness() {
                    obj.set_brightness(brightness);
                }
                glib::ControlFlow::Continue
            });
        }
        
        fn read_current_brightness(&self) -> Result<f64> {
            let interface = self.interface.borrow();
            let path = format!("/sys/class/backlight/{}/brightness", interface);
            let content = fs::read_to_string(path)?;
            let raw: u32 = content.trim().parse()?;
            
            let min = self.min.get();
            let max = self.max.get();
            Ok((raw - min) as f64 / (max - min) as f64)
        }
    }
}

glib::wrapper! {
    pub struct BrightnessService(ObjectSubclass<imp::BrightnessService>);
}

impl BrightnessService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
    
    pub fn set_brightness_value(&self, percent: f64) -> Result<()> {
        let imp = self.imp();
        let min = imp.min.get();
        let max = imp.max.get();
        let raw_value = min + ((max - min) as f64 * percent) as u32;
        let clamped = raw_value.clamp(min, max);
        
        // Use brillo for privileged write
        std::process::Command::new("brillo")
            .args(&["-Sr", &clamped.to_string()])
            .output()?;
        
        // The property will be updated by the monitoring loop
        Ok(())
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

// WirePlumber/PipeWire integration using GObject
mod audio_service {
    use super::*;
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::Cell;
    
    mod imp {
        use super::*;
        
        #[derive(Properties, Default)]
        #[properties(wrapper_type = super::AudioService)]
        pub struct AudioService {
            #[property(get, set, minimum = 0.0, maximum = 1.0)]
            volume: Cell<f64>,
            
            #[property(get, set)]
            muted: Cell<bool>,
            
            connection: RefCell<Option<Connection>>,
        }
        
        #[glib::object_subclass]
        impl ObjectSubclass for AudioService {
            const NAME: &'static str = "MuseShellAudioService";
            type Type = super::AudioService;
            type ParentType = glib::Object;
        }
        
        #[glib::derived_properties]
        impl ObjectImpl for AudioService {
            fn constructed(&self) {
                self.parent_constructed();
                
                // Initialize WirePlumber connection asynchronously
                let obj = self.obj().clone();
                glib::spawn_future_local(async move {
                    if let Ok(conn) = Connection::session().await {
                        obj.imp().connection.replace(Some(conn));
                        obj.imp().start_monitoring().await;
                    }
                });
            }
        }
        
        impl AudioService {
            async fn start_monitoring(&self) {
                // Set up D-Bus signal monitoring for volume/mute changes
                // Update properties when changes occur
            }
        }
    }
    
    glib::wrapper! {
        pub struct AudioService(ObjectSubclass<imp::AudioService>);
    }
    
    impl AudioService {
        pub fn new() -> Self {
            glib::Object::builder().build()
        }
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

#### 4.2 Tile Widgets (Using Native Property Bindings)

```rust
// src/tiles/brightness.rs
use gtk4::prelude::*;
use gtk4::{Box, Label, ProgressBar, Orientation};
use gtk4::glib;
use crate::services::brightness::BrightnessService;

const BRIGHTNESS_ICONS: &[&str] = &["󰃞", "󰃟", "󰃠", "󰃡", "󰃢", "󰃣"];

pub struct BrightnessWidget {
    container: Box,
    service: BrightnessService,
}

impl BrightnessWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = BrightnessService::new();

        // Only show widget if brightness is available
        service.bind_property("available", &container, "visible")
            .sync_create()
            .build();

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

            // Bind brightness value directly to progress bar
            service.bind_property("brightness", &progress_bar, "fraction")
                .sync_create()
                .build();

            // Update icon based on brightness changes
            service.connect_brightness_notify(glib::clone!(@weak icon_label => move |service| {
                let brightness = service.brightness();
                let icon_index = (brightness * BRIGHTNESS_ICONS.len() as f64) as usize;
                let icon = BRIGHTNESS_ICONS[icon_index.min(BRIGHTNESS_ICONS.len() - 1)];
                icon_label.set_text(icon);
                
                // Trigger fade animation
                icon_label.remove_css_class("dim");
                icon_label.add_css_class("bright");
                
                glib::timeout_add_local_once(std::time::Duration::from_secs(3), 
                    glib::clone!(@weak icon_label => move || {
                        icon_label.remove_css_class("bright");
                        icon_label.add_css_class("dim");
                    })
                );
            }));

            // Similar animation for progress bar
            service.connect_brightness_notify(glib::clone!(@weak progress_bar => move |_| {
                progress_bar.remove_css_class("dim");
                progress_bar.add_css_class("bright");
                
                glib::timeout_add_local_once(std::time::Duration::from_secs(3),
                    glib::clone!(@weak progress_bar => move || {
                        progress_bar.remove_css_class("bright");
                        progress_bar.add_css_class("dim");
                    })
                );
            }));

            container.append(&icon_label);
            container.append(&progress_bar);
        }

        Self { container, service }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}

// Volume widget using native bindings
// src/tiles/volume.rs
use gtk4::prelude::*;
use gtk4::{Box, Label, ProgressBar, Orientation};
use crate::services::audio::AudioService;

const VOLUME_ICONS: &[&str] = &["󰕿", "󰖀", "󰕾"];
const MUTE_ICON: &str = "󰖁";

pub struct VolumeWidget {
    container: Box,
    service: AudioService,
}

impl VolumeWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = AudioService::new();
        
        let icon_label = Label::builder()
            .css_classes(vec!["icon", "dim"])
            .width_request(16)
            .build();

        let progress_bar = ProgressBar::builder()
            .css_classes(vec!["dim"])
            .valign(gtk4::Align::Center)
            .width_request(16)
            .build();

        // Bind volume to progress bar
        service.bind_property("volume", &progress_bar, "fraction")
            .sync_create()
            .build();

        // Create expression for icon that depends on both volume and mute
        let volume_expr = gtk4::PropertyExpression::new(
            AudioService::static_type(),
            None::<&gtk4::Expression>,
            "volume",
        );
        
        let muted_expr = gtk4::PropertyExpression::new(
            AudioService::static_type(),
            None::<&gtk4::Expression>,
            "muted",
        );

        // Update icon when either volume or mute changes
        let update_icon = glib::clone!(@weak icon_label, @weak service => move || {
            let icon = if service.is_muted() {
                MUTE_ICON
            } else {
                let volume = service.volume();
                let idx = (volume * VOLUME_ICONS.len() as f64) as usize;
                VOLUME_ICONS[idx.min(VOLUME_ICONS.len() - 1)]
            };
            icon_label.set_text(icon);
        });

        service.connect_volume_notify(glib::clone!(@strong update_icon => move |_| {
            update_icon();
        }));
        
        service.connect_muted_notify(glib::clone!(@strong update_icon => move |_| {
            update_icon();
        }));

        // Initial icon update
        update_icon();

        container.append(&icon_label);
        container.append(&progress_bar);

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

### Native GTK Integration Benefits

- **Property Bindings**: Use GTK4's native property binding system instead of
  custom reactive state
- **GObject Signals**: Leverage built-in signal/slot mechanism for event
  handling
- **Expression API**: Use GTK4's expression API for computed values and
  transformations
- **Type Safety**: GObject properties provide compile-time type checking with
  the `glib::Properties` derive macro

### Performance Optimizations

- Leverage GTK4's built-in property caching and lazy evaluation
- Use `glib::clone!` macro for efficient weak reference handling in closures
- Implement lazy loading for widgets using GTK4's visibility properties
- Profile and optimize render paths using GTK Inspector

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

## Advantages of Native GTK Approach

### Why Native GTK State Management?

1. **Better Integration**: Native GObject properties integrate seamlessly with
   GTK Inspector for debugging
1. **Automatic Memory Management**: GTK's reference counting handles memory
   automatically
1. **Built-in Animations**: Property transitions can use GTK's animation
   framework
1. **Standardized Patterns**: Following GTK conventions makes the code more
   maintainable
1. **Performance**: Native bindings avoid overhead of custom reactive systems
1. **Ecosystem Compatibility**: Works naturally with other GTK/GNOME libraries

### Example: Reactive Updates with Native Bindings

```rust
// Traditional imperative approach
button.connect_clicked(move |_| {
    label.set_text("Clicked!");
});

// Native property binding approach
model.bind_property("status", &label, "label")
    .transform_to(|_, status: &str| Some(format!("Status: {}", status)))
    .sync_create()
    .build();

// Expression-based computed values
let expr = gtk4::ClosureExpression::new::<String>(
    &[volume_expr, muted_expr],
    closure!(|_: Option<glib::Object>, volume: f64, muted: bool| {
        if muted { "Muted" } else { format!("Volume: {:.0}%", volume * 100.0) }
    }),
);
expr.bind(&label, "label", None::<&glib::Object>);
```

## Conclusion

This migration plan provides a structured approach to converting the Muse Shell
from TypeScript/AGS to Rust/gtk-rs using native GTK patterns. The new
implementation will maintain feature parity while providing improved
performance, type safety, and maintainability through idiomatic Rust and GTK4
practices. The use of native GObject properties and signals ensures better
integration with the GTK ecosystem and provides a more maintainable codebase
that follows established patterns.

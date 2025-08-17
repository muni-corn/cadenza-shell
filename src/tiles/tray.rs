use anyhow::Result;
use futures_lite::stream::StreamExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Button, Image, MenuButton, Orientation, Revealer, RevealerTransitionType};
use std::cell::RefCell;
use std::collections::HashMap;
use zbus::{Connection, proxy, Result as ZbusResult};
use futures_util::stream::StreamExt;

#[derive(Debug, Clone)]
pub struct TrayItem {
    pub id: String,
    pub title: String,
    pub tooltip: String,
    pub icon_name: String,
    pub icon_pixmap: Option<Vec<u8>>,
    pub menu: Option<gio::MenuModel>,
}

#[proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
trait StatusNotifierWatcher {
    fn register_status_notifier_item(&self, service: &str) -> ZbusResult<()>;

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> ZbusResult<Vec<String>>;

    #[zbus(signal)]
    fn status_notifier_item_registered(&self, service: String) -> ZbusResult<()>;

    #[zbus(signal)]
    fn status_notifier_item_unregistered(&self, service: String) -> ZbusResult<()>;
}

#[proxy(interface = "org.kde.StatusNotifierItem")]
trait StatusNotifierItem {
    #[zbus(property)]
    fn id(&self) -> ZbusResult<String>;

    #[zbus(property)]
    fn title(&self) -> ZbusResult<String>;

    #[zbus(property)]
    fn tooltip_title(&self) -> ZbusResult<String>;

    #[zbus(property)]
    fn icon_name(&self) -> ZbusResult<String>;

    #[zbus(property)]
    fn icon_pixmap(&self) -> ZbusResult<Vec<(i32, i32, Vec<u8>)>>;

    #[zbus(property)]
    fn menu(&self) -> ZbusResult<zbus::zvariant::OwnedObjectPath>;

    fn activate(&self, x: i32, y: i32) -> ZbusResult<()>;

    fn secondary_activate(&self, x: i32, y: i32) -> ZbusResult<()>;

    fn context_menu(&self, x: i32, y: i32) -> ZbusResult<()>;
}

pub struct SysTray {
    container: Box,
    revealer: Revealer,
    toggle_button: Button,
    items_container: Box,
    items: RefCell<HashMap<String, TrayItem>>,
    expanded: RefCell<bool>,
}

impl SysTray {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(0)
            .build();

        let items_container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(4)
            .build();

        let revealer = Revealer::builder()
            .child(&items_container)
            .reveal_child(false)
            .transition_type(RevealerTransitionType::SlideLeft)
            .transition_duration(200)
            .build();

        let toggle_button = Button::builder()
            .icon_name("pan-start-symbolic")
            .css_classes(vec!["tile"])
            .width_request(32)
            .height_request(32)
            .build();

        container.append(&revealer);
        container.append(&toggle_button);

        let tray = Self {
            container,
            revealer,
            toggle_button,
            items_container,
            items: RefCell::new(HashMap::new()),
            expanded: RefCell::new(false),
        };

        tray.setup_toggle_button();
        tray.initialize_tray();

        tray
    }

    fn setup_toggle_button(&self) {
        let revealer = self.revealer.clone();
        let button = self.toggle_button.clone();
        let expanded = self.expanded.clone();

        self.toggle_button.connect_clicked(move |_| {
            let is_expanded = *expanded.borrow();
            let new_expanded = !is_expanded;

            revealer.set_reveal_child(new_expanded);
            button.set_icon_name(if new_expanded {
                "pan-end-symbolic"
            } else {
                "pan-start-symbolic"
            });

            *expanded.borrow_mut() = new_expanded;
        });
    }

    fn initialize_tray(&self) {
        let items_container = self.items_container.clone();
        let items = self.items.clone();

        glib::spawn_future_local(async move {
            if let Err(e) = Self::setup_status_notifier_watcher(items_container, items).await {
                log::warn!("Failed to initialize system tray: {}", e);
            }
        });
    }

    async fn setup_status_notifier_watcher(
        items_container: Box,
        items: RefCell<HashMap<String, TrayItem>>,
    ) -> Result<()> {
        let connection = Connection::session().await?;

        // Try to get the status notifier watcher
        let watcher = match StatusNotifierWatcherProxy::new(&connection).await {
            Ok(watcher) => watcher,
            Err(_) => {
                log::info!("No system tray available (StatusNotifierWatcher not found)");
                return Ok(());
            }
        };

        // Get existing items
        if let Ok(registered_items) = watcher.registered_status_notifier_items().await {
            for item_service in registered_items {
                Self::add_tray_item(&connection, &items_container, &items, &item_service).await;
            }
        }

        // Listen for new items
        let mut item_registered_stream = watcher.receive_status_notifier_item_registered().await?;
        let items_container_clone = items_container.clone();
        let items_clone = items.clone();
        let connection_clone = connection.clone();

        glib::spawn_future_local(async move {
            while let Some(signal) = item_registered_stream.next().await {
                if let Ok(args) = signal.args() {
                    Self::add_tray_item(
                        &connection_clone,
                        &items_container_clone,
                        &items_clone,
                        &args.service,
                    )
                    .await;
                }
            }
        });

        // Listen for removed items
        let mut item_unregistered_stream =
            watcher.receive_status_notifier_item_unregistered().await?;
        glib::spawn_future_local(async move {
            while let Some(signal) = item_unregistered_stream.next().await {
                if let Ok(args) = signal.args() {
                    Self::remove_tray_item(&items_container, &items, &args.service);
                }
            }
        });

        Ok(())
    }

    async fn add_tray_item(
        connection: &Connection,
        items_container: &Box,
        items: &RefCell<HashMap<String, TrayItem>>,
        service: &str,
    ) {
        // Parse service name to get bus name and object path
        let (bus_name, object_path) = if service.contains('/') {
            let parts: Vec<&str> = service.splitn(2, '/').collect();
            (parts[0], format!("/{}", parts[1]))
        } else {
            (service, "/StatusNotifierItem".to_string())
        };

        // Create proxy for the status notifier item
        let item_proxy = match StatusNotifierItemProxy::builder(connection)
            .destination(bus_name)
            .unwrap()
            .path(object_path)
            .unwrap()
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(e) => {
                log::warn!("Failed to create proxy for tray item {}: {}", service, e);
                return;
            }
        };

        // Get item properties
        let id = item_proxy.id().await.unwrap_or_default();
        let title = item_proxy.title().await.unwrap_or_default();
        let tooltip = item_proxy.tooltip_title().await.unwrap_or_default();
        let icon_name = item_proxy.icon_name().await.unwrap_or_default();

        let tray_item = TrayItem {
            id: id.clone(),
            title,
            tooltip: tooltip.clone(),
            icon_name: icon_name.clone(),
            icon_pixmap: None, // TODO: Handle pixmap icons
            menu: None,        // TODO: Handle menus
        };

        // Create widget for the tray item
        let menu_button = MenuButton::builder()
            .css_classes(vec!["bar-button"])
            .width_request(32)
            .height_request(32)
            .tooltip_text(&tooltip)
            .build();

        let image = if !icon_name.is_empty() {
            Image::from_icon_name(&icon_name)
        } else {
            Image::from_icon_name("application-x-executable")
        };

        image.set_pixel_size(16);
        menu_button.set_child(Some(&image));

        // Handle clicks - use button instead of menu_button since MenuButton doesn't have connect_clicked
        let button = Button::new();
        button.set_child(Some(&image));
        button.set_css_classes(&["bar-button"]);
        button.set_width_request(32);
        button.set_height_request(32);
        button.set_tooltip_text(Some(&tooltip));

        let item_proxy_clone = item_proxy.clone();
        button.connect_clicked(move |button| {
            let allocation = button.allocation();
            let x = allocation.x() + allocation.width() / 2;
            let y = allocation.y() + allocation.height() / 2;
            
            let proxy = item_proxy_clone.clone();
            glib::spawn_future_local(async move {
                if let Err(e) = proxy.activate(x, y).await {
                    log::warn!("Failed to activate tray item: {}", e);
                }
            });
        });

        items_container.append(&button);
        items.borrow_mut().insert(service.to_string(), tray_item);
    }

    fn remove_tray_item(
        items_container: &Box,
        items: &RefCell<HashMap<String, TrayItem>>,
        service: &str,
    ) {
        items.borrow_mut().remove(service);

        // Find and remove the corresponding widget
        let mut child = items_container.first_child();
        while let Some(widget) = child {
            let next = widget.next_sibling();

            // TODO: Better way to identify which widget corresponds to which service
            // For now, we'll need to store widget references or use a different approach

            child = next;
        }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}

impl Default for SysTray {
    fn default() -> Self {
        Self::new()
    }
}
use gtk4::prelude::*;
use relm4::{
    factory::{FactoryComponent, FactoryView},
    prelude::*,
};

use crate::tray::status_notifier::item::{StatusNotifierItemProxyBlocking, StatusNotifierTooltip};

pub mod status_notifier;

pub type TrayStatus = status_notifier::item::StatusNotifierStatus;

#[derive(Debug, Clone, PartialEq)]
pub struct TrayPixmap {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>, // RGBA data
}

impl From<(i32, i32, Vec<u8>)> for TrayPixmap {
    fn from((width, height, data): (i32, i32, Vec<u8>)) -> Self {
        Self {
            width,
            height,
            data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayState {
    pub items: Vec<TrayItem>,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrayItem {
    index: <Self as FactoryComponent>::Index,
    pub id: String,
    pub title: String,
    pub icon_name: Option<String>,
    pub icon_pixmap: Option<TrayPixmap>,
    pub tooltip: Option<StatusNotifierTooltip>,
    pub menu: Option<String>,
    pub status: TrayStatus,
    pub service_name: String,
    pub object_path: String,
}

impl FactoryComponent for TrayItem {
    type CommandOutput = ();
    type Index = DynamicIndex;
    type Init = StatusNotifierItemProxyBlocking<'static>;
    type Input = ();
    type Output = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = ();

    fn init_root(&self) -> Self::Root {
        gtk::Button::builder().build()
    }

    fn init_model(init: Self::Init, index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        // For now, we'll use placeholder values for service_name and object_path
        // These will be set properly when the tray service creates the items
        Self {
            index: index.clone(),
            id: init.id().unwrap_or_default(),
            title: init.title().unwrap_or_default(),
            icon_name: init.icon_name().ok(),
            icon_pixmap: init.icon_pixmap().ok().map(Into::into),
            tooltip: init.tool_tip().ok(),
            menu: init.menu().ok(),
            status: init.status().unwrap_or_default(),
            service_name: String::new(), // Will be set by tray service
            object_path: String::new(),  // Will be set by tray service
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        // Set up the button styling
        root.add_css_class("bar-button");
        root.set_width_request(24);
        root.set_height_request(24);

        // Add status-specific CSS classes
        match self.status {
            TrayStatus::Active => root.add_css_class("tray-active"),
            TrayStatus::NeedsAttention => root.add_css_class("tray-needs-attention"),
            TrayStatus::Passive => {} // Default styling
        }

        // Create enhanced tooltip with more information
        let tooltip_text = if let Some(tooltip) = &self.tooltip {
            format!("{}\n{}", self.title, tooltip.description)
        } else {
            format!("{}\n{}", self.title, self.id)
        };
        root.set_tooltip_text(Some(&tooltip_text));

        // Create image or label for the button
        if let Some(icon_name) = &self.icon_name {
            let image = gtk::Image::from_icon_name(icon_name);
            image.set_pixel_size(16);
            image.set_halign(gtk::Align::Center);
            image.set_valign(gtk::Align::Center);
            root.set_child(Some(&image));
        } else if let Some(_pixmap) = &self.icon_pixmap {
            // TODO: Implement pixmap icon rendering in Phase 4
            // For now, fallback to text
            let label = gtk::Label::new(Some(&self.id.chars().take(2).collect::<String>()));
            root.set_child(Some(&label));
        } else {
            // Fallback to text
            let label = gtk::Label::new(Some(&self.id.chars().take(2).collect::<String>()));
            root.set_child(Some(&label));
        }

        // Connect click handlers for D-Bus actions (Phase 2)
        let item_id = self.id.clone();

        // Left click - activate
        root.connect_clicked(move |_button| {
            log::debug!("Tray item activated: {}", item_id);
            // TODO: This will be implemented in Phase 2
            log::info!("Would activate tray item {}", item_id);
        });
    }
}

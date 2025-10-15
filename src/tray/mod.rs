use gtk4::prelude::*;
use relm4::{factory::FactoryView, prelude::*};
pub(crate) use system_tray::client::{Client as TrayClient, Event as TrayEvent};
use system_tray::{
    client::ActivateRequest,
    item::{Status, StatusNotifierItem},
    menu::TrayMenu,
};

#[derive(Debug, Clone)]
pub struct TrayItem {
    address: String,
    data: StatusNotifierItem,
    menu: Option<TrayMenu>,
}

impl TrayItem {
    pub fn address(&self) -> &String {
        &self.address
    }
}

#[derive(Debug)]
pub enum TrayItemOutput {
    Activate(ActivateRequest),
    RequestMenu,
}

impl AsyncFactoryComponent for TrayItem {
    type CommandOutput = ();
    type Init = (String, StatusNotifierItem, Option<TrayMenu>);
    type Input = ();
    type Output = TrayItemOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Button::builder().build()
    }

    async fn init_model(
        (address, data, menu): Self::Init,
        _index: &DynamicIndex,
        _sender: AsyncFactorySender<Self>,
    ) -> Self {
        Self {
            address,
            data,
            menu,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as FactoryView>::ReturnedWidget,
        sender: AsyncFactorySender<Self>,
    ) -> Self::Widgets {
        // set up the button styling
        root.add_css_class("tray-item");
        root.set_width_request(24);
        root.set_height_request(24);

        // add status-specific CSS classes
        match self.data.status {
            Status::Active => root.add_css_class("tray-active"),
            Status::NeedsAttention => root.add_css_class("tray-needs-attention"),
            _ => {} // default styling
        }

        // Create enhanced tooltip with more information
        let tooltip_text = if let Some(tooltip) = &self.data.tool_tip {
            format!("{:?}\n{}", self.data.title, tooltip.description)
        } else {
            format!("{:?}\n{}", self.data.title, self.data.id)
        };
        root.set_tooltip_text(Some(&tooltip_text));

        // Create image or label for the button
        if let Some(icon_name) = &self.data.icon_name {
            let image = gtk::Image::from_icon_name(icon_name);
            image.set_pixel_size(16);
            image.set_halign(gtk::Align::Center);
            image.set_valign(gtk::Align::Center);
            root.set_child(Some(&image));
        } else if let Some(_pixmap) = &self.data.icon_pixmap {
            // TODO: Implement pixmap icon rendering in Phase 4
            // For now, fallback to text
            let label = gtk::Label::new(Some(&self.data.id.chars().take(2).collect::<String>()));
            root.set_child(Some(&label));
        } else {
            // Fallback to text
            let label = gtk::Label::new(Some(&self.data.id.chars().take(2).collect::<String>()));
            root.set_child(Some(&label));
        }

        // TODO: Left click - activate
        let address_clone = self.address.clone();
        let sender_clone = sender.clone();
        root.connect_clicked(move |_| {
            log::debug!("tray activate requested: {}", address_clone.clone());
            sender_clone
                .output(TrayItemOutput::Activate(ActivateRequest::Default {
                    address: address_clone.clone(),
                    x: 0,
                    y: 0,
                }))
                .unwrap_or_else(|_| log::error!("couldn't activate tray item {}", address_clone));
        });

        // right click for context menu
        // create a gesture for right-click detection
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // right click
        let address_clone = self.address.clone();
        gesture.connect_pressed(move |_, _, x, y| {
            log::debug!(
                "TODO: tray secondary activate requested: {}",
                address_clone.clone()
            );
            sender
                .output(TrayItemOutput::Activate(ActivateRequest::Secondary {
                    address: address_clone.clone(),
                    x: x.round() as i32,
                    y: y.round() as i32,
                }))
                .unwrap_or_else(|_| {
                    log::error!("couldn't secondary activate for {}", address_clone)
                })
        });
        root.add_controller(gesture);
    }
}

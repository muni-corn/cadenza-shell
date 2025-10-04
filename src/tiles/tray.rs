use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    services::tray::TRAY_STATE,
    tray::{TrayItem, TrayStatus},
    widgets::tile::TileOutput,
};

#[derive(Debug)]
pub struct TrayWidget {
    items: FactoryVecDeque<TrayItem>,
    visible: bool,
    expanded: bool,
}

#[derive(Debug)]
pub enum TrayMsg {
    UpdateItems(Vec<TrayItem>),
    ToggleExpanded,
}

#[relm4::component(pub)]
impl SimpleComponent for TrayWidget {
    type Init = ();
    type Input = TrayMsg;
    type Output = TileOutput;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
            #[watch]
            set_visible: model.visible,

            #[name(revealer)]
            gtk::Revealer {
                #[watch]
                set_reveal_child: model.expanded,
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_transition_duration: 200,

                #[local_ref]
                items_box -> gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 2,
                    set_margin_end: 4,
                }
            },

            gtk::Button {
                add_css_class: "tile",
                add_css_class: "tray",

                connect_clicked[sender] => move |_| {
                    sender.input(TrayMsg::ToggleExpanded);
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_halign: gtk::Align::Center,

                    gtk::Label {
                        #[watch]
                        set_label: if model.expanded { "󰅂" } else { "󰅁" }, // Arrow icons
                        add_css_class: "tile-icon",
                    },

                    gtk::Label {
                        #[watch]
                        set_text: &if !model.items.is_empty() {
                            model.items.len().to_string()
                        } else {
                            "".to_string()
                        },
                        #[watch]
                        set_visible: !model.items.is_empty(),
                        add_css_class: "tile-text",
                        add_css_class: "tray-count",
                    },
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = TrayWidget {
            items: FactoryVecDeque::builder()
                .launch(gtk::Box::default())
                .detach(),
            visible: false,
            expanded: false,
        };

        let items_box = model.items.widget();
        let widgets = view_output!();

        // Watch for tray state changes
        TRAY_STATE.subscribe(sender.input_sender(), |_state| {
            TrayMsg::UpdateItems(_state.items.clone())
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            TrayMsg::ToggleExpanded => {
                self.expanded = !self.expanded;
            }
            // TODO
            // TrayMsg::UpdateItems(items) => {
            //     self.items = items;
            //     self.visible = !self.items.is_empty();

            //     log::debug!("Tray items updated: {} items", self.items.len());
            //     for item in &self.items {
            //         log::debug!(
            //             "  - {} (icon: {:?}, tooltip: {:?})",
            //             item.id,
            //             item.icon_name,
            //             item.tooltip
            //         );
            //     }

            //     // For now, we'll just log the items since widget access is complex
            //     // In a future implementation, we could use a different approach

            //     // Update the items box with actual tray item buttons
            //     self.update_tray_items();
            // }
            _ => (),
        }
    }
}

impl TrayWidget {
    fn update_tray_items(&mut self) {
        // TODO
        // for now, just log what we would display
        log::info!("would display {} tray items:", self.items.len());
        // for (i, item) in self.items.iter().enumerate() {
        //     log::info!(
        //         "  [{}] {} - icon: {:?}, tooltip: {:?}",
        //         i,
        //         item.id,
        //         item.icon_name,
        //         item.tooltip
        //     );
        // }
    }

    fn create_tray_item_button(&self, item: &TrayItem) -> gtk::Button {
        let button = gtk::Button::new();
        button.add_css_class("bar-button");

        // Add status-specific CSS classes
        match item.status {
            TrayStatus::Active => button.add_css_class("tray-active"),
            TrayStatus::NeedsAttention => button.add_css_class("tray-needs-attention"),
            TrayStatus::Passive => {} // Default styling
        }

        // Create enhanced tooltip with more information
        let tooltip_text = if let Some(tooltip) = &item.tooltip {
            format!("{}\n{:?}", item.title, tooltip)
        } else {
            format!("{}\n{}", item.title, item.id)
        };
        button.set_tooltip_text(Some(&tooltip_text));
        button.set_width_request(24);
        button.set_height_request(24);

        // Create image for the button
        if let Some(icon_name) = &item.icon_name {
            let image = gtk::Image::from_icon_name(icon_name);
            image.set_pixel_size(16);
            image.set_halign(gtk::Align::Center);
            image.set_valign(gtk::Align::Center);
            button.set_child(Some(&image));
        } else {
            // Fallback to text
            let label = gtk::Label::new(Some(&item.id.chars().take(2).collect::<String>()));
            button.set_child(Some(&label));
        }

        // Connect click handler
        let item_id = item.id.clone();

        button.connect_clicked(move |_| {
            log::info!("Tray item clicked: {}", item_id);
            // TODO: Implement activation via D-Bus (Activate method)
            // This would call: proxy.activate(x, y)
        });

        button
    }
}

pub fn create_tray_widget() -> gtk4::Widget {
    let controller = TrayWidget::builder().launch(()).detach();
    controller.widget().clone().into()
}

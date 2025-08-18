use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::tiles::simple_battery::SimpleBatteryTile;
use crate::simple_messages::TileOutput;

pub struct SimpleBar {
    monitor: Monitor,
    battery: Controller<SimpleBatteryTile>,
}

#[derive(Debug)]
pub enum BarMsg {
    TileClicked(String),
}

#[derive(Debug)]
pub enum BarOutput {
    // Currently no outputs needed
}

#[relm4::component]
impl SimpleComponent for SimpleBar {
    type Init = Monitor;
    type Input = BarMsg;
    type Output = BarOutput;

    view! {
        #[root]
        window = gtk::ApplicationWindow {
            set_title: Some("Muse Shell Bar"),
            set_visible: true,
            
            // Configure window after creation
            connect_realize => move |window| {
                window.init_layer_shell();
                window.set_layer(Layer::Top);
                window.set_exclusive_zone(32);
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
            },

            #[name = "bar_container"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                add_css_class: "bar",
                set_height_request: 32,

                // Left section
                #[name = "left_section"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 20,
                    
                    gtk::Label {
                        set_text: "Workspaces",
                        add_css_class: "placeholder",
                    },
                },

                // Center section - clock placeholder
                #[name = "center_section"] 
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Center,
                    set_hexpand: true,
                    
                    gtk::Label {
                        set_text: "12:00",
                        add_css_class: "clock",
                    },
                },

                // Right section - system tiles
                #[name = "right_section"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_halign: gtk::Align::End,
                    set_hexpand: true,
                    
                    model.battery.widget(),
                },
            }
        }
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize battery tile
        let battery = SimpleBatteryTile::builder()
            .launch(())
            .forward(sender.input_sender(), |output| {
                match output {
                    TileOutput::Clicked(name) => BarMsg::TileClicked(name),
                }
            });

        let model = SimpleBar {
            monitor: monitor.clone(),
            battery,
        };

        let widgets = view_output!();

        // Set monitor for the window after creation
        widgets.window.set_monitor(Some(&monitor));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BarMsg::TileClicked(tile_name) => {
                log::debug!("Tile clicked: {}", tile_name);
            }
        }
    }
}
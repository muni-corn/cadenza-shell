use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::tiles::battery;


#[derive(Debug)]
struct Bar {
    monitor: Monitor,
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
impl SimpleComponent for Bar {
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
                    
                    // Battery tile will be added after initialization
                },
            }
        }
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Bar {
            monitor: monitor.clone(),
        };

        let widgets = view_output!();

        // Set monitor for the window after creation
        widgets.window.set_monitor(Some(&monitor));

        // Initialize battery tile and add to right section
        let battery_widget = battery::create_battery_widget();
        widgets.right_section.append(&battery_widget);

        // init layer shell
        if !window.is_layer_window() {
            window.init_layer_shell();
            log::debug!("layer shell initialized: {}", window.is_layer_window());
        } else {
            log::debug!("window already is a layer window");
        }

        window.set_layer(Layer::Top);
        log::debug!("set layer to top");

        // Use configuration for bar height
        let config = settings::get_config();
        window.set_exclusive_zone(config.bar.height);
        log::debug!("set exclusive zone to: {}", config.bar.height);

        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        log::debug!("set anchors: Top, Left, Right");

        let config = settings::get_config();

        let bar = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .spacing(config.bar.spacing)
            .margin_start(config.bar.margin)
            .margin_end(config.bar.margin)
            .build();

        window.set_child(Some(&bar));

        // Left section - workspaces and focused window
        let left = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["bar-left"])
            .build();

        // Center section - spacer and clock
        let center = gtk4::Box::builder()
            .hexpand(true)
            .halign(gtk4::Align::Center)
            .orientation(gtk4::Orientation::Horizontal)
            .build();

        let clock_label = gtk4::Label::builder()
            .label(chrono::Local::now().format("%H:%M").to_string())
            .css_classes(["bar-clock"])
            .build();

        center.append(&clock_label);

        // Right section - system tiles
        let right = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["bar-right"])
            .build();

        bar.append(&left);
        bar.append(&center);
        bar.append(&right);

        let widgets = BarWidgets { clock_label };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BarMsg::TileClicked(tile_name) => {
                log::debug!("Tile clicked: {}", tile_name);
            }
        }
    }

// Public function to create a bar for a monitor
pub fn create_bar(monitor: Monitor) {
    let _controller = Bar::builder().launch(monitor).detach();
    // The controller is detached and will live independently
}

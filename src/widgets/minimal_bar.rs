use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

#[derive(Debug)]
pub struct MinimalBar {
    monitor: Monitor,
}

#[derive(Debug)]
pub enum MinimalBarMsg {
    // Currently no messages needed
}

#[relm4::component]
impl SimpleComponent for MinimalBar {
    type Init = Monitor;
    type Input = MinimalBarMsg;
    type Output = ();

    view! {
        #[root]
        window = gtk4::ApplicationWindow {
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

            gtk4::Box {
                set_orientation: gtk4::Orientation::Horizontal,
                add_css_class: "bar",
                set_height_request: 32,
                set_hexpand: true,
                set_spacing: 8,
                set_margin_start: 8,
                set_margin_end: 8,

                // Left section
                gtk4::Box {
                    set_orientation: gtk4::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "bar-left",

                    gtk4::Label {
                        set_text: "Muse Shell",
                        add_css_class: "bar-title",
                    },
                },

                // Center section (spacer)
                gtk4::Box {
                    set_hexpand: true,
                },

                // Right section
                gtk4::Box {
                    set_orientation: gtk4::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "bar-right",

                    gtk4::Label {
                        set_text: &format!("Monitor: {}", 
                            model.monitor.connector().unwrap_or_default()),
                        add_css_class: "bar-info",
                    },

                    gtk4::Label {
                        set_text: &chrono::Local::now().format("%H:%M").to_string(),
                        add_css_class: "bar-clock",
                    },
                },
            }
        }
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = MinimalBar { monitor };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            // No messages to handle yet
        }
    }
}
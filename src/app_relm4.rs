use gdk4::Display;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;
use std::collections::HashMap;

use crate::style::load_css;
use crate::widgets::bar_relm4::BarComponent;

pub struct AppModel {
    bars: HashMap<String, Controller<BarComponent>>,
    display: Display,
}

#[derive(Debug)]
pub enum AppMsg {
    MonitorAdded(gdk4::Monitor),
    MonitorRemoved(String), // monitor connector name
    Quit,
}

#[relm4::component]
impl SimpleComponent for AppModel {
    type Init = ();
    type Input = AppMsg;
    type Output = ();

    view! {
        gtk::ApplicationWindow {
            set_visible: false, // This is a hidden root window
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Load CSS styles at startup
        if let Err(e) = load_css() {
            log::warn!("Failed to load CSS: {}", e);
        }

        let display = Display::default().expect("Could not get default display");
        
        let model = AppModel {
            bars: HashMap::new(),
            display: display.clone(),
        };

        let widgets = view_output!();

        // Set up monitor detection
        let monitors = display.monitors();
        
        // Create bars for existing monitors
        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            sender.input(AppMsg::MonitorAdded(monitor));
        }

        // Monitor for display changes
        monitors.connect_items_changed(glib::clone!(
            #[weak] sender,
            move |monitors, position, removed, added| {
                // Handle removed monitors
                for i in position..position + removed {
                    if let Some(monitor) = monitors.item(i).and_downcast::<gdk4::Monitor>() {
                        if let Some(connector) = monitor.connector() {
                            sender.input(AppMsg::MonitorRemoved(connector.to_string()));
                        }
                    }
                }
                
                // Handle added monitors
                for i in position..position + added {
                    if let Some(monitor) = monitors.item(i).and_downcast::<gdk4::Monitor>() {
                        sender.input(AppMsg::MonitorAdded(monitor));
                    }
                }
            }
        ));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppMsg::MonitorAdded(monitor) => {
                let connector = monitor.connector();
                if let Some(connector) = connector {
                    let connector_str = connector.to_string();
                    
                    if !self.bars.contains_key(&connector_str) {
                        log::info!("Creating bar for monitor: {}", connector_str);
                        
                        // Create a new bar component for this monitor
                        let bar_controller = BarComponent::builder()
                            .launch(monitor.clone())
                            .forward(sender.input_sender(), |output| {
                                // Handle bar outputs if needed
                                match output {
                                    // Currently no outputs defined
                                }
                            });
                        
                        self.bars.insert(connector_str, bar_controller);
                    }
                }
            }
            AppMsg::MonitorRemoved(connector) => {
                log::info!("Removing bar for monitor: {}", connector);
                self.bars.remove(&connector);
            }
            AppMsg::Quit => {
                log::info!("Quitting application");
                relm4::main_application().quit();
            }
        }
    }
}

pub struct MuseShellRelm4;

impl MuseShellRelm4 {
    pub fn new() -> RelmApp {
        RelmApp::new("com.muse.shell")
    }

    pub fn run() -> gtk4::glib::ExitCode {
        let app = Self::new();
        app.run::<AppModel>(())
    }
}
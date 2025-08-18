use gdk4::Display;
use gtk4::prelude::*;
use relm4::prelude::*;
use std::collections::HashMap;

use crate::style::load_css;
use crate::widgets::bar::create_bar;

#[derive(Debug)]
struct MuseShell {
    // Store just the connector names for now
    bars: HashMap<String, String>,
}

#[derive(Debug)]
pub enum MuseShellMsg {
    MonitorAdded(gdk4::Monitor),
    Quit,
}

#[relm4::component(pub)]
impl SimpleComponent for MuseShellModel {
    type Init = ();
    type Input = MuseShellMsg;
    type Output = ();

    view! {
        gtk::ApplicationWindow {
            set_visible: false, // Hidden root window
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Load CSS styles
        if let Err(e) = load_css() {
            log::warn!("failed to load css: {}", e);
        }

        let display = Display::default().expect("Could not get default display");
        let monitors = display.monitors();
        
        let model = MuseShell {
            bars: HashMap::new(),
        };

        let widgets = view_output!();

        // Create bars for existing monitors
        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            sender.input(MuseShellMsg::MonitorAdded(monitor));
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MuseShellMsg::MonitorAdded(monitor) => {
                let connector = monitor.connector();
                if let Some(connector) = connector {
                    let connector_str = connector.to_string();
                    
                    if !self.bars.contains_key(&connector_str) {
                        log::info!("Creating bar for monitor: {}", connector_str);
                        
                        let _bar_controller = create_bar(monitor.clone());
                        
                        self.bars.insert(connector_str.clone(), connector_str);
                    }
                }
            }
            AppMsg::Quit => {
                log::info!("Quitting application");
                relm4::main_application().quit();
            }
        }
    }
}

// Public interface to run the app
pub fn run() -> gtk4::glib::ExitCode {
    let app = RelmApp::new("com.muse.shell");
    app.run::<MuseShell>(());
    gtk4::glib::ExitCode::SUCCESS
}
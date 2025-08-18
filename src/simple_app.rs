use gdk4::Display;
use gtk4::prelude::*;
use relm4::prelude::*;
use std::collections::HashMap;

use crate::style::load_css;
use crate::widgets::simple_bar::SimpleBar;

pub struct SimpleApp {
    bars: HashMap<String, Controller<SimpleBar>>,
}

#[derive(Debug)]
pub enum AppMsg {
    MonitorAdded(gdk4::Monitor),
    Quit,
}

#[relm4::component]
impl SimpleComponent for SimpleApp {
    type Init = ();
    type Input = AppMsg;
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
            log::warn!("Failed to load CSS: {}", e);
        }

        let display = Display::default().expect("Could not get default display");
        let monitors = display.monitors();
        
        let model = SimpleApp {
            bars: HashMap::new(),
        };

        let widgets = view_output!();

        // Create bars for existing monitors
        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            sender.input(AppMsg::MonitorAdded(monitor));
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::MonitorAdded(monitor) => {
                let connector = monitor.connector();
                if let Some(connector) = connector {
                    let connector_str = connector.to_string();
                    
                    if !self.bars.contains_key(&connector_str) {
                        log::info!("Creating bar for monitor: {}", connector_str);
                        
                        let bar_controller = SimpleBar::builder()
                            .launch(monitor.clone())
                            .detach();
                        
                        self.bars.insert(connector_str, bar_controller);
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

pub fn run_simple_app() -> gtk4::glib::ExitCode {
    let app = RelmApp::new("com.muse.shell");
    app.run::<SimpleApp>(())
}
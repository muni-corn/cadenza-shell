use gdk4::Display;
use gtk4::prelude::*;
use relm4::prelude::*;
use std::collections::HashMap;

use crate::style::load_css;
use crate::widgets::bar::Bar;

#[derive(Debug)]
pub(crate) struct MuseShellModel {
    bars: HashMap<String, Controller<Bar>>,
    display: Display,
}

#[derive(Debug)]
pub enum MuseShellMsg {
    MonitorAdded(gdk4::Monitor),
    MonitorRemoved(String), // monitor connector name
}

impl SimpleComponent for MuseShellModel {
    type Init = ();
    type Input = MuseShellMsg;
    type Output = ();
    type Root = gtk::ApplicationWindow;
    type Widgets = ();

    fn init_root() -> Self::Root {
        // hidden root window
        gtk::ApplicationWindow::builder().visible(false).build()
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // load css styles at startup
        if let Err(e) = load_css() {
            log::warn!("failed to load css: {}", e);
        }

        let display = Display::default().expect("could not get default display");

        let model = MuseShellModel {
            bars: HashMap::new(),
            display: display.clone(),
        };

        // set up monitor detection
        let monitors = display.monitors();

        // create bars for existing monitors
        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            sender.input(MuseShellMsg::MonitorAdded(monitor));
        }

        // monitor for display changes (hotplug support)
        let sender_clone = sender.clone();
        monitors.connect_items_changed(move |monitors, position, removed, added| {
            // handle removed monitors
            for i in position..position + removed {
                if let Some(monitor) = monitors.item(i).and_downcast::<gdk4::Monitor>() {
                    if let Some(connector) = monitor.connector() {
                        sender_clone.input(MuseShellMsg::MonitorRemoved(connector.to_string()));
                    }
                }
            }

            // handle added monitors
            for i in position..position + added {
                if let Some(monitor) = monitors.item(i).and_downcast::<gdk4::Monitor>() {
                    sender_clone.input(MuseShellMsg::MonitorAdded(monitor));
                }
            }
        });

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MuseShellMsg::MonitorAdded(monitor) => {
                let connector = monitor.connector();
                if let Some(connector) = connector {
                    let connector_str = connector.to_string();

                    self.bars
                        .entry(connector_str.clone())
                        .or_insert_with(move || {
                            log::info!("creating bar for monitor: {}", connector_str);

                            // create a new bar component for this monitor
                            Bar::builder().launch(monitor.clone()).detach()
                        });
                }
            }
            MuseShellMsg::MonitorRemoved(connector) => {
                log::info!("removing bar for monitor: {}", connector);
                self.bars.remove(&connector);
            }
        }
    }
}

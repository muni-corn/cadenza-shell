use std::collections::HashMap;

use gdk4::Display;
use gtk4::prelude::*;
use relm4::{WorkerHandle, prelude::*};

use crate::{
    services::{battery::BatteryService, weather::WeatherService},
    widgets::bar::Bar,
};

pub(crate) struct CadenzaShellModel {
    bars: HashMap<String, Controller<Bar>>,
    display: Display,
    battery_service: WorkerHandle<BatteryService>,
    weather_service: WorkerHandle<WeatherService>,
}

#[derive(Debug)]
pub enum CadenzaShellMsg {
    MonitorAdded(gdk4::Monitor),
    MonitorRemoved(String), // monitor connector name
}

impl SimpleComponent for CadenzaShellModel {
    type Init = ();
    type Input = CadenzaShellMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = ();

    fn init_root() -> Self::Root {
        // hidden root window
        gtk::Window::new()
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // load css styles at startup

        let display = Display::default().expect("could not get default display");

        let model = CadenzaShellModel {
            bars: HashMap::new(),
            display: display.clone(),
            battery_service: BatteryService::builder().detach_worker(()),
            weather_service: WeatherService::builder().detach_worker(()),
        };

        // set up monitor detection
        let monitors = display.monitors();

        // create bars for existing monitors
        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            sender.input(CadenzaShellMsg::MonitorAdded(monitor));
        }

        // monitor for display changes (hotplug support)
        let sender_clone = sender.clone();
        monitors.connect_items_changed(move |monitors, position, removed, added| {
            // handle removed monitors
            for i in position..position + removed {
                if let Some(monitor) = monitors.item(i).and_downcast::<gdk4::Monitor>()
                    && let Some(connector) = monitor.connector()
                {
                    sender_clone.input(CadenzaShellMsg::MonitorRemoved(connector.to_string()));
                }
            }

            // handle added monitors
            for i in position..position + added {
                if let Some(monitor) = monitors.item(i).and_downcast::<gdk4::Monitor>() {
                    sender_clone.input(CadenzaShellMsg::MonitorAdded(monitor));
                }
            }
        });

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            CadenzaShellMsg::MonitorAdded(monitor) => {
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
            CadenzaShellMsg::MonitorRemoved(connector) => {
                log::info!("removing bar for monitor: {}", connector);
                self.bars.remove(&connector);
            }
        }
    }
}

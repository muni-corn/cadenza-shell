use std::{collections::HashMap, sync::Arc};

use gdk4::Display;
use gtk4::prelude::*;
use relm4::{WorkerHandle, prelude::*};
use tokio::sync::Mutex;

use crate::{
    services::{
        battery::BatteryService, brightness::BrightnessService, mpris::MprisService,
        network::NetworkService, niri::NiriService, pulseaudio::PulseAudioService,
        weather::WeatherService,
    },
    widgets::bar::Bar,
};

pub(crate) struct CadenzaShellModel {
    bars: HashMap<String, Controller<Bar>>,
    _display: Display,
    _battery_service: WorkerHandle<BatteryService>,
    _weather_service: WorkerHandle<WeatherService>,
    _brightness_service: WorkerHandle<BrightnessService>,
    _pulseaudio_service: WorkerHandle<PulseAudioService>,
    _network_service: WorkerHandle<NetworkService>,
    _niri_service: WorkerHandle<NiriService>,
    _mpris_service: WorkerHandle<MprisService>,
}

#[derive(Debug)]
pub enum CadenzaShellMsg {
    MonitorAdded(gdk4::Monitor),
    MonitorRemoved(String), // monitor connector name
}

#[derive(Debug)]
pub(crate) enum CadenzaShellCommandOutput {
    TrayEvent(TrayEvent),
}

impl AsyncComponent for CadenzaShellModel {
    type CommandOutput = CadenzaShellCommandOutput;
    type Init = ();
    type Input = CadenzaShellMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = ();

    fn init_root() -> Self::Root {
        // hidden root window
        gtk::Window::new()
    }

    async fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let display = Display::default().expect("could not get default display");

        let model = CadenzaShellModel {
            bars: HashMap::new(),
            _display: display.clone(),
            _battery_service: BatteryService::builder().detach_worker(()),
            _weather_service: WeatherService::builder().detach_worker(()),
            _brightness_service: BrightnessService::builder().detach_worker(()),
            _pulseaudio_service: PulseAudioService::builder().detach_worker(()),
            _network_service: NetworkService::builder().detach_worker(()),
            _niri_service: NiriService::builder().detach_worker(()),
            _mpris_service: MprisService::builder().detach_worker(()),
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

        AsyncComponentParts { model, widgets: () }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
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

    async fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        _sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            Self::CommandOutput::TrayEvent(event) => {
                for bar in self.bars.values() {
                    bar.emit(BarMsg::TrayEvent(event.clone()));
                }
            }
        }
    }
}

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
    tray::{TrayClient, TrayEvent, TrayItemOutput},
    widgets::bar::{Bar, BarInit, BarMsg, BarOutput},
};

pub(crate) struct CadenzaShellModel {
    bars: HashMap<String, AsyncController<Bar>>,
    tray_client: Option<Arc<Mutex<TrayClient>>>,

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
pub(crate) enum CadenzaShellMsg {
    MonitorAdded(gdk4::Monitor),
    MonitorRemoved(String), // monitor connector name
    HandleTrayItemOutput(TrayItemOutput),
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
        let tray_client = TrayClient::new()
            .await
            .inspect_err(|e| log::error!("couldn't setup tray client: {}", e))
            .ok()
            .map(|c| Arc::new(Mutex::new(c)));

        if let Some(ref tray_client) = tray_client {
            let tray_client = Arc::clone(tray_client);
            sender.command(|out, shutdown| {
                shutdown
                    .register(async move {
                        // subscribe to tray events
                        let mut rx = tray_client.lock().await.subscribe();
                        loop {
                            match rx.recv().await {
                                Ok(event) => {
                                    log::debug!("tray event received: {:?}", event);
                                    out.send(CadenzaShellCommandOutput::TrayEvent(event))
                                        .unwrap_or_else(|_| {
                                            log::error!(
                                                "unable to send tray event as command output",
                                            )
                                        });
                                }
                                Err(e) => log::error!("error receiving tray event: {}", e),
                            }
                        }
                    })
                    .drop_on_shutdown()
            });
        }
        let display = Display::default().expect("could not get default display");

        let model = CadenzaShellModel {
            bars: HashMap::new(),
            tray_client,

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

                    // get the current system tray items
                    let tray_items = if let Some(ref c) = self.tray_client {
                        Some(c.lock().await.items())
                    } else {
                        None
                    };

                    self.bars
                        .entry(connector_str.clone())
                        .or_insert_with(move || {
                            log::info!("creating bar for monitor: {}", connector_str);

                            // create a new bar component for this monitor
                            Bar::builder()
                                .launch(BarInit {
                                    monitor,
                                    tray_items,
                                })
                                .forward(sender.input_sender(), |output| match output {
                                    BarOutput::ToggleNotificationCenter => {
                                        todo!()
                                    }
                                    BarOutput::TrayItemOutput(tray_item_output) => {
                                        CadenzaShellMsg::HandleTrayItemOutput(tray_item_output)
                                    }
                                })
                        });
                }
            }
            CadenzaShellMsg::MonitorRemoved(connector) => {
                log::info!("removing bar for monitor: {}", connector);
                self.bars.remove(&connector);
            }
            CadenzaShellMsg::HandleTrayItemOutput(tray_item_output) => match tray_item_output {
                TrayItemOutput::Activate(activate_request) => {
                    if let Some(client) = &self.tray_client {
                        client
                            .lock()
                            .await
                            .activate(activate_request)
                            .await
                            .unwrap_or_else(|e| {
                                log::error!("error sending activate request to tray client: {}", e)
                            });
                    }
                }
                TrayItemOutput::RequestMenu => todo!(),
            },
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

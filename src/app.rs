use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use gdk4::Display;
use gtk4::prelude::*;
use relm4::prelude::*;
use tokio::sync::Mutex;

use crate::{
    battery::start_battery_service,
    bluetooth::run_bluetooth_service,
    brightness::start_brightness_watcher,
    mpris::run_mpris_service,
    network::run_network_service,
    niri,
    notifications::run_notifications_service,
    pulseaudio::run_pulseaudio_loop,
    sleep_monitor::run_sleep_monitor,
    weather::start_weather_polling,
    widgets::{
        bar::{Bar, BarInit, BarMsg, BarOutput},
        tray_item::{TrayClient, TrayEvent, TrayItemOutput},
    },
};

pub(crate) struct CadenzaShellModel {
    bars: HashMap<String, AsyncController<Bar>>,
    tray_client: Option<Arc<Mutex<TrayClient>>>,

    _display: Display,
}

#[derive(Debug)]
pub(crate) enum CadenzaShellMsg {
    MonitorAdded(gdk4::Monitor),
    MonitorRemoved(String), // monitor connector name
    HandleTrayItemOutput(TrayItemOutput),
    ToggleNotificationCenter,
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

        // start sleep monitor (must be first so other services can subscribe)
        sender.command(|_, shutdown| shutdown.register(run_sleep_monitor()).drop_on_shutdown());

        // start notifications service (registers the D-Bus daemon; must be
        // started before any UI component subscribes or issues commands)
        sender.command(|_, shutdown| {
            shutdown
                .register(run_notifications_service())
                .drop_on_shutdown()
        });

        // start battery watching
        sender.command(|_, shutdown| {
            shutdown
                .register(start_battery_service())
                .drop_on_shutdown()
        });

        // start bluetooth watching
        sender.command(|_, shutdown| {
            shutdown
                .register(run_bluetooth_service())
                .drop_on_shutdown()
        });

        // start brightness watching
        sender.command(|_, shutdown| {
            shutdown
                .register(start_brightness_watcher())
                .drop_on_shutdown()
        });

        // start network service
        sender.command(|_, shutdown| shutdown.register(run_network_service()).drop_on_shutdown());

        // start weather watching
        sender.command(|_, shutdown| {
            shutdown
                .register(start_weather_polling())
                .drop_on_shutdown()
        });

        // start mpris service
        sender.command(|_, shutdown| shutdown.register(run_mpris_service()).drop_on_shutdown());

        // start niri event watching
        sender.command(|_, shutdown| {
            shutdown
                .register(niri::start_event_listener())
                .drop_on_shutdown()
        });

        // start pulseaudio service
        sender.command(|_, shutdown| shutdown.register(run_pulseaudio_loop()).drop_on_shutdown());

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
        };

        // set up monitor detection
        let monitors = display.monitors();

        // build initial connector list, mirroring the GListModel order so we
        // can recover connector names for removed items later (items_changed
        // fires after the model has already mutated, so we cannot call
        // monitors.item(i) for removed indices)
        let tracked: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(
            monitors
                .iter::<gdk4::Monitor>()
                .filter_map(|m| m.ok())
                .filter_map(|m| m.connector().map(|c| c.to_string()))
                .collect(),
        ));

        log::debug!("initial monitors: {:?}", tracked.borrow().as_slice());

        // create bars for existing monitors (skip any without a connector)
        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            if monitor.connector().is_some() {
                sender.input(CadenzaShellMsg::MonitorAdded(monitor));
            }
        }

        // monitor for display changes (hotplug support)
        let sender_clone = sender.clone();
        monitors.connect_items_changed(move |monitors, position, removed, added| {
            log::debug!(
                "items_changed: position={}, removed={}, added={}",
                position,
                removed,
                added
            );

            // read removed connector names from our tracked list before mutating
            // it; after the signal fires the model has already changed, so
            // monitors.item(i) for removed indices would return the wrong item
            let removed_connectors: Vec<String> = {
                let tracked = tracked.borrow();
                let start = position as usize;
                let end = (position + removed) as usize;
                tracked.get(start..end).unwrap_or(&[]).to_vec()
            };

            // collect newly added monitor objects (at position..position+added)
            let added_monitors: Vec<gdk4::Monitor> = (position..position + added)
                .filter_map(|i| monitors.item(i).and_downcast::<gdk4::Monitor>())
                .collect();

            // build the new connector names for items being added
            let added_connectors: Vec<String> = added_monitors
                .iter()
                .filter_map(|m| m.connector().map(|c| c.to_string()))
                .collect();

            // update the tracked list: splice out the removed range and insert
            // the new connectors in their place, mirroring the model mutation
            {
                let mut tracked = tracked.borrow_mut();
                tracked.splice(
                    (position as usize)..(position as usize + removed as usize),
                    added_connectors.clone(),
                );
            }

            log::debug!(
                "hotplug — removing {:?}, adding {:?}",
                removed_connectors,
                added_connectors
            );

            // emit removal messages for every connector that just left
            for connector in removed_connectors {
                sender_clone.input(CadenzaShellMsg::MonitorRemoved(connector));
            }

            // emit addition messages for every monitor that just joined
            for monitor in added_monitors {
                sender_clone.input(CadenzaShellMsg::MonitorAdded(monitor));
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
                let Some(connector) = monitor.connector() else {
                    log::warn!("ignoring monitor with no connector name");
                    return;
                };

                let connector_str = connector.to_string();

                // replace any existing (now-stale) bar for this connector so the
                // new Monitor object is used; this handles disconnect→reconnect
                // cycles where the compositor reuses the same connector name
                if self.bars.contains_key(&connector_str) {
                    log::warn!(
                        "bar already exists for connector '{}' — replacing with fresh monitor",
                        connector_str
                    );
                    self.bars.remove(&connector_str);
                }

                // get the current system tray items
                let tray_items = if let Some(ref c) = self.tray_client {
                    Some(c.lock().await.items())
                } else {
                    None
                };

                log::info!("creating bar for monitor: {}", connector_str);

                let bar = Bar::builder()
                    .launch(BarInit {
                        monitor,
                        tray_items,
                    })
                    .forward(sender.input_sender(), |output| match output {
                        BarOutput::ToggleNotificationCenter => {
                            CadenzaShellMsg::ToggleNotificationCenter
                        }
                        BarOutput::TrayItemOutput(tray_item_output) => {
                            CadenzaShellMsg::HandleTrayItemOutput(tray_item_output)
                        }
                    });

                self.bars.insert(connector_str, bar);
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
            },
            CadenzaShellMsg::ToggleNotificationCenter => {
                // broadcast to all bars so each monitor's center toggles
                for bar in self.bars.values() {
                    bar.emit(BarMsg::ToggleNotificationCenter);
                }
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

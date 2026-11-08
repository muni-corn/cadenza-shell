use std::sync::Arc;

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
    notifications::{
        NotificationEvent,
        fresh::{FreshNotifications, FreshNotificationsMsg, FreshNotificationsOutput},
        run_notifications_service, subscribe_events,
    },
    pulseaudio::run_pulseaudio_loop,
    sleep_monitor::run_sleep_monitor,
    weather::start_weather_polling,
    widgets::{
        bar::{Bar, BarInit, BarMsg, BarOutput},
        tray_item::{TrayClient, TrayEvent, TrayItemOutput},
    },
};

/// Pairs a monitor with its fresh-notification panel so we can drop the panel
/// if that specific monitor goes away.
struct FreshPanel {
    monitor: gdk4::Monitor,
    controller: Controller<FreshNotifications>,
}

/// A bar together with the monitor it was created for.
struct MonitorBar {
    monitor: gdk4::Monitor,
    controller: AsyncController<Bar>,
}

pub(crate) struct CadenzaShellModel {
    display: Display,
    bars: Vec<MonitorBar>,
    tray_client: Option<Arc<Mutex<TrayClient>>>,
    /// Single global fresh-notification popup panel, anchored to one monitor.
    fresh_panel: Option<FreshPanel>,
}

#[derive(Debug)]
pub(crate) enum CadenzaShellMsg {
    /// The monitor list changed or a monitor was invalidated; bars are
    /// reconciled against the current list.
    MonitorsChanged,
    HandleTrayItemOutput(TrayItemOutput),
    ToggleNotificationCenter,
    /// A notification event forwarded from the global broadcast channel.
    NotificationEvent(NotificationEvent),
    /// A notification was dismissed from the fresh panel.
    FreshNotificationDismissed(u32),
    /// A notification action was triggered from the fresh panel.
    FreshNotificationAction(u32, String),
}

#[derive(Debug)]
pub(crate) enum CadenzaShellCommandOutput {
    TrayEvent(TrayEvent),
    NotificationEvent(NotificationEvent),
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
            .inspect_err(|e| tracing::error!("couldn't setup tray client: {}", e))
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

        // subscribe to notification events and forward them into update()
        // note: run_notifications_service initializes EVENT_TX eagerly, so
        // subscribe_events() is safe to call from here
        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    let mut rx = subscribe_events();
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                out.send(CadenzaShellCommandOutput::NotificationEvent(event))
                                    .unwrap_or_else(|_| {
                                        tracing::error!(
                                            "couldn't forward notification event to app"
                                        )
                                    });
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    missed_events = n,
                                    "app lagged on notification broadcast receiver"
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::warn!(
                                    "notification event channel closed; app listener stopping"
                                );
                                break;
                            }
                        }
                    }
                })
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
                                            tracing::error!(
                                                "unable to send tray event as command output",
                                            )
                                        });
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    tracing::warn!(
                                        missed_events = n,
                                        "app lagged on tray event receiver"
                                    );
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    tracing::warn!(
                                        "tray event channel closed; app listener stopping"
                                    );
                                    break;
                                }
                            }
                        }
                    })
                    .drop_on_shutdown()
            });
        }
        let display = Display::default().expect("could not get default display");

        let model = CadenzaShellModel {
            display: display.clone(),
            bars: Vec::new(),
            tray_client,
            fresh_panel: None,
        };

        // reconcile bars whenever the monitor list changes (hotplug), and once
        // now for the monitors that already exist
        let sender_clone = sender.clone();
        display
            .monitors()
            .connect_items_changed(move |_, position, removed, added| {
                tracing::debug!(position, removed, added, "monitor list changed");
                sender_clone.input(CadenzaShellMsg::MonitorsChanged);
            });
        sender.input(CadenzaShellMsg::MonitorsChanged);

        AsyncComponentParts { model, widgets: () }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            CadenzaShellMsg::MonitorsChanged => self.reconcile_bars(&sender).await,
            CadenzaShellMsg::HandleTrayItemOutput(tray_item_output) => match tray_item_output {
                TrayItemOutput::Activate(activate_request) => {
                    if let Some(client) = &self.tray_client {
                        client
                            .lock()
                            .await
                            .activate(activate_request)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::error!(
                                    "error sending activate request to tray client: {}",
                                    e
                                )
                            });
                    }
                }
            },
            CadenzaShellMsg::ToggleNotificationCenter => {
                // broadcast to all bars so each monitor's center toggles
                for bar in &self.bars {
                    bar.controller.emit(BarMsg::ToggleNotificationCenter);
                }
            }
            CadenzaShellMsg::NotificationEvent(event) => {
                let Some(panel) = &self.fresh_panel else {
                    return;
                };

                match event {
                    NotificationEvent::Received(notification) => {
                        tracing::debug!(
                            notification.id = notification.id,
                            notification.app = %notification.app_name,
                            "app forwarding new notification to fresh panel"
                        );
                        panel
                            .controller
                            .emit(FreshNotificationsMsg::NewNotification(notification));
                    }
                    NotificationEvent::Closed { id, .. } => {
                        tracing::debug!(
                            notification.id = id,
                            "app forwarding notification closed to fresh panel"
                        );
                        panel
                            .controller
                            .emit(FreshNotificationsMsg::RemoveNotification(id));
                    }
                    // AllCleared and ActionInvoked don't require fresh panel
                    // updates; individual Closed events will drain it
                    NotificationEvent::AllCleared | NotificationEvent::ActionInvoked { .. } => {}
                }
            }
            CadenzaShellMsg::FreshNotificationDismissed(id) => {
                tracing::debug!(notification.id = id, "fresh panel dismissed notification");
                crate::notifications::dismiss(id);
            }
            CadenzaShellMsg::FreshNotificationAction(id, action) => {
                tracing::debug!(
                    notification.id = id,
                    %action,
                    "fresh panel triggered notification action"
                );
                crate::notifications::invoke_action(id, action);
            }
        }
    }

    async fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            Self::CommandOutput::TrayEvent(event) => {
                for bar in &self.bars {
                    bar.controller.emit(BarMsg::TrayEvent(event.clone()));
                }
            }
            Self::CommandOutput::NotificationEvent(event) => {
                sender.input(CadenzaShellMsg::NotificationEvent(event));
            }
        }
    }
}

impl CadenzaShellModel {
    /// Creates bars for new monitors and drops bars whose monitor is gone.
    ///
    /// Monitors are compared by identity rather than connector name, so a
    /// reconnected display that reuses its connector name gets a fresh bar
    /// bound to its new monitor object.
    async fn reconcile_bars(&mut self, sender: &AsyncComponentSender<Self>) {
        let current: Vec<gdk4::Monitor> = self
            .display
            .monitors()
            .iter::<gdk4::Monitor>()
            .filter_map(Result::ok)
            .filter(|monitor| monitor.is_valid() && monitor.connector().is_some())
            .collect();

        self.bars.retain(|bar| {
            let keep = current.contains(&bar.monitor);
            if !keep {
                tracing::info!("removing bar for monitor: {:?}", bar.monitor.connector());
            }
            keep
        });

        if self
            .fresh_panel
            .as_ref()
            .is_some_and(|panel| !current.contains(&panel.monitor))
        {
            tracing::info!("dropping fresh notification panel; monitor removed");
            self.fresh_panel = None;
        }

        for monitor in current {
            if self.bars.iter().any(|bar| bar.monitor == monitor) {
                continue;
            }
            self.create_bar(monitor, sender).await;
        }

        // keep popups available after their monitor is unplugged by moving
        // the panel to any remaining monitor
        if let Some(monitor) = self.bars.first().map(|bar| bar.monitor.clone()) {
            self.ensure_fresh_panel(monitor, sender);
        }
    }

    /// Creates a bar on `monitor` and reconciles again as soon as the
    /// monitor is invalidated, which gdk can report before the monitor list
    /// changes.
    async fn create_bar(&mut self, monitor: gdk4::Monitor, sender: &AsyncComponentSender<Self>) {
        tracing::info!("creating bar for monitor: {:?}", monitor.connector());

        let tray_items = if let Some(ref c) = self.tray_client {
            Some(c.lock().await.items())
        } else {
            None
        };

        let controller = Bar::builder()
            .launch(BarInit {
                monitor: monitor.clone(),
                tray_items,
            })
            .forward(sender.input_sender(), |output| match output {
                BarOutput::ToggleNotificationCenter => CadenzaShellMsg::ToggleNotificationCenter,
                BarOutput::TrayItemOutput(tray_item_output) => {
                    CadenzaShellMsg::HandleTrayItemOutput(tray_item_output)
                }
            });

        let input = sender.input_sender().clone();
        monitor.connect_invalidate(move |monitor| {
            tracing::info!("monitor invalidated: {:?}", monitor.connector());
            input.emit(CadenzaShellMsg::MonitorsChanged);
        });

        self.bars.push(MonitorBar {
            monitor,
            controller,
        });
    }

    /// Creates the fresh notification panel anchored to `monitor` if none
    /// exists yet. Does nothing if a panel is already running.
    fn ensure_fresh_panel(&mut self, monitor: gdk4::Monitor, sender: &AsyncComponentSender<Self>) {
        if self.fresh_panel.is_some() {
            return;
        }

        tracing::info!(
            "creating fresh notification panel on monitor: {:?}",
            monitor.connector()
        );

        let controller = FreshNotifications::builder()
            .launch(monitor.clone())
            .forward(sender.input_sender(), |msg| match msg {
                FreshNotificationsOutput::NotificationDismissed(id) => {
                    CadenzaShellMsg::FreshNotificationDismissed(id)
                }
                FreshNotificationsOutput::NotificationActionTriggered(id, action) => {
                    CadenzaShellMsg::FreshNotificationAction(id, action)
                }
            });

        self.fresh_panel = Some(FreshPanel {
            monitor,
            controller,
        });
    }
}

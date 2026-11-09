use std::sync::Arc;

use gdk4::Display;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    notifications::{
        NotificationEvent,
        fresh::{FreshNotifications, FreshNotificationsMsg, FreshNotificationsOutput},
        subscribe_events,
        types::InvokedAction,
    },
    widgets::{
        bar::{Bar, BarInit},
        tray_item::TrayClient,
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
    /// Held so the bar lives exactly as long as its monitor entry.
    _bar: Controller<Bar>,
}

pub(crate) struct CadenzaShellModel {
    display: Display,
    bars: Vec<MonitorBar>,
    tray_client: Option<Arc<TrayClient>>,
    /// Single global fresh-notification popup panel, anchored to one monitor.
    fresh_panel: Option<FreshPanel>,
}

#[derive(Debug)]
pub(crate) enum CadenzaShellMsg {
    /// The monitor list changed or a monitor was invalidated; bars are
    /// reconciled against the current list.
    MonitorsChanged,
    /// A notification was dismissed from the fresh panel.
    FreshNotificationDismissed(u32),
    /// A notification action was triggered from the fresh panel.
    FreshNotificationAction(InvokedAction),
}

#[derive(Debug)]
pub(crate) enum CadenzaShellCommandOutput {
    NotificationEvent(NotificationEvent),
    /// Some notification events were missed, so popups may be stale.
    NotificationEventsMissed,
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
            .map(Arc::new);

        crate::services::start_all(&sender);

        // subscribe to notification events and forward them to the fresh panel
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
                                let _ =
                                    out.send(CadenzaShellCommandOutput::NotificationEventsMissed);
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
            CadenzaShellMsg::MonitorsChanged => self.reconcile_bars(&sender),
            CadenzaShellMsg::FreshNotificationDismissed(id) => {
                tracing::debug!(notification.id = id, "fresh panel dismissed notification");
                crate::notifications::dismiss(id);
            }
            CadenzaShellMsg::FreshNotificationAction(action) => {
                tracing::debug!(
                    notification.id = action.id,
                    action = %action.key,
                    "fresh panel triggered notification action"
                );
                crate::notifications::invoke_action(action);
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
            Self::CommandOutput::NotificationEvent(event) => {
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
                    NotificationEvent::Closed { id } => {
                        tracing::debug!(
                            notification.id = id,
                            "app forwarding notification closed to fresh panel"
                        );
                        panel
                            .controller
                            .emit(FreshNotificationsMsg::RemoveNotification(id));
                    }
                }
            }
            Self::CommandOutput::NotificationEventsMissed => {
                if let Some(panel) = &self.fresh_panel {
                    panel.controller.emit(FreshNotificationsMsg::Resync);
                }
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
    fn reconcile_bars(&mut self, sender: &AsyncComponentSender<Self>) {
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
            self.create_bar(monitor, sender);
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
    fn create_bar(&mut self, monitor: gdk4::Monitor, sender: &AsyncComponentSender<Self>) {
        tracing::info!("creating bar for monitor: {:?}", monitor.connector());

        let bar = Bar::builder()
            .launch(BarInit {
                monitor: monitor.clone(),
                tray_client: self.tray_client.clone(),
            })
            .detach();

        let input = sender.input_sender().clone();
        monitor.connect_invalidate(move |monitor| {
            tracing::info!("monitor invalidated: {:?}", monitor.connector());
            input.emit(CadenzaShellMsg::MonitorsChanged);
        });

        self.bars.push(MonitorBar { monitor, _bar: bar });
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
                FreshNotificationsOutput::NotificationActionTriggered(action) => {
                    CadenzaShellMsg::FreshNotificationAction(action)
                }
            });

        self.fresh_panel = Some(FreshPanel {
            monitor,
            controller,
        });
    }
}

mod apply;
mod devices;
mod watchers;

use std::{collections::HashMap, time::Duration};

use bluer::{
    Adapter, AdapterEvent, Address, DeviceEvent, Session, SessionEvent, agent::AgentHandle,
};
use futures_lite::StreamExt;
use tokio::{
    sync::mpsc::{UnboundedSender, unbounded_channel},
    task::AbortHandle,
    time::MissedTickBehavior,
};

use self::{
    apply::{apply_adapter_property, apply_device_property, update_state},
    watchers::{ensure_agent, spawn_bluez_owner_watcher, spawn_session_watcher},
};
use crate::{
    bluetooth::{
        actions,
        commands::{self, BluetoothCommand},
    },
    settings, sleep_monitor,
};

/// Events produced by the watcher tasks for the service loop.
#[derive(Debug)]
enum BluetoothEvent {
    Adapter(AdapterEvent),
    Device(Address, DeviceEvent),
    /// An adapter appeared or disappeared on the bus (e.g. a USB dongle
    /// being unplugged or replugged, or rfkill being toggled).
    Session(SessionEvent),
    /// bluetoothd gained a new owner on the bus (it started or restarted),
    /// which loses our agent registration and every adapter subscription.
    BluezRestarted,
}

/// Runs the bluetooth service: watches bluez, publishes [`BLUETOOTH_STATE`],
/// and handles commands from the menu, all from a single loop.
pub async fn run_bluetooth_service() {
    // installed first, so a second instance bails out before starting tasks
    let (cmd_tx, mut cmd_rx) = unbounded_channel::<BluetoothCommand>();
    if commands::install_command_sender(cmd_tx).is_err() {
        tracing::warn!("bluetooth service started more than once; extra instance exiting");
        return;
    }

    let session = match Session::new().await {
        Ok(session) => session,
        Err(e) => {
            tracing::error!("couldn't initialize bluetooth session: {e}");
            return;
        }
    };

    let (event_tx, mut event_rx) = unbounded_channel();
    let mut service = BluetoothService::start(session, event_tx).await;

    let mut wake_rx = sleep_monitor::subscribe_wake();

    // periodically reconcile full state so a missed bluez event never leaves
    // the tile stale indefinitely
    let mut reconcile = tokio::time::interval(Duration::from_secs(
        settings::get_config().bluetooth.reconcile_interval,
    ));
    reconcile.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // the first tick fires immediately, but start() already fetched state
    reconcile.tick().await;

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => service.handle_event(event).await,
            Some(cmd) = cmd_rx.recv() => service.handle_command(cmd).await,
            Ok(()) = wake_rx.recv() => service.reconcile("system wake").await,
            _ = reconcile.tick() => service.reconcile("periodic reconcile").await,
            else => break,
        }
    }
    tracing::warn!("bluetooth service has stopped receiving events");
}

/// Everything the bluetooth service loop owns. Dropping it stops every task
/// it started and unregisters the agent.
struct BluetoothService {
    session: Session,
    event_tx: UnboundedSender<BluetoothEvent>,
    /// The agent stays registered for as long as its handle is held.
    agent: Option<AgentHandle>,
    /// The adapter in use; kept out of shared state so the ui never does
    /// D-Bus I/O itself.
    adapter: Option<Adapter>,
    /// Session and bus name watchers that live as long as the service.
    background_tasks: Vec<AbortHandle>,
    adapter_events_task: Option<AbortHandle>,
    device_event_tasks: HashMap<Address, AbortHandle>,
    discovery_task: Option<AbortHandle>,
}

impl BluetoothService {
    /// Registers the agent, starts the watchers and reads the initial state.
    async fn start(session: Session, event_tx: UnboundedSender<BluetoothEvent>) -> Self {
        let background_tasks = vec![
            // bluer's session events don't report bluetoothd restarting, so
            // watch its bus name directly
            spawn_bluez_owner_watcher(event_tx.clone()),
            spawn_session_watcher(session.clone(), event_tx.clone()),
        ];

        let mut service = Self {
            session,
            event_tx,
            agent: None,
            adapter: None,
            background_tasks,
            adapter_events_task: None,
            device_event_tasks: HashMap::new(),
            discovery_task: None,
        };
        ensure_agent(&service.session, &mut service.agent).await;
        service.reinitialize_adapter().await;
        service
    }

    async fn handle_event(&mut self, event: BluetoothEvent) {
        match event {
            BluetoothEvent::Session(SessionEvent::AdapterRemoved(name)) => {
                let is_ours = self
                    .adapter
                    .as_ref()
                    .is_some_and(|adapter| adapter.name() == name);
                if is_ours {
                    tracing::warn!("bluetooth adapter '{name}' disappeared");
                    // switch to any other adapter that's still present
                    self.reinitialize_adapter().await;
                }
            }
            BluetoothEvent::Session(SessionEvent::AdapterAdded(name)) => {
                if self.adapter.is_none() {
                    tracing::info!("bluetooth adapter '{name}' available, initializing");
                    self.reinitialize_adapter().await;
                }
            }
            BluetoothEvent::BluezRestarted => {
                tracing::info!("bluetoothd restarted, re-registering agent and subscriptions");
                self.agent = None;
                ensure_agent(&self.session, &mut self.agent).await;
                self.reinitialize_adapter().await;
            }
            BluetoothEvent::Adapter(AdapterEvent::DeviceAdded(address)) => {
                self.add_device(address).await;
            }
            BluetoothEvent::Adapter(AdapterEvent::DeviceRemoved(address)) => {
                if let Some(handle) = self.device_event_tasks.remove(&address) {
                    handle.abort();
                }
                update_state(|state| {
                    state.devices.remove(&address);
                });
            }
            BluetoothEvent::Adapter(AdapterEvent::PropertyChanged(property)) => {
                apply_adapter_property(property);
            }
            BluetoothEvent::Device(address, DeviceEvent::PropertyChanged(property)) => {
                apply_device_property(address, property);
            }
        }
    }

    /// Dispatches a single UI-issued command, logging failures. They aren't
    /// fatal to the service; the user can simply retry from the menu.
    async fn handle_command(&mut self, cmd: BluetoothCommand) {
        match cmd {
            BluetoothCommand::StartDiscovery => self.start_discovery().await,
            BluetoothCommand::StopDiscovery => {
                if let Some(handle) = self.discovery_task.take() {
                    handle.abort();
                }
            }
            BluetoothCommand::SetPowered(powered) => match self.adapter.clone() {
                Some(adapter) => actions::spawn_set_powered(adapter, powered),
                None => tracing::warn!("can't toggle bluetooth: no adapter available"),
            },
            BluetoothCommand::Device(address, action) => match self.adapter.clone() {
                Some(adapter) => actions::spawn_device_action(adapter, address, action),
                None => tracing::warn!("can't {action:?} {address}: no adapter available"),
            },
        }
    }

    /// Self-heals anything a missed event left stale.
    ///
    /// A failed agent registration (e.g. bluetoothd wasn't up yet) is
    /// retried, and a missed `AdapterAdded` is healed by looking for an
    /// adapter again.
    async fn reconcile(&mut self, reason: &str) {
        tracing::debug!("reconciling bluetooth state: {reason}");
        ensure_agent(&self.session, &mut self.agent).await;
        if self.adapter.is_none() {
            self.reinitialize_adapter().await;
        } else {
            self.reconcile_devices().await;
        }
    }

    /// Stops every subscription tied to the current adapter.
    fn stop_adapter_tasks(&mut self) {
        self.adapter = None;
        if let Some(handle) = self.adapter_events_task.take() {
            handle.abort();
        }
        for (_, handle) in self.device_event_tasks.drain() {
            handle.abort();
        }
        if let Some(handle) = self.discovery_task.take() {
            handle.abort();
        }
    }

    /// Starts a discovery session, holding it open (and thus BlueZ's
    /// `Discovering` state true) until `bluetooth.discovery_timeout` elapses or
    /// it's cancelled by a `StopDiscovery` command. A no-op if one is still
    /// running.
    ///
    /// `Adapter::discover_devices`'s returned stream doesn't need to be read
    /// for us to learn about discovered devices - the adapter-wide event
    /// subscription already established in `reinitialize_adapter` picks those
    /// up via the normal `DeviceAdded` path - but the stream (and the discovery
    /// session token it holds internally) must stay alive for discovery to
    /// remain active, so this task exists purely to hold and eventually drop
    /// it.
    async fn start_discovery(&mut self) {
        // the task ends on its own after the timeout, so a finished handle
        // doesn't count as an active session
        if self
            .discovery_task
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
        {
            tracing::debug!("discovery already in progress, ignoring StartDiscovery");
            return;
        }

        let Some(adapter) = self.adapter.clone() else {
            tracing::warn!("can't start discovery: no bluetooth adapter available");
            return;
        };

        let timeout_secs = settings::get_config().bluetooth.discovery_timeout;

        let mut stream = match adapter.discover_devices().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("couldn't start bluetooth discovery: {e}");
                return;
            }
        };

        let handle = relm4::spawn(async move {
            let sleep = tokio::time::sleep(Duration::from_secs(timeout_secs));
            tokio::pin!(sleep);

            loop {
                tokio::select! {
                    () = &mut sleep => {
                        tracing::debug!("discovery timeout elapsed");
                        break;
                    }
                    event = stream.next() => {
                        if event.is_none() {
                            tracing::debug!("discovery stream ended");
                            break;
                        }
                    }
                }
            }
            // dropping `stream` here (end of scope) drops its internal
            // discovery session token, which is what actually stops discovery
        })
        .abort_handle();

        self.discovery_task = Some(handle);
    }
}

impl Drop for BluetoothService {
    fn drop(&mut self) {
        self.stop_adapter_tasks();
        for handle in &self.background_tasks {
            handle.abort();
        }
    }
}

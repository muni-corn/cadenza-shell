use std::{collections::HashMap, time::Duration};

use bluer::{
    AdapterEvent, AdapterProperty, Address, Device, DeviceEvent, DeviceProperty, Session,
    SessionEvent, agent::AgentHandle,
};
use futures_lite::StreamExt;
use tokio::{
    sync::mpsc::{UnboundedSender, unbounded_channel},
    task::AbortHandle,
    time::MissedTickBehavior,
};

use crate::{
    bluetooth::{
        actions, agent,
        commands::{self, BluetoothCommand},
        state::{BLUETOOTH_STATE, BluetoothState, DeviceInfo},
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
                let is_ours = BLUETOOTH_STATE
                    .read()
                    .as_ref()
                    .is_some_and(|state| state.adapter.name() == name);
                if is_ours {
                    tracing::warn!("bluetooth adapter '{name}' disappeared");
                    *BLUETOOTH_STATE.write() = None;
                    // switch to any other adapter that's still present
                    self.reinitialize_adapter().await;
                }
            }
            BluetoothEvent::Session(SessionEvent::AdapterAdded(name)) => {
                if BLUETOOTH_STATE.read().is_none() {
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
                if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
                    state.devices.remove(&address);
                }
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
            BluetoothCommand::SetPowered(powered) => match self.adapter() {
                Some(adapter) => actions::spawn_set_powered(adapter, powered),
                None => tracing::warn!("can't toggle bluetooth: no adapter available"),
            },
            BluetoothCommand::Device(address, action) => match self.adapter() {
                Some(adapter) => actions::spawn_device_action(adapter, address, action),
                None => tracing::warn!("can't {action:?} {address}: no adapter available"),
            },
        }
    }

    /// The adapter currently in use, if any.
    fn adapter(&self) -> Option<bluer::Adapter> {
        BLUETOOTH_STATE
            .read()
            .as_ref()
            .map(|state| state.adapter.clone())
    }

    /// Self-heals anything a missed event left stale.
    ///
    /// A failed agent registration (e.g. bluetoothd wasn't up yet) is
    /// retried, and a missed `AdapterAdded` is healed by looking for an
    /// adapter again.
    async fn reconcile(&mut self, reason: &str) {
        tracing::debug!("reconciling bluetooth state: {reason}");
        ensure_agent(&self.session, &mut self.agent).await;
        if BLUETOOTH_STATE.read().is_none() {
            self.reinitialize_adapter().await;
        } else {
            self.reconcile_devices().await;
        }
    }

    /// Stops every subscription tied to the current adapter.
    fn stop_adapter_tasks(&mut self) {
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

    /// (Re-)establishes [`BLUETOOTH_STATE`] and its watcher tasks against the
    /// session's current default adapter, or clears state to `None` if none
    /// is currently available.
    ///
    /// Called at startup and whenever the tracked adapter goes away, since a
    /// restarted bluez or a replugged adapter needs fresh subscriptions.
    async fn reinitialize_adapter(&mut self) {
        self.stop_adapter_tasks();

        let adapter = match self.session.default_adapter().await {
            Ok(adapter) => adapter,
            Err(e) => {
                tracing::warn!("no bluetooth adapter currently available: {e}");
                *BLUETOOTH_STATE.write() = None;
                return;
            }
        };

        // subscribe before snapshotting so devices and changes that appear in
        // between are delivered as events instead of being lost
        match adapter.events().await {
            Ok(mut adapter_events) => {
                let event_tx = self.event_tx.clone();
                let handle = relm4::spawn(async move {
                    while let Some(event) = adapter_events.next().await {
                        if event_tx.send(BluetoothEvent::Adapter(event)).is_err() {
                            break;
                        }
                    }
                    tracing::warn!("bluetooth service has stopped receiving adapter events");
                });
                self.adapter_events_task = Some(handle.abort_handle());
            }
            Err(e) => tracing::error!("couldn't subscribe to bluetooth adapter events: {e}"),
        }

        let mut devices = HashMap::new();
        for address in adapter.device_addresses().await.unwrap_or_default() {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            self.subscribe_device(address, device.clone()).await;
            match build_device_info(&device).await {
                Ok(info) => {
                    devices.insert(address, info);
                }
                Err(e) => tracing::debug!("couldn't read bluetooth device {address}: {e}"),
            }
        }

        let state = BluetoothState {
            powered: adapter.is_powered().await.unwrap_or(false),
            devices,
            discovering: adapter.is_discovering().await.unwrap_or(false),
            adapter,
        };

        *BLUETOOTH_STATE.write() = Some(state);
    }

    /// Fetches a full property snapshot for a newly discovered device and
    /// inserts it into [`BLUETOOTH_STATE`].
    async fn add_device(&mut self, address: Address) {
        let Some(adapter) = BLUETOOTH_STATE.read().as_ref().map(|s| s.adapter.clone()) else {
            return;
        };
        let Ok(device) = adapter.device(address) else {
            return;
        };

        // subscribe before fetching so a change in between isn't lost
        self.subscribe_device(address, device.clone()).await;

        match build_device_info(&device).await {
            Ok(info) => {
                if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
                    state.devices.insert(address, info);
                }
            }
            Err(e) => tracing::debug!("couldn't read new bluetooth device {address}: {e}"),
        }
    }

    /// Re-polls adapter state and rebuilds the device list from the
    /// adapter's own.
    ///
    /// The adapter's device list is the source of truth: devices it no
    /// longer knows are dropped along with their subscriptions, devices we
    /// never heard about are subscribed, and devices that can't be read are
    /// skipped.
    async fn reconcile_devices(&mut self) {
        let Some(adapter) = BLUETOOTH_STATE.read().as_ref().map(|s| s.adapter.clone()) else {
            return;
        };

        let addresses = match adapter.device_addresses().await {
            Ok(addresses) => addresses,
            Err(e) => {
                tracing::warn!("couldn't list bluetooth devices: {e}");
                return;
            }
        };

        self.device_event_tasks.retain(|address, handle| {
            let known = addresses.contains(address);
            if !known {
                handle.abort();
            }
            known
        });

        let mut devices = HashMap::with_capacity(addresses.len());
        for address in addresses {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            if !self.device_event_tasks.contains_key(&address) {
                self.subscribe_device(address, device.clone()).await;
            }
            match build_device_info(&device).await {
                Ok(info) => {
                    devices.insert(address, info);
                }
                Err(e) => tracing::debug!("couldn't read bluetooth device {address}: {e}"),
            }
        }

        let powered = adapter.is_powered().await.unwrap_or(false);
        let discovering = adapter.is_discovering().await.unwrap_or(false);

        if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
            state.powered = powered;
            state.discovering = discovering;
            state.devices = devices;
        }
    }

    /// Forwards a device's property changes into the event channel,
    /// replacing any stale subscription for the same address (e.g. it was
    /// removed and re-added) rather than leaking it.
    async fn subscribe_device(&mut self, address: Address, device: Device) {
        let mut device_events = match device.events().await {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!("couldn't subscribe to events for device {address}: {e}");
                return;
            }
        };

        let event_tx = self.event_tx.clone();
        let handle = relm4::spawn(async move {
            while let Some(event) = device_events.next().await {
                if event_tx
                    .send(BluetoothEvent::Device(address, event))
                    .is_err()
                {
                    break;
                }
            }
            tracing::debug!("bluetooth event stream ended for device {address}");
        })
        .abort_handle();

        if let Some(old_handle) = self.device_event_tasks.insert(address, handle) {
            old_handle.abort();
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

        let adapter = {
            let state = BLUETOOTH_STATE.read();
            let Some(ref state) = *state else {
                tracing::warn!("can't start discovery: no bluetooth adapter available");
                return;
            };
            state.adapter.clone()
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

/// Applies an adapter property change to [`BLUETOOTH_STATE`].
fn apply_adapter_property(property: AdapterProperty) {
    // other properties (class, uuids, discoverable, ...) aren't shown, so
    // don't wake every subscriber for them
    if !matches!(
        property,
        AdapterProperty::Powered(_) | AdapterProperty::Discovering(_)
    ) {
        tracing::trace!("ignoring adapter property change: {property:?}");
        return;
    }

    let mut guard = BLUETOOTH_STATE.write();
    let Some(state) = guard.as_mut() else {
        return;
    };
    match property {
        AdapterProperty::Powered(powered) => state.powered = powered,
        AdapterProperty::Discovering(discovering) => state.discovering = discovering,
        _ => {}
    }
}

/// Applies a device property change to [`BLUETOOTH_STATE`].
fn apply_device_property(address: Address, property: DeviceProperty) {
    // properties the shell doesn't show (rssi changes several times a second
    // during discovery) would otherwise wake every subscriber for nothing
    if !matches!(
        property,
        DeviceProperty::Connected(_)
            | DeviceProperty::Alias(_)
            | DeviceProperty::Icon(_)
            | DeviceProperty::Paired(_)
    ) {
        tracing::trace!("ignoring device {address} property change: {property:?}");
        return;
    }

    let mut guard = BLUETOOTH_STATE.write();
    let Some(info) = guard
        .as_mut()
        .and_then(|state| state.devices.get_mut(&address))
    else {
        tracing::debug!("property change for untracked device {address}, ignoring");
        return;
    };
    match property {
        DeviceProperty::Connected(connected) => info.connected = connected,
        DeviceProperty::Alias(alias) => info.alias = alias,
        DeviceProperty::Icon(icon) => info.icon = Some(icon),
        DeviceProperty::Paired(paired) => info.paired = paired,
        _ => {}
    }
}

/// Registers the pairing agent unless it's already registered.
///
/// If becoming the default agent fails (e.g. another agent holds that role),
/// registration is retried as a non-default agent, which still handles the
/// pairings this shell starts itself.
async fn ensure_agent(session: &Session, handle: &mut Option<bluer::agent::AgentHandle>) {
    if handle.is_some() {
        return;
    }

    let default_agent = settings::get_config().bluetooth.default_agent;
    let registered = match agent::register_agent(session, default_agent).await {
        Err(e) if default_agent => {
            tracing::warn!("couldn't become the default pairing agent, registering normally: {e}");
            agent::register_agent(session, false).await
        }
        result => result,
    };

    match registered {
        Ok(agent_handle) => {
            tracing::info!("bluetooth pairing agent registered");
            *handle = Some(agent_handle);
        }
        Err(e) => tracing::warn!("couldn't register bluetooth pairing agent: {e}"),
    }
}

/// Sends [`BluetoothEvent::BluezRestarted`] whenever `org.bluez` gets a new
/// owner.
fn spawn_bluez_owner_watcher(event_tx: UnboundedSender<BluetoothEvent>) -> AbortHandle {
    relm4::spawn(async move {
        let result: anyhow::Result<()> = async {
            let conn = zbus::Connection::system().await?;
            let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
            let mut changes = dbus.receive_name_owner_changed().await?;
            while let Some(change) = changes.next().await {
                let args = change.args()?;
                if args.name.as_str() == "org.bluez" && args.new_owner.is_some() {
                    event_tx.send(BluetoothEvent::BluezRestarted)?;
                }
            }
            Ok(())
        }
        .await;

        if let Err(e) = result {
            tracing::warn!("stopped watching for bluetoothd restarts: {e}");
        }
    })
    .abort_handle()
}

/// Watches for adapters appearing or disappearing and forwards them as
/// [`BluetoothEvent::Session`] events.
fn spawn_session_watcher(
    session: Session,
    event_tx: UnboundedSender<BluetoothEvent>,
) -> AbortHandle {
    relm4::spawn(async move {
        let stream = match session.events().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("couldn't subscribe to bluetooth session events: {e}");
                return;
            }
        };
        // Session::events()'s stream isn't Unpin, unlike Adapter/Device's;
        // pin it to a stack slot so StreamExt::next() can be called on it
        let mut stream = std::pin::pin!(stream);

        while let Some(event) = stream.next().await {
            tracing::debug!("bluetooth session event: {event:?}");
            if event_tx.send(BluetoothEvent::Session(event)).is_err() {
                break;
            }
        }
        tracing::warn!("stream for bluetooth session events has closed");
    })
    .abort_handle()
}

/// Fetches a full property snapshot for a device.
///
/// Fails if the device can't be read (usually because bluez forgot it), so
/// callers can drop it instead of showing made-up defaults.
async fn build_device_info(device: &Device) -> bluer::Result<DeviceInfo> {
    Ok(DeviceInfo {
        address: device.address(),
        alias: device.alias().await?,
        icon: device.icon().await?,
        paired: device.is_paired().await?,
        connected: device.is_connected().await?,
    })
}

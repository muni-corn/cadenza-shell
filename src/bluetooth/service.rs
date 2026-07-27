use std::{collections::HashMap, time::Duration};

use bluer::{
    Adapter, AdapterEvent, AdapterProperty, Address, Device, DeviceEvent, DeviceProperty, Session,
    SessionEvent,
};
use futures_lite::StreamExt;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::{
    bluetooth::{
        agent,
        commands::{self, BluetoothCommand},
        state::{BLUETOOTH_STATE, BluetoothState, DeviceInfo},
    },
    settings, sleep_monitor,
};

#[derive(Debug)]
pub enum BluetoothEvent {
    Adapter(AdapterEvent),
    Device(Address, DeviceEvent),
    /// An adapter appeared or disappeared on the bus (e.g. bluetoothd
    /// restarting, a USB dongle being unplugged/replugged, or rfkill being
    /// toggled).
    Session(SessionEvent),
    /// The system has just woken from sleep; triggers a full state refresh.
    Wake,
    /// Periodic full refetch, independent of any BlueZ event. Self-heals
    /// state that a missed event would otherwise leave stale forever, the
    /// same way battery::watcher's poll interval does.
    Reconcile,
    /// A command issued by the UI (via `bluetooth::commands`).
    Command(BluetoothCommand),
}

pub async fn run_bluetooth_service() {
    let Ok(session) = Session::new()
        .await
        .inspect_err(|e| tracing::error!("couldn't initialize bluetooth session: {e}"))
    else {
        return;
    };

    // register our pairing agent; the handle must be held for the agent to
    // stay registered, so it's kept alive for the rest of this function
    // (which runs for the life of the service)
    let default_agent = settings::get_config().bluetooth.default_agent;
    let _agent_handle = match agent::register_agent(&session, default_agent).await {
        Ok(handle) => {
            tracing::info!(default_agent, "bluetooth pairing agent registered");
            Some(handle)
        }
        Err(e) => {
            tracing::error!("couldn't register bluetooth pairing agent: {e}");
            None
        }
    };

    let (event_tx, mut event_rx) = unbounded_channel();

    // watch for adapters appearing or disappearing on the bus; this task
    // itself never needs to be respawned, since it's subscribed against the
    // session (bluetoothd's D-Bus connection at the session level), not any
    // particular adapter
    spawn_session_watcher(session.clone(), event_tx.clone());

    // subscribe to system wake events and forward them into the event channel
    let mut wake_rx = sleep_monitor::subscribe_wake();
    let event_tx_wake = event_tx.clone();
    relm4::spawn(async move {
        loop {
            match wake_rx.recv().await {
                Ok(()) => {
                    event_tx_wake
                        .send(BluetoothEvent::Wake)
                        .unwrap_or_else(|e| tracing::error!("couldn't send wake event: {e}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("bluetooth wake receiver lagged, missed {n} wake event(s)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // periodically reconcile full state so a missed BlueZ event never
    // leaves the tile stale indefinitely
    let event_tx_reconcile = event_tx.clone();
    relm4::spawn(async move {
        let interval_secs = settings::get_config().bluetooth.reconcile_interval;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        // skip the first tick, which fires immediately; initial state is
        // already fetched separately below
        interval.tick().await;
        loop {
            interval.tick().await;
            event_tx_reconcile
                .send(BluetoothEvent::Reconcile)
                .unwrap_or_else(|e| tracing::error!("couldn't send reconcile tick: {e}"));
        }
    });

    // install the command sender so free functions in bluetooth::commands
    // can push commands, and forward them into the same event channel as
    // everything else
    let (cmd_tx, mut cmd_rx) = unbounded_channel::<BluetoothCommand>();
    if commands::install_command_sender(cmd_tx).is_err() {
        tracing::warn!("bluetooth service started more than once; extra instance exiting");
        return;
    }
    let event_tx_cmd = event_tx.clone();
    relm4::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            event_tx_cmd
                .send(BluetoothEvent::Command(cmd))
                .unwrap_or_else(|e| tracing::error!("couldn't forward bluetooth command: {e}"));
        }
        tracing::warn!("bluetooth command channel closed");
    });

    let mut adapter_events_task: Option<tokio::task::AbortHandle> = None;
    let mut device_event_tasks: HashMap<Address, tokio::task::AbortHandle> = HashMap::new();
    let mut discovery_task: Option<tokio::task::AbortHandle> = None;

    reinitialize_adapter(
        &session,
        &event_tx,
        &mut adapter_events_task,
        &mut device_event_tasks,
    )
    .await;

    while let Some(event) = event_rx.recv().await {
        match event {
            BluetoothEvent::Session(SessionEvent::AdapterRemoved(name)) => {
                let is_ours = BLUETOOTH_STATE
                    .read()
                    .as_ref()
                    .is_some_and(|state| state.adapter.name() == name);
                if is_ours {
                    tracing::warn!("bluetooth adapter '{name}' disappeared, clearing state");
                    if let Some(handle) = adapter_events_task.take() {
                        handle.abort();
                    }
                    for (_, handle) in device_event_tasks.drain() {
                        handle.abort();
                    }
                    if let Some(handle) = discovery_task.take() {
                        handle.abort();
                    }
                    *BLUETOOTH_STATE.write() = None;
                }
            }
            BluetoothEvent::Session(SessionEvent::AdapterAdded(name)) => {
                let have_adapter = BLUETOOTH_STATE.read().is_some();
                if !have_adapter {
                    tracing::info!("bluetooth adapter '{name}' available, initializing");
                    reinitialize_adapter(
                        &session,
                        &event_tx,
                        &mut adapter_events_task,
                        &mut device_event_tasks,
                    )
                    .await;
                }
            }
            BluetoothEvent::Command(cmd) => {
                handle_command(cmd, &mut discovery_task).await;
            }
            other => update(other, &event_tx, &mut device_event_tasks).await,
        }
    }
    tracing::warn!("bluetooth service has stopped receiving events");
}

/// Dispatches a single UI-issued command to its `bluer` handler, logging on
/// failure. Failures are not fatal to the service; the user can simply
/// retry from the menu.
async fn handle_command(
    cmd: BluetoothCommand,
    discovery_task: &mut Option<tokio::task::AbortHandle>,
) {
    match cmd {
        BluetoothCommand::StartDiscovery => {
            handle_start_discovery(discovery_task).await;
        }
        BluetoothCommand::StopDiscovery => {
            if let Some(handle) = discovery_task.take() {
                handle.abort();
            }
        }
        BluetoothCommand::Pair(address) => {
            handle_pair(address);
        }
        BluetoothCommand::Remove(address) => {
            if let Err(e) = handle_remove(address).await {
                tracing::warn!("couldn't remove device {address}: {e}");
            }
        }
        BluetoothCommand::PairingReply(reply) => {
            agent::respond(reply);
        }
        BluetoothCommand::CancelPairing => {
            agent::cancel();
        }
    }
}

/// Starts a discovery session, holding it open (and thus BlueZ's
/// `Discovering` state true) until `bluetooth.discovery_timeout` elapses or
/// it's cancelled by a `StopDiscovery` command. A no-op if one is already
/// running.
///
/// `Adapter::discover_devices`'s returned stream doesn't need to be read for
/// us to learn about discovered devices - the adapter-wide event
/// subscription already established in `start_event_listening` picks those
/// up via the normal `DeviceAdded` path - but the stream (and the discovery
/// session token it holds internally) must stay alive for discovery to
/// remain active, so this task exists purely to hold and eventually drop it.
async fn handle_start_discovery(discovery_task: &mut Option<tokio::task::AbortHandle>) {
    if discovery_task.is_some() {
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

    *discovery_task = Some(handle);
}

/// Initiates pairing with a device, spawned rather than awaited inline:
/// pairing can take many seconds while it waits on the user to answer a PIN
/// or passkey prompt, and awaiting it directly in the command loop would
/// block every other bluetooth event (property updates, discovery, etc.)
/// for that whole duration.
fn handle_pair(address: Address) {
    let adapter = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            tracing::warn!("can't pair with {address}: no bluetooth adapter available");
            return;
        };
        state.adapter.clone()
    };

    relm4::spawn(async move {
        let device = match adapter.device(address) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("couldn't build device handle for {address}: {e}");
                return;
            }
        };

        match device.pair().await {
            Ok(()) => {
                tracing::info!("paired with {address}");
                // trusting a device we just paired with is the common
                // convention (lets it reconnect without re-authorizing)
                if let Err(e) = device.set_trusted(true).await {
                    tracing::warn!("paired with {address} but couldn't set it trusted: {e}");
                }
            }
            Err(e) => tracing::warn!("pairing with {address} failed: {e}"),
        }
    });
}

async fn handle_remove(address: Address) -> anyhow::Result<()> {
    let adapter = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            anyhow::bail!("no bluetooth adapter available");
        };
        state.adapter.clone()
    };

    adapter.remove_device(address).await?;
    Ok(())
}

/// Watches for adapters appearing or disappearing and forwards them as
/// [`BluetoothEvent::Session`] events.
fn spawn_session_watcher(session: Session, event_tx: UnboundedSender<BluetoothEvent>) {
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
            event_tx
                .send(BluetoothEvent::Session(event))
                .unwrap_or_else(|e| tracing::error!("couldn't send session event: {e}"));
        }
        tracing::warn!("stream for bluetooth session events has closed");
    });
}

/// (Re-)establishes [`BLUETOOTH_STATE`] and its watcher tasks against the
/// session's current default adapter, or clears state to `None` if none is
/// currently available.
///
/// Called at startup and again whenever the tracked adapter disappears and a
/// (possibly different) one becomes available, since BlueZ restarting or a
/// USB adapter being replugged gets a fresh D-Bus object with fresh
/// subscriptions required.
async fn reinitialize_adapter(
    session: &Session,
    event_tx: &UnboundedSender<BluetoothEvent>,
    adapter_events_task: &mut Option<tokio::task::AbortHandle>,
    device_event_tasks: &mut HashMap<Address, tokio::task::AbortHandle>,
) {
    let Ok(adapter) = session.default_adapter().await.inspect_err(|e| {
        tracing::warn!("no bluetooth adapter currently available: {e}");
    }) else {
        *BLUETOOTH_STATE.write() = None;
        return;
    };

    let mut devices = HashMap::new();
    if let Ok(addresses) = adapter.device_addresses().await {
        for address in addresses {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            devices.insert(address, build_device_info(&device).await);
        }
    }

    let state = BluetoothState {
        _session: session.clone(),
        powered: adapter.is_powered().await.unwrap_or(false),
        devices,
        discovering: adapter.is_discovering().await.unwrap_or(false),
        adapter: adapter.clone(),
    };

    *BLUETOOTH_STATE.write() = Some(state);

    match start_event_listening(adapter, event_tx).await {
        Ok((new_adapter_events_task, new_device_event_tasks)) => {
            *adapter_events_task = Some(new_adapter_events_task);
            *device_event_tasks = new_device_event_tasks;
        }
        Err(e) => {
            tracing::error!("failed to setup bluetooth monitoring: {e}");
        }
    }
}

async fn start_event_listening(
    adapter: Adapter,
    event_tx: &UnboundedSender<BluetoothEvent>,
) -> anyhow::Result<(
    tokio::task::AbortHandle,
    HashMap<Address, tokio::task::AbortHandle>,
)> {
    // monitor adapter events
    let mut adapter_events = adapter.events().await?;
    let event_tx_clone = event_tx.clone();
    let adapter_events_task = relm4::spawn(async move {
        while let Some(event) = adapter_events.next().await {
            event_tx_clone
                .send(BluetoothEvent::Adapter(event))
                .unwrap_or_else(|e| tracing::error!("couldn't send adapter bluetooth event: {e}"));
        }
        tracing::warn!("bluetooth service has stopped receiving adapter events");
    })
    .abort_handle();

    // monitor existing devices for connection status changes
    let mut device_event_tasks = HashMap::new();
    let devices = adapter.device_addresses().await.unwrap_or_default();
    for addr in devices {
        if let Ok(device) = adapter.device(addr)
            && let Some(handle) = subscribe_device_events(addr, device, event_tx).await
        {
            device_event_tasks.insert(addr, handle);
        }
    }

    Ok((adapter_events_task, device_event_tasks))
}

async fn update(
    input: BluetoothEvent,
    event_tx: &UnboundedSender<BluetoothEvent>,
    device_event_tasks: &mut HashMap<Address, tokio::task::AbortHandle>,
) {
    match input {
        BluetoothEvent::Wake => {
            tracing::debug!("system wake: refreshing bluetooth state");
            reconcile_state().await;
        }
        BluetoothEvent::Reconcile => {
            tracing::debug!("periodic reconcile: refreshing bluetooth state");
            reconcile_state().await;
        }
        // a newly added device needs its full property snapshot fetched
        // before it can be inserted, which requires an async round trip
        // that update_from_event's synchronous write-lock section can't do
        BluetoothEvent::Adapter(AdapterEvent::DeviceAdded(address)) => {
            handle_device_added(address, event_tx, device_event_tasks).await;
        }
        // abort the now-orphaned event subscription before applying the
        // sync state removal; otherwise the task leaks and keeps forwarding
        // events for a device that's no longer tracked
        BluetoothEvent::Adapter(AdapterEvent::DeviceRemoved(address)) => {
            if let Some(handle) = device_event_tasks.remove(&address) {
                handle.abort();
            }
            update_from_event(BluetoothEvent::Adapter(AdapterEvent::DeviceRemoved(
                address,
            )));
        }
        other => {
            // sync, so the write lock is always released before any
            // subsequent async subscription work
            update_from_event(other);
        }
    }
}

/// Fetches a full property snapshot for a newly discovered device, inserts
/// it into [`BLUETOOTH_STATE`], and subscribes to its future property
/// changes.
async fn handle_device_added(
    address: Address,
    event_tx: &UnboundedSender<BluetoothEvent>,
    device_event_tasks: &mut HashMap<Address, tokio::task::AbortHandle>,
) {
    let adapter = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            return;
        };
        state.adapter.clone()
    };

    let Ok(device) = adapter.device(address) else {
        return;
    };
    let info = build_device_info(&device).await;

    {
        let mut guard = BLUETOOTH_STATE.write();
        if let Some(ref mut state) = *guard {
            state.devices.insert(address, info);
        }
    }

    // replace any stale subscription for this address (e.g. it was
    // previously removed and re-added) rather than leaking it
    if let Some(handle) = subscribe_device_events(address, device, event_tx).await
        && let Some(old_handle) = device_event_tasks.insert(address, handle)
    {
        old_handle.abort();
    }
}

/// Fetches a full property snapshot for a device.
async fn build_device_info(device: &Device) -> DeviceInfo {
    DeviceInfo {
        address: device.address(),
        alias: device
            .alias()
            .await
            .unwrap_or_else(|_| device.address().to_string()),
        icon: device.icon().await.unwrap_or(None),
        paired: device.is_paired().await.unwrap_or(false),
        trusted: device.is_trusted().await.unwrap_or(false),
        connected: device.is_connected().await.unwrap_or(false),
        rssi: device.rssi().await.unwrap_or(None),
        battery_percentage: device.battery_percentage().await.unwrap_or(None),
    }
}

/// Re-polls adapter powered/discovering state and refreshes every device's
/// property snapshot, self-healing anything a missed BlueZ event would
/// otherwise have left stale. Used both after a system wake and on the
/// periodic reconcile tick.
async fn reconcile_state() {
    // clone the adapter and known addresses without holding the write lock
    // across the async re-fetch below
    let (adapter, addresses) = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            return;
        };
        (
            state.adapter.clone(),
            state.devices.keys().copied().collect::<Vec<_>>(),
        )
    };

    let powered = adapter.is_powered().await.unwrap_or(false);
    let discovering = adapter.is_discovering().await.unwrap_or(false);

    let mut devices = HashMap::with_capacity(addresses.len());
    for address in addresses {
        let Ok(device) = adapter.device(address) else {
            continue;
        };
        devices.insert(address, build_device_info(&device).await);
    }

    if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
        state.powered = powered;
        state.discovering = discovering;
        state.devices = devices;
    }
}

/// Applies a bluetooth event to [`BLUETOOTH_STATE`] synchronously.
///
/// Handles everything except newly-added devices, which need an async
/// property fetch before they can be snapshotted (see
/// [`handle_device_added`]).
fn update_from_event(input: BluetoothEvent) {
    let mut guard = BLUETOOTH_STATE.write();
    let Some(state) = (*guard).as_mut() else {
        return;
    };

    tracing::debug!("updating bluetooth state with event: {:?}", input);

    match input {
        BluetoothEvent::Adapter(adapter_event) => match adapter_event {
            AdapterEvent::DeviceAdded(_) => {
                unreachable!("DeviceAdded is handled by handle_device_added")
            }
            AdapterEvent::DeviceRemoved(address) => {
                state.devices.remove(&address);
            }
            AdapterEvent::PropertyChanged(adapter_property) => match adapter_property {
                AdapterProperty::Powered(p) => state.powered = p,
                AdapterProperty::Discovering(d) => state.discovering = d,
                p => tracing::warn!("unhandled AdapterProperty event: {p:?}"),
            },
        },
        // wake, reconcile, session, and command events are all handled
        // before this function is called
        BluetoothEvent::Wake
        | BluetoothEvent::Reconcile
        | BluetoothEvent::Session(_)
        | BluetoothEvent::Command(_) => {}
        BluetoothEvent::Device(address, device_event) => {
            let Some(info) = state.devices.get_mut(&address) else {
                tracing::debug!("property change for untracked device {address}, ignoring");
                return;
            };

            match device_event {
                DeviceEvent::PropertyChanged(DeviceProperty::Connected(connected)) => {
                    info.connected = connected;
                    tracing::debug!("device {address} connected={connected}");
                }
                DeviceEvent::PropertyChanged(DeviceProperty::Alias(alias)) => info.alias = alias,
                DeviceEvent::PropertyChanged(DeviceProperty::Icon(icon)) => info.icon = Some(icon),
                DeviceEvent::PropertyChanged(DeviceProperty::Paired(paired)) => {
                    info.paired = paired
                }
                DeviceEvent::PropertyChanged(DeviceProperty::Trusted(trusted)) => {
                    info.trusted = trusted
                }
                DeviceEvent::PropertyChanged(DeviceProperty::Rssi(rssi)) => info.rssi = Some(rssi),
                DeviceEvent::PropertyChanged(DeviceProperty::BatteryPercentage(pct)) => {
                    info.battery_percentage = Some(pct)
                }
                DeviceEvent::PropertyChanged(device_property) => {
                    tracing::debug!("device {address} property changed: {device_property:?}");
                }
            }
        }
    }
}

/// Subscribes to BlueZ property change events for a device and forwards them
/// into the shared event channel.
///
/// Returns the spawned task's abort handle so the caller can cancel it once
/// the device is removed, avoiding a task leak that would otherwise keep a
/// dead subscription running (and, since bluer's BlueZ object paths can be
/// reused, potentially forwarding events for a stale device instance).
async fn subscribe_device_events(
    address: Address,
    device: Device,
    event_tx: &UnboundedSender<BluetoothEvent>,
) -> Option<tokio::task::AbortHandle> {
    match device.events().await {
        Ok(mut device_events) => {
            let tx = event_tx.clone();
            let handle = relm4::spawn(async move {
                while let Some(event) = device_events.next().await {
                    tx.send(BluetoothEvent::Device(address, event))
                        .unwrap_or_else(|e| {
                            tracing::error!(
                                "couldn't send device bluetooth event for {address}: {e}"
                            )
                        });
                }
                tracing::warn!("bluetooth event stream ended for device {address}");
            });
            Some(handle.abort_handle())
        }
        Err(e) => {
            tracing::warn!("couldn't subscribe to events for device {address}: {e}");
            None
        }
    }
}

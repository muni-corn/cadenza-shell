# Housekeeping plan

This plan collects the results of a full review of cadenza-shell (bugs, dead
code, overengineering and structural duplication) and turns them into a
sequence of atomic commits, grouped into phases.

## Decisions

These were settled with muni before the plan was written:

- **Scope:** everything, including the new features listed in the last phase.
- **Battery estimate:** replace the weekly Fourier model in
  `src/battery/discharging.rs` with a short and long time-based power EMA. Drop
  persistence entirely; leave any existing `discharge_profile.json` on disk.
- **mpris:** port from the blocking `mpris` crate to zbus.
- **Settings:** wire up fields that duplicate hardcoded values
  (`notifications.timeout`, `notifications.popup_width`,
  `notifications.center_width`, `tiles.icon_size`), delete the remaining unread
  fields, and shrink `ConfigManager` to a static config.
- **Notification capabilities:** implement every advertised capability
  (`body-markup`, `body-hyperlinks`, `body-images`, `action-icons`,
  `icon-static`, `sound`) instead of trimming the list.
- **Weather TLS:** rustls.
- **Action panel:** the bell button only opens the action panel on the monitor
  it was clicked on.
- **Branching:** one git worktree and branch per phase.

## Workflow

### Worktrees and branches

Each phase gets its own worktree and branch. Phases stack: each branch starts
from the previous phase's branch, so later phases can build on earlier fixes
without waiting for a merge. If an earlier phase has already been merged into
`main`, start from `main` instead.

```bash
# from /home/muni/code/cadenza/cadenza_shell
git worktree add ../cadenza_shell-phase-1 -b housekeeping/phase-1-crashes main
git worktree add ../cadenza_shell-phase-2 -b housekeeping/phase-2-hotplug housekeeping/phase-1-crashes
# ...and so on for each phase
```

| Phase | Branch                                   | Worktree                   |
| ----- | ---------------------------------------- | -------------------------- |
| 1     | `housekeeping/phase-1-crashes`           | `../cadenza_shell-phase-1` |
| 2     | `housekeeping/phase-2-hotplug`           | `../cadenza_shell-phase-2` |
| 3     | `housekeeping/phase-3-async-services`    | `../cadenza_shell-phase-3` |
| 4     | `housekeeping/phase-4-dead-code`         | `../cadenza_shell-phase-4` |
| 5     | `housekeeping/phase-5-network-bluetooth` | `../cadenza_shell-phase-5` |
| 6     | `housekeeping/phase-6-services`          | `../cadenza_shell-phase-6` |
| 7     | `housekeeping/phase-7-notification-caps` | `../cadenza_shell-phase-7` |
| 8     | `housekeeping/phase-8-structure`         | `../cadenza_shell-phase-8` |
| 9     | `housekeeping/phase-9-features`          | `../cadenza_shell-phase-9` |

If a later phase branch needs fixes from a rewritten earlier branch, rebase it
onto the new tip rather than merging.

### Verifying each commit

Every commit must build, pass clippy without new warnings, and pass tests:

```bash
SECRETSPEC_REASON="building and testing cadenza-shell" devenv shell cargo clippy --all-targets
SECRETSPEC_REASON="building and testing cadenza-shell" devenv shell cargo test
```

From the end of phase 4 onwards, the tree should be warning-free, so use
`cargo clippy --all-targets -- -D warnings`.

Commits marked **(manual check)** change behaviour that can only be confirmed
on a live niri session (monitor hotplug, layer-shell placement, pairing, wifi).
Run the shell from the worktree and exercise the described scenario before
moving on.

### Rules for implementers

- Never mention phases, steps or other commits in code comments.
- Doc comments use sentence case; inline comments are lowercase and explain
  why.
- Keep files under 500 lines. New code goes in new, focused modules.
- Use `cargo add` and `cargo remove` for dependencies. `cargo add` can't drop a
  feature, so removing a feature is the one case where the `features` array may
  be edited by hand.
- Don't add `#[allow(dead_code)]`. Items introduced by one commit and consumed
  by the next may warn in between; that's acceptable within a series, but the
  last commit of the series must leave no warnings.
- Never amend commits. If a formatter hook rewrites files, re-add them and
  commit again.

## Phase 1: crashes, hangs and quick fixes

### 1.1 Reconnect when the niri event socket closes

- `src/niri.rs`: replace `while reader.read_line(..).await.is_ok()` with a loop
  that breaks on `Ok(0)` or `Err`, then falls through to the existing sleep and
  reconnect.
- Read and check the first line as a `niri_ipc::Reply` before parsing events.

```text
fix(niri): reconnect when the event socket closes

- at EOF, read_line returns Ok(0) forever, so the loop spun at 100% CPU,
  logged a parse error per iteration, and never reached the reconnect
- the first line of the stream is the request's reply, not an event, so
  it was logging a spurious parse error on every connect
```

### 1.2 Stop the progress tile fade timer from panicking

- `src/widgets/progress_tile.rs`: replace `fade_timeout_source: Option<SourceId>`
  with a `fade_generation: u64`.
- `activate()` bumps the generation and starts
  `sender.oneshot_command(async move { sleep(3s).await; generation })`;
  `update_cmd` ignores stale generations.

```text
fix(widgets): stop the progress tile fade timer from panicking

- SourceId::remove() unwraps g_source_remove, which fails once the
  timeout has already fired and queued FadeTimeout
- the glib timer also called ComponentSender::input, which panics after
  the tile shuts down (e.g. on monitor unplug), aborting the process
- relm4 commands are cancelled with the component and need no manual
  removal
```

### 1.3 Stop the popup auto-dismiss timer from panicking

- `src/notifications/fresh.rs`: same approach as 1.2, keyed per notification:
  `HashMap<u32, u64>` of generations, `oneshot_command` returning
  `(id, generation)`, ignore stale or unknown ids.

```text
fix(notifications): stop the popup auto-dismiss timer from panicking

- removing a SourceId whose timeout already fired panics, which happened
  when a close event raced the 10 second popup timer
- relm4 commands avoid both that and sending input after shutdown
```

### 1.4 Clamp polling intervals to at least one second

- `src/settings.rs`: after loading, clamp `network.scan_interval`,
  `network.reconcile_interval` and `bluetooth.reconcile_interval` to `>= 1`,
  logging a warning when a value was raised.

```text
fix(settings): clamp polling intervals to at least one second

- tokio::time::interval panics on a zero period, so a 0 in the config
  file crashed the network and bluetooth services
```

### 1.5 Exit with a success code

- `src/main.rs`: return `glib::ExitCode::SUCCESS`.

```text
fix(main): exit with a success code

- main returned FAILURE unconditionally, so a clean quit looked like a
  crash to systemd units using Restart=on-failure
```

### 1.6 Drop the unused tokio runtime

- `src/main.rs`: replace `#[tokio::main] async fn main` with `fn main`.
- Call `gtk4::init()` explicitly before `relm4::set_global_css`, which unwraps
  `Display::default()`.

```text
refactor(main): drop the unused tokio runtime

- relm4 runs commands on its own runtime, so the #[tokio::main] runtime
  only started idle worker threads
- set_global_css needs an initialized display; it only worked because
  relm4_icons happened to initialize gtk first
```

### 1.7 Stop the tray event loop when its channel closes

- `src/app.rs`: match `RecvError::Lagged` (log and continue) and
  `RecvError::Closed` (break), like the notification forwarder does.

```text
fix(app): stop the tray event loop when its channel closes

- a closed broadcast channel returned an error on every recv, turning
  the loop into a hot loop that flooded the log
```

### 1.8 Reset the scanning flag when a scan fails early

- `src/network/service.rs`: move the body of `handle_scan` into an inner
  `async fn` and always reset `WIFI_SCAN_STATE.scanning` afterwards.

```text
fix(network): reset the scanning flag when a scan fails early

- any early return (no wifi device, proxy errors) left scanning set
  forever, keeping "Scanning..." visible and the rescan button disabled
- machines without wifi hit this every time the menu opened
```

### 1.9 Allow discovery to restart after it ends

- `src/bluetooth/service.rs`: in `handle_start_discovery`, treat a stored
  handle as active only if `!handle.is_finished()`.

```text
fix(bluetooth): allow discovery to restart after it ends

- the discovery task ends on its own after the timeout or when bluez
  stops discovering, but its handle stayed in the slot, so every later
  StartDiscovery was ignored for the rest of the session
```

### 1.10 Stop the wifi switch from re-triggering itself

- `src/network_menu.rs`: keep the `SignalHandlerId` from `connect_state_set`;
  in `update_view`, only call `set_active` when the value differs, wrapped in
  `block_signal` and `unblock_signal`.

```text
fix(network-menu): stop the wifi switch from re-triggering itself

- gtk_switch_set_active emits state-set, so syncing the switch from
  state sent ToggleWifi again, and a stale state update could undo the
  user's toggle
```

### 1.11 Stop the bluetooth power switch from re-triggering itself

- `src/bluetooth_menu.rs`: same signal blocking as 1.10.
- Return `glib::Propagation::Proceed` so the switch's state follows the user.
- Ignore `ToggleBluetooth(v)` when `v` already matches `powered`.

```text
fix(bluetooth-menu): stop the power switch from re-triggering itself

- update_view called set_active after every message, which emitted
  state-set and sent the opposite toggle, so turning bluetooth off
  usually turned it back on
- returning Propagation::Stop without calling set_state left the
  switch's visual state stuck
```

## Phase 2: windows and timers on hotplug

### 2.1 Destroy menu windows on shutdown **(manual check)**

- `src/widgets/menu_window.rs`: implement `shutdown` and call
  `widgets.window.destroy()`.

```text
fix(widgets): destroy menu windows on shutdown

- dropping a relm4 controller only signals shutdown; it doesn't destroy
  the root window, so a visible menu survived its bar being removed
  and migrated to another output
```

### 2.2 Destroy notification panel windows on shutdown

- `src/notifications/panel.rs` and `src/notifications/fresh.rs`: implement
  `shutdown` and destroy the window.

```text
fix(notifications): destroy panel windows on shutdown

- the action panel and popup windows outlived their owners after a
  monitor was unplugged, for the same reason as the menu windows
```

### 2.3 Tie the clock loop to the tile's lifetime

- `src/tiles/clock.rs`: replace `relm4::spawn` with
  `sender.command(|out, shutdown| shutdown.register(..).drop_on_shutdown())`.
- Sleep until the next minute boundary instead of ticking twice a second.

```text
fix(tiles): tie the clock loop to the tile's lifetime

- the loop was spawned with relm4::spawn and never stopped, so every
  removed bar left a task logging "receiver was dropped" twice a second
- the labels only show minutes, so waking at the minute boundary is
  enough
```

### 2.4 Stop the action panel clock when the panel is dropped

- `src/notifications/panel.rs`: hold weak references to the labels in the
  timeout closure and return `ControlFlow::Break` once they're gone. Store the
  `SourceId` and remove it in `shutdown`.

```text
fix(notifications): stop the action panel clock when the panel is dropped

- the one second timer held strong label references and ran forever,
  leaking one timer and its widgets per removed bar
```

### 2.5 Redraw the analog clock with a tick callback

- `src/analog_clock.rs`: replace the 100 ms `timeout_add_local` with
  `drawing_area.add_tick_callback`, redrawing only when the displayed second
  changes.

```text
fix(analog-clock): redraw with a tick callback

- the timer redrew ten times a second on every monitor, even while the
  action panel was hidden, and kept the drawing area alive forever
- tick callbacks only run while the widget is mapped and die with it
```

### 2.6 Honour the analog clock radius passed by the caller

- `src/analog_clock.rs`: use the `Init` radius instead of reading
  `tiles.analog_clock_radius`; remove the empty message enum.
- `src/notifications/panel.rs`: pass `tiles.analog_clock_radius` from settings.

```text
fix(analog-clock): honour the radius passed by the caller

- the init radius was ignored in favour of the config value, so the
  caller's hardcoded 32 and the config's 60 disagreed silently
- the panel now passes the configured radius explicitly
```

### 2.7 Tie network menu loops to the menu's lifetime

- `src/network_menu.rs`: replace both `relm4::spawn` loops with
  `sender.command` plus shutdown.
- Only run the rescan ticker between `SetOpen(true)` and `SetOpen(false)`.

```text
fix(network-menu): tie scan loops to the menu's lifetime

- the loops called sender.input after the menu was dropped, which
  panicked inside the task
- the rescan ticker kept requesting scans while the menu was closed
```

### 2.8 Configure layer shell before the window is realized **(manual check)**

- `src/widgets/bar.rs`: build the window with `.visible(false)` in `init_root`,
  configure layer shell in `init`, then `window.present()`. Remove the
  `is_layer_window` guard.

```text
fix(bar): configure layer shell before the window is realized

- init_root showed the window immediately, but gtk4-layer-shell
  requires init_layer_shell to run before the window is realized
```

### 2.9 Reconcile bars against the monitor list **(manual check)**

- `src/app.rs`: replace `tracked: Rc<RefCell<Vec<String>>>`, the splice logic
  and `MonitorAdded`, `MonitorRemoved` and `MonitorInvalidated` with one
  `MonitorsChanged` message.
- On each change, collect the current `gdk4::Monitor`s, drop bars whose monitor
  is gone (compared by identity, not connector), and create bars for new
  monitors.
- `src/widgets/bar.rs`: remove `BarOutput::MonitorInvalidated` and its handler.

```text
refactor(app): reconcile bars against the monitor list

- a late MonitorInvalidated for a reused connector name could delete the
  replacement bar
- connector tracking was spliced with indices from the full monitor
  list, so one monitor without a connector shifted every later entry
- comparing the live monitor list with existing bars removes both
  classes of bug and the Rc<RefCell> bookkeeping
```

### 2.10 Move the popup panel to a remaining monitor

- `src/app.rs`: when the monitor owning the fresh popup panel goes away, create
  the panel on the first remaining monitor during reconciliation.

```text
fix(app): move the popup panel to a remaining monitor

- the panel was only recreated on the next monitor hotplug, so
  unplugging its monitor silently dropped every popup until another
  monitor appeared
```

### 2.11 Open the action panel only on the clicked monitor

- `src/widgets/bar.rs`: handle the right group's toggle output inside `Bar` by
  toggling its own `ActionPanel`.
- Rename the relay variants to `ToggleActionPanel`; remove
  `BarOutput::ToggleNotificationCenter` and
  `CadenzaShellMsg::ToggleNotificationCenter`.

```text
fix(bar): open the action panel only on the clicked monitor

- the bell button toggled the action panel on every monitor because the
  click was relayed to the app and broadcast to all bars
- each bar owns its panel, so it can toggle it directly
```

### 2.12 Align menu and popup margins with the bar **(manual check)**

- `src/widgets/menu_window.rs` and `src/notifications/fresh.rs`: use exclusive
  zone 0 and a small fixed margin, relying on the compositor to place surfaces
  below the bar's exclusive zone. Remove the hardcoded `32` and the
  `bar.height + 4` offset.

```text
fix(widgets): align menu and popup margins with the bar

- menus added the bar height on top of the compositor's exclusive zone
  offset, leaving a large gap
- popups used a hardcoded 32 px margin that overlapped taller bars
```

## Phase 3: async services

### 3.1 Add a set_if_changed helper for shared state

- New `src/utils/shared_state.rs`:
  `pub fn set_if_changed<T: PartialEq>(state: &SharedState<T>, value: T) -> bool`.
  Compare under `read()`, write only when different. Unit tests.

```text
feat(utils): add a set_if_changed helper for shared state

- relm4's SharedState notifies every subscriber on each write guard
  drop, even when nothing changed, which re-renders tiles and menus for
  no reason
```

### 3.2 Add an exponential backoff helper

- New `src/utils/backoff.rs`: `Backoff { initial, max, current }` with `next()`
  and `reset()`. Unit tests.

```text
feat(utils): add an exponential backoff helper

- several services need to retry lost connections without busy-looping
  or hammering a daemon that's still starting up
```

### 3.3 Read brightness events asynchronously

- `src/brightness.rs`: use `inotify.into_event_stream(buffer)` and
  `while let Some(event) = stream.next().await`. Read the files once per batch.

```text
fix(brightness): read inotify events asynchronously

- read_events_blocking parked a relm4 runtime thread forever and could
  never be cancelled on shutdown
```

### 3.4 Pick the preferred backlight and reject a zero maximum

- `src/brightness.rs`: choose the backlight by `type` (`firmware`, then
  `platform`, then `raw`), log startup failures, and
  `anyhow::ensure!(max > 0, ..)`.

```text
fix(brightness): pick the preferred backlight and reject a zero maximum

- the first directory entry won, which is often the wrong device on
  machines with both acpi_video0 and a native backlight
- a zero max_brightness produced NaN progress
- startup failures were dropped without a log line
```

### 3.5 Run the pulseaudio main loop on a blocking thread

- `src/pulseaudio.rs`: move the session body into a sync `fn run_session()` and
  call it via `tokio::task::spawn_blocking` from the async service.

```text
fix(pulseaudio): run the main loop on a blocking thread

- mainloop.iterate(true) blocked a relm4 runtime thread forever inside
  an async fn that never yielded, so shutdown couldn't cancel it
```

### 3.6 Reconnect when the sound server restarts

- `src/pulseaudio.rs`: `run_session` returns when the context reaches `Failed`
  or `Terminated` (quit the main loop from the state callback) or iteration
  errors. The async wrapper clears `VOLUME_STATE`, waits on `Backoff`, and
  retries.

```text
fix(pulseaudio): reconnect when the sound server restarts

- a failed initial connect or a restarted pipewire-pulse left the volume
  tile hidden or stale until the shell restarted
- iteration errors were logged in a tight loop
```

### 3.7 Keep the pulseaudio context in Rc<RefCell>

- `src/pulseaudio.rs`: replace `Arc<Mutex<Context>>` and the `lock().unwrap()`
  calls with `Rc<RefCell<Context>>` inside the blocking session. Derive
  `Default` on `PulseAudioData`.

```text
refactor(pulseaudio): keep the context in Rc<RefCell>

- the context never leaves its thread, so the mutex only added poisoning
  panics and try_lock juggling
```

### 3.8 Only publish volume when it changes

- `src/pulseaudio.rs`: derive `PartialEq` on `PulseAudioData` and publish with
  `set_if_changed`. Remember the default sink index and ignore events for other
  sinks. Use `ChannelVolumes::avg()` instead of the hand-rolled sum.

```text
fix(pulseaudio): only publish volume when it changes

- every sink event, including suspend and resume of unrelated sinks,
  rewrote the state and lit up the volume tile
- the hand-rolled average could overflow a u32 with many channels
```

### 3.9 Add zbus proxies for media player interfaces

- New `src/mpris/proxy.rs`: `#[proxy]` traits for
  `org.mpris.MediaPlayer2.Player` (`PlaybackStatus`, `Metadata`) and the
  `org.freedesktop.DBus` name listing used for discovery.

```text
feat(mpris): add zbus proxies for media player interfaces

- the blocking mpris crate can't be awaited and can't report players
  appearing, so the service needs async proxies
```

### 3.10 Add player selection logic

- New `src/mpris/selection.rs`: a pure `Players` struct keyed by bus name that
  records status changes and picks the most recently playing player, else the
  most recently changed one. Unit tests.

```text
feat(mpris): add player selection logic

- with several players open, the tile should follow whichever one most
  recently started playing
- keeping the choice pure makes it testable without a bus
```

### 3.11 Replace the blocking mpris crate with a zbus service

- `src/mpris.rs`: list `org.mpris.MediaPlayer2.*` names, watch
  `NameOwnerChanged`, merge each player's property streams into one channel,
  and publish the selected player with `set_if_changed`.
- Define a local `PlaybackStatus` so the tile no longer imports the crate.
  Use `xesam:artist` everywhere.
- `cargo remove mpris`.

```text
refactor(mpris): replace the blocking mpris crate with a zbus service

- the old service blocked a runtime thread, followed only the first
  player it found, and polled for players every three seconds
- track changes read album artists while the initial state read
  artists, so the artist vanished after the first track change
- watching NameOwnerChanged picks up players as they come and go
```

### 3.12 Lower the relm4 worker thread count

- `src/main.rs`: `RELM_THREADS.set(4)`.

```text
perf(main): lower the relm4 worker thread count

- sixteen threads were only needed to hide services that blocked their
  worker forever; nothing blocks the runtime any more
```

## Phase 4: dead code and dependencies

### 4.1 Remove unused dependencies

- `cargo remove async-channel csv relm4-components systemstat thiserror`
- `cargo remove --build glib-build-tools`
- Delete the unused `docs` feature from `Cargo.toml`.

```text
chore(deps): remove unused dependencies

- none of these crates are referenced in src or build.rs, and cargo
  already warns about them
- the docs feature gates nothing
```

### 4.2 Remove the dead_code allow from the build script

```text
chore(build): remove the dead_code allow from the build script

- build.rs has no dead code to hide
```

### 4.3 Trim NetworkManager proxies to the members in use

- `src/network/dbus.rs`: delete every unused method, property and signal (see
  the review list), the whole `WiredDevice` proxy, and the mistyped
  `Device.StateReason`. Keep `activate_connection`, `add_and_activate_connection2`
  and `last_scan`, which later fixes need.

```text
refactor(network): trim NetworkManager proxies to the members in use

- most of the hand-written proxy surface was never called, and one
  property was declared with the wrong D-Bus type
- the file drops from about 450 to about 130 lines
```

### 4.4 Replace NetworkManager mirror enums with constants

- `src/network/types.rs`: replace `DeviceType`, `DeviceState`,
  `DeviceStateReason`, `ActiveConnectionState` and
  `ActiveConnectionStateReason` with the handful of `u32` constants actually
  compared. Keep `State` and `ConnectivityState`. Remove the file-level
  `#![allow(dead_code)]`.
- `src/network/state.rs`: map `DeviceKind` straight from `u32`.

```text
refactor(network): replace NetworkManager mirror enums with constants

- five enums with over 150 variants existed to match three values, and
  their hand-written From tables had already drifted from the discriminants
- the file-level dead_code allow was hiding two enums with no users
```

### 4.5 Move access point security into its own module

- Move `ApSecurity`, its flag constants and tests from `types.rs` to
  `src/network/security.rs`. Make the flag helpers private; drop the unused
  `Display` and `Default` impls.

```text
refactor(network): move access point security into its own module

- security classification is domain logic, not a D-Bus mirror type
```

### 4.6 Make network submodules private and drop dead fields

- `src/network.rs`: `mod` instead of `pub mod`; re-export `ApSecurity`, `State`
  and `ConnectivityState`. Rename the re-exported `scan` function to
  `request_scan`.
- Remove `WifiScanState::last_scan_ms`, the `Primary` payload and the
  `#[allow(dead_code)]` on `NetworkPropertyChange`.
- `fetch_specific_info` returns its value directly instead of an `Ok`-only
  `Result`.

```text
refactor(network): make submodules private and drop dead fields

- public modules hid dead code from the compiler
- a function and a module were both named scan
- last_scan_ms was written but never read
```

### 4.7 Remove unused bluetooth state fields

- `src/bluetooth/state.rs`: remove `_session`, `trusted`, `rssi` and
  `battery_percentage` and their writers.
- `src/tiles/bluetooth.rs`: remove `bluetooth_info`; use `subscribe` instead of
  `subscribe_optional`.

```text
refactor(bluetooth): remove unused state fields

- these fields were written but never read
- rssi changes during discovery took the write lock and re-rendered
  every subscriber several times a second
- the adapter already keeps the session alive
```

### 4.8 Tighten bluetooth visibility and fix stale comments

- `mod` instead of `pub mod` in `src/bluetooth.rs`; private row types in
  `bluetooth_menu.rs`; remove `_main_box` fields here and in `network_menu.rs`.
- Rewrite history-style comments to describe current behaviour.
- Replace `Mutex::lock().unwrap()` with `unwrap_or_else(PoisonError::into_inner)`
  in `agent.rs`.

```text
refactor(bluetooth): tighten visibility and fix stale comments

- several comments described removed code such as a set_trusted command
- a poisoned mutex shouldn't take down the pairing agent
```

### 4.9 Remove unused battery identity, udev helpers and serde derives

- Delete `BatteryIdentity`, `read_battery_identity`, `device_key`,
  `read_status_from_event`, the debug-only device enumeration in `udev.rs`,
  `SysfsReading::when`, and the unused serde derives on `SysfsReading`,
  `BatteryCapacity` and `ChargingStatus`.
- `src/battery/alerts.rs`: derive `Default`. Rename `BatteryCapacity::div` to
  `ratio`.
- Drop chrono's `serde` feature if nothing else needs it after this commit
  (`cargo check` confirms).

```text
refactor(battery): remove unused identity, udev helpers and serde derives

- battery identity only fed a log line; the profile path never used it
- the serde derives and chrono's serde feature were never exercised
- div read like the Div trait but returned an Option
```

### 4.10 Remove unused notification events and plumbing

- Remove `NotificationEvent::{AllCleared, ActionInvoked}` and the dead fields
  on `Closed`, `NotificationsDaemon::event_tx` and `new()`, the `_emitter`
  parameter, `FreshNotifications::visible`, `de_actions` and the `Deserialize`
  derive on `Notification`. Log before moving fields to drop the extra clones.

```text
refactor(notifications): remove unused events and plumbing

- these variants, fields and helpers had no readers and needed
  dead_code allows to compile quietly
```

### 4.11 Remove unused progress tile output and init fields

- `src/widgets/progress_tile.rs`: remove `ProgressTileOutput::Clicked`,
  `extra_classes` and the file-level `#![allow(dead_code)]`.

```text
refactor(widgets): remove unused progress tile output and init fields

- the clicked output was never emitted and extra classes were never read
```

### 4.12 Remove unused tile messages and constant icon helpers

- Remove `PulseAudioTileMsg::TileClicked`, `_percent_to_volume`, the
  one-entry `BRIGHTNESS_ICON_NAMES` and `get_icon` in the brightness tile, the
  duplicate `VOLUME_ZERO`, the empty clock `update`, and unused `Output` types.
- `src/widgets/tile.rs`: emit the click output straight from the closure
  instead of looping through `TileMsg::Click`.

```text
refactor(tiles): remove unused messages and constant icon helpers

- the brightness icon helper claimed a logarithmic curve but always
  returned the same icon
- the zero and muted volume icons were identical
```

### 4.13 Remove the unused display handle and stale ordering comments

- `src/app.rs`: remove `_display`; delete the "must be first" comments, since
  the channels are lazily created with `get_or_init`.

```text
refactor(app): remove the unused display handle and stale comments

- the default display is a gdk singleton and needs no handle
- service start order doesn't matter because every channel is created
  on first use
```

### 4.14 Replace ConfigManager with a static config

- `src/settings.rs`: `static CONFIG: OnceLock<CadenzaShellConfig>`,
  `pub fn init()`, `pub fn get_config() -> &'static CadenzaShellConfig`, and
  `fn load()` using `dirs::config_dir()`. Remove `update_config`, `save`,
  `reload`, `reset_to_defaults` and the file-level `#![allow(dead_code)]`.

```text
refactor(settings): replace ConfigManager with a static config

- the config is never mutated after startup, so the mutex and the
  update, save, reload and reset helpers were all unused
- dirs already provides the config directory lookup
```

### 4.15 Remove unread settings fields

- Remove `ui.scale_factor`, `bar.position` (and `BarPosition`),
  `tiles.show_labels`, `tiles.max_text_width` and
  `notifications.max_notifications`.

```text
refactor(settings): remove unread fields

- these fields were accepted and written to new config files but had no
  effect; existing files keep parsing because unknown keys are ignored
```

### 4.16 Use notification and icon size settings

- `src/notifications/fresh.rs`: popup width from `notifications.popup_width`;
  default popup duration from `notifications.timeout`.
- `src/notifications/panel.rs`: width from `notifications.center_width`.
- `src/widgets/tile.rs`: pixel size from `tiles.icon_size`.

```text
feat(settings): use notification and icon size settings

- these settings existed but the same values were hardcoded at their
  use sites, so changing them did nothing
```

### 4.17 Remove unused style rules and scope the revealer background

- `src/style.scss`: delete `.bar-button`, `.bar-groups`, `trough.full`,
  `.empty-state`, `.bold`, `.access-point-frequency`, `$dim-color`; use
  `$warn-color` for `#fa0`; scope `revealer` to `.action-panel revealer`.

```text
style: remove unused rules and scope the revealer background

- these selectors match nothing in the code
- the bare revealer rule tinted every revealer, including the tray's
```

### 4.18 Style tray attention, active progress tiles and card classes

- Add rules for `tray-needs-attention`, `tray-active`, and the card's `body`,
  `close-button`, `icon-image`, `low` and `normal` classes.
- Use `.dim:not(.active)` so active volume and brightness tiles brighten.

```text
fix(style): style tray attention, active tiles and card classes

- tray items needing attention looked identical to idle ones
- the dim opacity overrode the active state, so progress tiles never
  brightened when changed
```

### 4.19 Avoid adding an empty css class

- `src/tiles.rs`: `Attention::css_class()` returns `Option<&'static str>`.

```text
fix(tiles): avoid adding an empty css class

- normal attention added "" to the class list on every update
```

## Phase 5: network and bluetooth fixes

### 5.1 Activate saved profiles instead of creating duplicates **(manual check)**

- Carry `saved_connection: Option<OwnedObjectPath>` in `NetworkCommand::Connect`
  (from `AccessPointSummary`).
- When present, call `activate_connection(saved, device, ap_path)`; otherwise
  call `add_and_activate_connection2` and delete the new profile if the attempt
  fails.
- Fix the doc comments in `service.rs` and `commands.rs` that claimed the old
  call reused matching profiles.

```text
fix(network): activate saved profiles instead of creating duplicates

- AddAndActivateConnection always creates a new profile, so each click
  on a saved network added another copy, and saved secured networks
  failed with "wrong password" because the copy had no secrets
- failed password attempts also left profiles holding bad passwords
```

### 5.2 Track connect outcomes through the active connection

- Watch the active connection path returned by the activate call: subscribe
  to its `StateChanged` first, then read `State` once to close the race.
- Map reason `NO_SECRETS` to `WrongPassword`. On timeout, report "still
  connecting" instead of a failure.

```text
fix(network): track connect outcomes through the active connection

- watching the whole device after the call returned could miss a quick
  failure and attributed other attempts' results to this one
- slow DHCP was reported as a failure after twenty seconds
```

### 5.3 Report connect errors to the menu

- `handle_command`: on `Err` from a connect, send
  `NetworkEvent::ConnectionFailed` with a friendly reason.
- `src/network_menu.rs`: validate the PSK (8 to 63 characters or 64 hex
  digits) before submitting, and show "enterprise networks aren't supported
  here" for 802.1X access points instead of attempting to connect.

```text
fix(network): report connect errors to the menu

- polkit denials and invalid keys were only logged, after the password
  prompt had already closed
- enterprise networks always failed without feedback
```

### 5.4 Disconnect the wifi device instead of the primary connection

- Add `disconnect` to the `NetworkDevice` proxy and call it on the wireless
  device instead of deactivating `PrimaryConnection`.

```text
fix(network): disconnect the wifi device instead of the primary connection

- with ethernet or a vpn as primary, the wifi disconnect button tore
  down the wrong connection
```

### 5.5 Show the wifi connection even when it isn't primary

- Read SSID and strength from the wireless device's `ActiveAccessPoint`,
  independent of `PrimaryConnection`; the tile keeps describing the primary
  connection, the menu uses the wifi info.

```text
fix(network): show the wifi connection even when it isn't primary

- with ethernet primary, the menu showed no ssid and clicking the
  connected network started a new connection
```

### 5.6 Refresh access points when scans complete

- While the menu is open, subscribe to the wireless device's
  `receive_last_scan_changed()` and refresh on change.
- Refresh after connect, forget and disconnect commands.

```text
fix(network): refresh access points when scans complete

- RequestScan only starts a scan, so refreshing right after it always
  showed the previous scan's results
- rows kept stale saved and connected markers after forget or disconnect
```

### 5.7 Derive the active access point from live state

- Remove `AccessPointSummary::is_active`; the menu compares each row's SSID
  with the wifi info from `NETWORK_STATE` when rendering.

```text
fix(network): derive the active access point from live state

- is_active was fixed at scan time, so rows showed stale "connected"
  markers and clicks on them did nothing
```

### 5.8 Sort access points deterministically and skip idle scans

- `src/network/scan.rs`: add `.then_with(|| a.ssid.cmp(&b.ssid))` to the sort.
- Skip `RequestScan` while wifi is disabled; log a scan failure once instead
  of in both `handle_scan` and `handle_command`.

```text
fix(network): sort access points deterministically and skip idle scans

- equal strengths came out in hashmap order, shuffling rows each scan
- scans with the radio off failed and logged twice every interval
```

### 5.9 Forget every saved profile for an ssid

- `src/network/scan.rs`: collect saved connections as
  `HashMap<String, Vec<OwnedObjectPath>>`; `Forget` deletes all of them.

```text
fix(network): forget every saved profile for an ssid

- with duplicate profiles only the last one was deleted, so the network
  still showed as saved
```

### 5.10 Clear state when NetworkManager disappears

- On owner loss, reset `NETWORK_STATE` to its default and stop the reconcile
  refetch until the owner returns.
- In `handle_primary_change`, abort the old subscriptions only after a
  successful fetch.

```text
fix(network): clear state when NetworkManager disappears

- the tile kept showing the last connection and reconcile logged a
  warning every interval
- a failed refetch dropped strength tracking
```

### 5.11 Resync the wifi switch when toggling fails

- On a `SetWirelessEnabled` error, re-read `WirelessEnabled` and write it to
  `NETWORK_STATE` so subscribers resync.

```text
fix(network): resync the wifi switch when toggling fails

- a denied SetWirelessEnabled produced no property change, leaving the
  switch in the user's position instead of the real one
```

### 5.12 Redact passwords from debug output

- Manual `Debug` impls for `NetworkMenuMsg` and `NetworkCommand` that print
  `<redacted>` in place of passwords.

```text
fix(network): redact passwords from debug output

- both types derived Debug with the plaintext password inside, one log
  statement away from leaking it
```

### 5.13 Let NetworkManager choose key management **(manual check)**

- In `build_security_settings`, set only `psk`, or `wep-key0` with
  `wep-key-type` 2 for passphrases, and pass the access point path as
  `specific_object`. Leave `key-mgmt` for NetworkManager to complete.
- Test on a WPA2/WPA3 transition network and a plain WPA2 network.

```text
fix(network): let NetworkManager choose key management

- forcing sae on wpa2/wpa3 transition networks broke hardware without
  sae support, and wep passphrases were rejected as hex keys
```

### 5.14 Correct stale network doc comments

- Fix the comments on `commands.rs` (no in-flight scan check exists),
  `state.rs` (`last_scan` purpose), the property-stream resubscription
  rationale in `service.rs`, `GetAllAccessPoints` in `dbus.rs`, and remove the
  "see a following commit" reference.

```text
docs(network): correct stale doc comments

- several comments described checks that don't exist, a field's old
  purpose, and the wrong reason property streams need resubscribing
```

### 5.15 Use a synchronous factory for bluetooth device rows

- `src/bluetooth_menu.rs`: replace `AsyncFactoryVecDeque` and
  `AsyncFactoryComponent` with `FactoryVecDeque` and `FactoryComponent`.

```text
fix(bluetooth-menu): use a synchronous factory for device rows

- rows still initializing were invisible to the diff, so rapid property
  updates during discovery pushed duplicate rows that never updated
- row initialization does no async work
```

### 5.16 Clear pairing prompts when requests end

- `src/bluetooth/agent.rs`: tag each request with a generation from an
  `AtomicU64`. A `PromptGuard` clears `PAIRING_PROMPT` and `PENDING_REPLY` on
  drop, but only if they still hold its generation.
- `respond()` also clears the prompt.

```text
fix(bluetooth): clear pairing prompts when requests end

- bluez cancels a request by dropping its future, which skipped the
  cleanup and left a stale prompt on screen
- overlapping requests cleared each other's prompts
```

### 5.17 Reply to display requests immediately **(manual check)**

- `DisplayPinCode` and `DisplayPasskey` set the prompt, move the guard into a
  spawned task that awaits `req.cancel`, and return `Ok(())` right away.
- Test legacy PIN pairing with a keyboard and passkey display pairing.

```text
fix(bluetooth): reply to display requests immediately

- bluez waits for the reply before continuing, so holding it until
  cancel stalled legacy pin pairing until it timed out
- repeated passkey updates replaced the cancel slot and wiped the
  newer prompt
```

### 5.18 Re-register the agent after bluetoothd restarts

- Retry `register_agent` in `reinitialize_adapter` whenever the handle is
  `None`; if `request_default` fails, retry with `request_default: false`.
- Watch `org.bluez` `NameOwnerChanged` and reinitialize when it changes.

```text
fix(bluetooth): re-register the agent after bluetoothd restarts

- registration was attempted once at startup, and a failed default
  agent request dropped the agent entirely
- bluer's session events don't report a restarted daemon
```

### 5.19 Subscribe to events before snapshotting devices

- `reinitialize_adapter` and `handle_device_added`: subscribe to adapter and
  device events first, then list devices once and fetch their properties.

```text
fix(bluetooth): subscribe to events before snapshotting devices

- devices added between the snapshot and the subscription were never
  tracked
```

### 5.20 Reconcile devices against the adapter

- Use `adapter.device_addresses()` as the source of truth: subscribe new
  devices, abort tasks and drop entries for removed ones, and drop devices
  whose property fetch fails. Reinitialize when state is `None`.

```text
fix(bluetooth): reconcile devices against the adapter

- reconcile only refreshed known addresses, so missed removals became
  ghost devices with default properties and missed additions never
  appeared
```

### 5.21 Fall back to another adapter when ours is removed

- On `AdapterRemoved` for the tracked adapter, clear state and call
  `reinitialize_adapter`.

```text
fix(bluetooth): fall back to another adapter when ours is removed

- state became empty even when a second adapter was present, until the
  next adapter was added
```

### 5.22 Subscribe to bluetooth state before reading it

- `src/bluetooth_menu.rs`: move the `BLUETOOTH_STATE.subscribe` call above the
  initial `read()`.

```text
fix(bluetooth-menu): subscribe to state before reading it

- a write between the read and the subscription was missed
```

### 5.23 Validate pairing input and show device aliases

- Accept PINs of 1 to 16 characters and passkeys up to 999999, showing an
  inline error label otherwise.
- Show the device alias from state, falling back to the address.
- Clear and focus the entry whenever a new prompt appears.

```text
fix(bluetooth-menu): validate pairing input and show device aliases

- invalid passkeys were silently ignored and empty pins were sent
- prompts showed raw addresses
```

### 5.24 Render initial bluetooth state and pluralize the count

- `src/tiles/bluetooth.rs`: launch the `Tile` with a `TileInit` built from the
  current state and set the root's visibility in `init`.
- Use "1 device connected" and "{n} devices connected".

```text
fix(tiles): render initial bluetooth state and pluralize the count

- tiles created after state existed showed an empty button until the
  next write
- "device(s)" read oddly in a branch that only runs for counts above one
```

## Phase 6: battery, notification and small service fixes

### 6.1 Read power_now when current_now is missing

- `src/battery/sysfs.rs`: read `power_now` (µW) when present, otherwise
  voltage times current.
- `src/battery/watcher.rs`: log an error before returning when no reading is
  possible.

```text
fix(battery): read power_now when current_now is missing

- many laptops expose energy_* and power_now but no current_now, so the
  battery tile never appeared and nothing was logged
```

### 6.2 Pair energy and charge readings consistently

- Pick `(energy_now, energy_full)` together, else `(charge_now, charge_full)`.
- Keep the percentage as `Option` and skip alerts when it's `None` instead of
  defaulting to 0.

```text
fix(battery): pair energy and charge readings consistently

- mixing an energy value with a charge value made the ratio None, which
  was shown as 0% and fired the critical alert
```

### 6.3 Ignore peripheral batteries

- `detect_battery_path`: skip supplies whose `scope` is `Device`; prefer
  `BAT*`.
- `is_battery_change`: compare the event's syspath with the chosen battery.

```text
fix(battery): ignore peripheral batteries

- mouse and controller batteries also report type Battery and could be
  picked as the system battery, or trigger needless updates
```

### 6.4 Fall back to polling when udev is unavailable

- If `create_battery_monitor()` fails, or `readable()` keeps erroring, continue
  with the poll interval only.

```text
fix(battery): fall back to polling when udev is unavailable

- without a udev monitor the watcher returned and the tile froze
- persistent readable() errors looped without delay
```

### 6.5 Add a short and long power EMA model

- New `src/battery/power_model.rs`: `PowerModel { short_w, long_w, last }`.
  `update(watts, now: Instant)` applies `α = 1 - exp(-dt / τ)` with τ of
  three minutes and two hours; `time_to_empty(wh)` blends them (70/30) and caps
  at a sane maximum. Unit tests drive it with explicit instants.

```text
feat(battery): add a short and long power EMA model

- a time-weighted average reacts to load changes within minutes and
  handles irregular sample spacing from udev events
```

### 6.6 Estimate time to empty from power EMAs

- `src/battery/watcher.rs`: feed `PowerModel` on every discharging reading and
  compute time to empty from it; skip the estimate unless discharging.
- Delete `src/battery/discharging.rs`, profile persistence, `STATISTICS_ALPHA`,
  `SAVE_INTERVAL` and `READINGS_PER_LIFETIME`. Leave any existing profile file
  on disk.
- `cargo remove serde-big-array`. Drop chrono's `serde` feature if still
  present.
- `watch_battery` returns `()` instead of `Option<!>`.

```text
refactor(battery): estimate time to empty from power EMAs

- the weekly fourier model was biased by sampling only while
  discharging, averaged over weeks so it ignored current load, and ran
  millions of float operations every ten seconds
- about 150 lines only fed debug logs
- the old profile file is no longer read or written; it's left on disk
```

### 6.7 Refresh battery state after resume

- Add `sleep_monitor::subscribe_wake()` as a `select!` branch that forces a
  reading.

```text
fix(battery): refresh state after resume

- monotonic timers pause during suspend, so state stayed stale until
  the next udev event
```

### 6.8 Only reset alerts once charging

- `src/battery/alerts.rs`: reset the flags only when `status.is_charging()`;
  fix the doc comment about both alerts firing.

```text
fix(battery): only reset alerts once charging

- brief unknown or not-charging readings re-armed alerts so they fired
  again, and the doc comment described the wrong behaviour
```

### 6.9 Only show battery warnings while discharging

- Move `BATTERY_THRESHOLD_LOW` and `BATTERY_THRESHOLD_CRITICAL` to
  `src/battery.rs` and use them in the tile.
- Gate low and critical styling on `status == Discharging`.
- Replace `RemainingDurationCategory` with a pure
  `format_remaining(status, remaining)` using `TIME_REMAINING_LOW_THRESHOLD`.
  Unit tests.

```text
fix(tiles): only show battery warnings while discharging

- plugged-in batteries in not-charging or unknown states could still
  show low warnings
- thresholds were duplicated as literals in the tile
```

### 6.10 Reap sound child processes

- `src/sound.rs`: spawn a task that awaits `child.wait()` and logs a non-zero
  exit; make the doc comment match.

```text
fix(sound): reap child processes

- spawned players were never waited on, and the doc comment promised
  exit status logging that didn't exist
```

### 6.11 Route every notification close through one function

- New `close(id, reason)` used by expiry, `CloseNotification`, user dismissal
  and `ClearAll`: abort the expiry timer (`HashMap<u32, AbortHandle>` behind a
  mutex), remove state, broadcast `Closed`, emit `NotificationClosed`.
- Reasons: expiry 1, user 2, `CloseNotification` 3. Unknown ids in
  `CloseNotification` return a D-Bus error. Critical notifications get no
  expiry timer; replacing a notification aborts its old timer.

```text
refactor(notifications): route every close through one function

- three close paths disagreed: expiry sent no D-Bus signal, timers were
  never cancelled so replaced notifications closed early, and
  CloseNotification used the wrong reason
- clear-all closed nothing on the bus
- critical notifications expired, which the spec forbids
```

### 6.12 Keep notifications when popups time out

- `src/notifications/fresh.rs`: the auto-dismiss timer only removes the popup
  card. Use the notification's `expire_timeout`: -1 uses
  `notifications.timeout`, 0 keeps the popup until closed, positive values are
  milliseconds.

```text
fix(notifications): keep notifications when popups time out

- every popup timeout removed the notification from the action panel
  and told the app the user had dismissed it
- expire_timeout was ignored
```

### 6.13 Replace popups in place

- On `NewNotification` with an id already shown, update that card in place and
  restart its timer generation.

```text
fix(notifications): replace popups in place

- replaces_id pushed a second card with the same id, and the old timer
  dismissed the new popup early
```

### 6.14 Parse hints leniently

- Take `hints: HashMap<String, OwnedValue>` in `Notify` and read the used
  hints through small helpers that fall back to defaults on type mismatch.
- `NotificationUrgency: From<u8>`. `cargo remove serde_repr`.

```text
fix(notifications): parse hints leniently

- one unexpected hint type failed the whole Notify call and dropped the
  notification
```

### 6.15 Avoid id collisions with client-chosen ids

- When honouring an unknown `replaces_id`, bump the counter with
  `fetch_max(replaces_id + 1)`.
- Use `chrono::Local::now().timestamp()` instead of unwrapping
  `duration_since`.

```text
fix(notifications): avoid id collisions with client-chosen ids

- an unknown replaces_id above the counter later collided with an
  automatically assigned id
```

### 6.16 Report the crate version in server information

- `GetServerInformation` returns `env!("CARGO_PKG_VERSION")`.

```text
fix(notifications): report the crate version in server information

- the hardcoded 1.0.0 didn't match the package version
```

### 6.17 Invoke the default action on click and close afterwards

- Body click invokes `"default"` when present; buttons for every other action,
  even when there is only one.
- After `ActionInvoked`, close with reason 2 unless the `resident` hint is set.

```text
fix(notifications): invoke the default action and close afterwards

- body clicks sent whichever action came first, a single non-default
  action had no button, and notifications stayed open after an action
```

### 6.18 Keep cards in a stable order

- `src/notifications/panel.rs`: sort by `Reverse((timestamp, id))`.

```text
fix(notifications): keep cards in a stable order

- sorting by whole seconds left ties in hashmap order, so cards jumped
  around
```

### 6.19 Resync popups after the event receiver lags

- On `RecvError::Lagged`, drop popups whose ids are no longer in
  `NOTIFICATIONS_STATE`.

```text
fix(notifications): resync popups after the event receiver lags

- missed close events left popups that never closed
```

### 6.20 Wire up the clear-all button

- Add a "Clear all" button to the action panel that sends `DismissAll`;
  remove its `#[allow(dead_code)]`.

```text
feat(notifications): wire up the clear-all button

- DismissAll and clear_all existed but nothing could trigger them
```

### 6.21 Render initial weather and notification state

- `src/tiles/weather.rs`: send the initial input after subscribing.
- `src/tiles/notifications.rs`: push the current count and attention to the
  `Tile` in `init`.

```text
fix(tiles): render initial weather and notification state

- tiles on hotplugged bars stayed hidden or showed no count until the
  next state change, up to ten minutes for weather
```

### 6.22 Only highlight progress tiles when the value changes

- `ProgressTile`: call `activate()` only when the new progress differs from the
  current one, and never for the initial value.

```text
fix(widgets): only highlight progress tiles when the value changes

- the initial value and unrelated updates lit up volume and brightness
```

### 6.23 Fetch weather over https with rustls

- `cargo add reqwest --no-default-features --features json,rustls`.
- Switch the URL to `https://` and remove the misleading comment.
- If `aws-lc-rs` fails to build, add `cmake` (and `clang` if needed) to
  `nativeBuildInputs` in `devenv.nix`.

```text
fix(weather): fetch over https with rustls

- https only failed because reqwest was built without a tls backend
```

### 6.24 Add request timeouts and keep the last good report

- Build one `reqwest::Client` with a 15 s timeout and 5 s connect timeout.
- Use `.error_for_status()?.json::<WttrReport>()`.
- On error, log it, keep the previous state, and retry with `Backoff`.

```text
fix(weather): add request timeouts and keep the last good report

- a hung request froze polling forever
- any error hid the tile and was never logged
```

### 6.25 Refresh weather after resume

- `select!` between the poll sleep and `subscribe_wake()`; wait a few seconds
  after waking for the network to settle.

```text
fix(weather): refresh after resume

- suspend pauses the timer, so data stayed stale after waking
```

### 6.26 Compare sun times in the location's local time

- Use `localObsDateTime` from the report as "now" and compare in minutes since
  midnight.

```text
fix(weather): compare sun times in the location's local time

- sunrise and sunset are local to the location, so day and night icons
  were wrong behind a vpn or while travelling
```

### 6.27 Map missing condition codes and use a sun icon

- Add codes 281, 284, 374 and 377 to the snow group; drop the non-existent 223.
- Bundle `weather-sunny-regular` in `build.rs` and use it for clear days.
- Treat a temperature parse failure as an error instead of showing 0°.
- Store the icon as `&'static str`.

```text
fix(weather): map missing condition codes and use a sun icon

- several codes fell back to the cloud icon, and sunny used the
  brightness icon
- a failed temperature parse showed 0°
```

### 6.28 Track niri state with EventStreamState

- `src/niri.rs`: apply each event to `niri_ipc::state::EventStreamState` and
  publish workspaces and the focused window from it. Remove `send_request` and
  `fetch_and_update`.

```text
refactor(niri): track state with EventStreamState

- every window or focus event opened two new socket connections to
  refetch state that niri-ipc can apply locally
- niri sends the full state at the start of the stream, so the separate
  initial fetch is unnecessary
```

### 6.29 Order workspace dots by index

- `src/tiles/niri.rs`: filter by output, sort by `idx`, resize the factory to
  the workspace count, and send `Update` by position.

```text
fix(tiles): order workspace dots by index

- dots were ordered by id and inserted with an off-by-one workaround
  that only worked because update messages hadn't been applied yet
- moved workspaces never re-sorted
```

### 6.30 Update tray icon, tooltip and status on change

- `src/widgets/tray_item.rs`: keep the `gtk::Image` in the widgets and refresh
  image, tooltip and status classes in `update_view`.

```text
fix(tray): update icon, tooltip and status on change

- these were set once at creation and never refreshed
```

### 6.31 Apply tray menu diffs once and unparent the popover

- Call `apply_menu_diffs` once per update; implement `shutdown` to unparent the
  popover. Remove the stale left-click TODO and the stored `action_group`.

```text
fix(tray): apply menu diffs once and unparent the popover

- diffs were applied once per diff, and the popover was left parented
  when its item was removed
```

### 6.32 Upload tray pixmaps without manual byte swizzling

- Pass the pixmap bytes to `gdk::MemoryTexture` as `MemoryFormat::A8r8g8b8`;
  pick the pixmap closest to `16 * scale_factor`.

```text
refactor(tray): upload pixmaps without manual byte swizzling

- argb32 pixmaps map directly to gdk's A8r8g8b8 format
- the first pixmap was used regardless of its size
```

### 6.33 Use bundled icons for the tray expand toggle

- Bundle `pan-start` and `pan-end` style icons in `build.rs` and use them
  instead of nerd font glyphs.

```text
fix(tray): use bundled icons for the expand toggle

- the toggle relied on a nerd font being installed
```

### 6.34 Reconnect the sleep monitor when the system bus drops

- Wrap the monitor in a retry loop with `Backoff`; log signal argument parse
  errors.

```text
fix(sleep-monitor): reconnect when the system bus drops

- wake events stopped for good after a bus error, and signal parse
  errors were swallowed
```

## Phase 7: notification capabilities

### 7.1 Show image-data and image-path hints

- Parse `image-data` (and the deprecated `image_data` and `icon_data`) into a
  `gdk::MemoryTexture`; load `image-path` and file path or `file://`
  `app_icon` values with `gtk::Image::from_file`.
- Remove `#[watch]` from card fields that never change.
- Advertise `icon-static`.

```text
feat(notifications): show image-data and image-path hints

- image hints were ignored and file path app icons were treated as
  icon names
```

### 7.2 Render body markup

- New `src/notifications/markup.rs`: sanitize the spec subset (`b`, `i`, `u`,
  `a href`, `img`), escape everything else, validate with
  `pango::parse_markup`, and fall back to escaped plain text. Unit tests.
- Cards use `set_markup` with the sanitized body. Advertise `body-markup`.

```text
feat(notifications): render body markup

- markup showed as raw tags, and clients that escape text showed
  entities
```

### 7.3 Open hyperlinks from notification bodies

- Keep `a href` in sanitized markup; handle the label's `activate-link` with
  `gtk::UriLauncher`. Advertise `body-hyperlinks`.

```text
feat(notifications): open hyperlinks from notification bodies

- links in notification bodies weren't clickable
```

### 7.4 Show images embedded in notification bodies

- The sanitizer extracts `img` tags; cards render each `src` as a
  `gtk::Picture` and show the `alt` text if loading fails. Advertise
  `body-images`.

```text
feat(notifications): show images embedded in notification bodies

- img tags were dropped or shown as raw markup
```

### 7.5 Render action icons

- Read the `action-icons` hint; when set, render action buttons with the
  action key as an icon name and the label as tooltip. Advertise
  `action-icons`.

```text
feat(notifications): render action icons

- clients asking for icon actions got buttons labelled with raw keys
```

### 7.6 Play sound files

- `src/sound.rs`: add `play_file(path)` using
  `canberra-gtk-play --file`.

```text
feat(sound): play sound files

- notification sounds can name a file instead of a theme event
```

### 7.7 Play notification sounds

- On `Notify`, unless `suppress-sound` is set, play `sound-file` or the
  `sound-name` theme event. Advertise `sound`.

```text
feat(notifications): play notification sounds

- the spec requires sound-file and suppress-sound support when the
  sound capability is advertised
```

### 7.8 Send activation tokens with invoked actions

- When an action button or body is clicked, request a token on the GTK thread
  via the display's `AppLaunchContext::startup_notify_id`, pass it with the
  action command, and emit `ActivationToken(id, token)` before
  `ActionInvoked`.

```text
feat(notifications): send activation tokens with invoked actions

- without a token, apps on wayland can't raise their window when an
  action is clicked
```

## Phase 8: structural simplification

### 8.1 Run the network service in a single select loop

- `src/network/service.rs`: one `tokio::select!` over property changes,
  `cmd_rx`, `wake_rx` and the reconcile interval. Remove the forwarding tasks
  and the `Wake`, `Reconcile` and `Command` variants; collapse refetch variants
  into `Refetch(&'static str)`. Install the command sender first.

```text
refactor(network): run the service in a single select loop

- three tasks existed only to copy messages between channels, and they
  leaked when the service exited or failed to start
```

### 8.2 Run the bluetooth service in a single select loop

- Same shape in `src/bluetooth/service.rs`; abort adapter and device tasks on
  exit.

```text
refactor(bluetooth): run the service in a single select loop

- the same forwarding tasks leaked here, and the reconcile ticker kept
  logging after the loop ended
```

### 8.3 Add a command slot helper

- New `src/utils/command_slot.rs`: `CommandSlot<T>` wrapping
  `OnceLock<UnboundedSender<T>>` with `install` and `send`. Unit tests.

```text
feat(utils): add a command slot helper

- network and bluetooth duplicated the OnceLock command sender pattern
```

### 8.4 Use the command slot in network and bluetooth

- Replace the hand-rolled statics in `network/commands.rs` and
  `bluetooth/commands.rs`.

```text
refactor: use the command slot in network and bluetooth

- removes two copies of the same install and send logic
```

### 8.5 Add an event bus helper

- New `src/utils/event_bus.rs`: `EventBus<T>` with a lazily created broadcast
  sender, `send` and `subscribe`. Unit tests.

```text
feat(utils): add an event bus helper

- sleep monitor, notifications and network each hand-rolled a lazily
  created broadcast channel
```

### 8.6 Use the event bus in sleep monitor, notifications and network

- Replace `wake_tx`, the notifications `event_tx` and `network/events.rs`.

```text
refactor: use the event bus in sleep monitor, notifications and network

- removes three copies of the same broadcast setup
```

### 8.7 Add a bluetooth actions module

- New `src/bluetooth/actions.rs`: `connect`, `disconnect`, `set_powered`,
  `pair` and `remove` spawn tasks against the service-owned adapter. The menu
  calls these instead of using `bluer` directly.

```text
refactor(bluetooth): add an actions module for adapter operations

- connect, disconnect and power were called from the ui while pair and
  remove went through the service loop
```

### 8.8 Call the pairing agent directly from the menu

- Remove `PairingReply` and `CancelPairing` commands; the menu calls
  `agent::respond` and `agent::cancel`.

```text
refactor(bluetooth): call the pairing agent directly from the menu

- replies went through two channels and waited behind slow service work
```

### 8.9 Keep the adapter out of shared state

- `BluetoothState` becomes plain data (`powered`, `discovering`, `devices`)
  deriving `Clone` and `PartialEq`; remove `adapter` and `device_handle`.

```text
refactor(bluetooth): keep the adapter out of shared state

- shared state holding a live adapter let the ui do D-Bus I/O directly
```

### 8.10 Dispatch bluetooth events in one place

- Match every event once in the main loop; split pure
  `apply_adapter_property` and `apply_device_property` functions. Remove the
  `unreachable!` arms and downgrade unhandled property logs to debug.

```text
refactor(bluetooth): dispatch events in one place

- three dispatch layers had unreachable arms that would panic after a
  careless refactor
```

### 8.11 Read bluetooth device properties in one call

- Build `DeviceInfo` from `device.all_properties()` folded through
  `apply_device_property`.

```text
perf(bluetooth): read device properties in one call

- seven sequential round trips per device slowed reconcile, and the
  field mapping duplicated the event applier
```

### 8.12 Skip bluetooth writes that don't change state

- Publish through `set_if_changed`.

```text
perf(bluetooth): skip writes that don't change state

- every reconcile woke all tiles and menus even when nothing changed
```

### 8.13 Reuse one proxy and skip redundant network writes

- Keep one `NetworkManagerProxy` for the service's lifetime and pass it to
  `fetch_network_info`; cache the wireless device path. Publish through
  `set_if_changed`.

```text
refactor(network): reuse one proxy and skip redundant writes

- every reconcile rebuilt several caching proxies and rewrote unchanged
  state
```

### 8.14 Resubscribe only when the device or access point changes

- Store the current paths in `SubscriptionTasks` and respawn only on change.
  Compare the first active access point value with the fetched path instead of
  discarding it.

```text
refactor(network): resubscribe only when the device or ap changes

- each reconcile aborted and respawned both subscriptions
- discarding the first stream value could miss a roam
```

### 8.15 Add a proxy helper and merge property watchers

- `async fn proxy<P>(conn, path) -> zbus::Result<P>`; merge the four property
  watchers into one stream and task. Drop `PropertyWatcherTasks`.

```text
refactor(network): add a proxy helper and merge property watchers

- proxy building was repeated about thirteen times and the four watchers
  were near-identical
```

### 8.16 Read access points concurrently

- `src/network/scan.rs`: fetch each access point with one
  `PropertiesProxy::get_all` and run them concurrently.

```text
perf(network): read access points concurrently

- access points were read one property at a time, inline in the service
  loop, delaying state updates during scans
```

### 8.17 Add network state predicates and flatten wifi info

- `State::is_connected()` and `State::is_offline()`.
- `NetworkInfo { device_kind, wifi: Option<WifiInfo { ssid, strength }> }`
  replaces `SpecificNetworkInfo`. A private `Fetched` struct replaces tuple
  returns.

```text
refactor(network): add state predicates and flatten wifi info

- connected and offline checks were copied in several places, and two
  enums described the same device kind
```

### 8.18 Split the network service into modules

- `service.rs` keeps the loop; add `service/commands.rs`,
  `service/connect_outcome.rs`, `service/fetch.rs` and `service/watchers.rs`.

```text
refactor(network): split the service into modules

- service.rs was over a thousand lines mixing commands, fetching and
  watchers
```

### 8.19 Split notification hints and the service loop into modules

- Move hints to `notifications/hints.rs` and the loop and D-Bus setup to
  `notifications/service.rs`; make submodules private with targeted re-exports.

```text
refactor(notifications): split hints and the service loop into modules

- notifications.rs mixed state, events, commands, hints and service
  setup
```

### 8.20 Add TileState with a single update message

- `src/widgets/tile.rs`: `TileState { icon, primary, secondary, attention,
tooltip, visible }` and `TileMsg::Set(TileState)`. Derive `Default` on
  `Attention`, `TileInit` and `ProgressTileInit`.

```text
feat(tiles): add TileState with a single update message

- tiles sent up to five set messages per update, re-rendering each time
```

### 8.21 Add a shared-state tile helper

- `state_tile(state: &'static SharedState<S>, view: fn(&S) -> Option<TileState>)`
  subscribes, seeds from the current value and hides the tile for `None`.

```text
feat(tiles): add a shared-state tile helper

- subscribing and seeding initial state was repeated in every tile and
  forgotten in three
```

### 8.22 Port the clock tile

```text
refactor(tiles): port the clock tile to the shared-state helper

- removes the wrapper box and per-field set messages
```

### 8.23 Port the mpris tile

```text
refactor(tiles): port the mpris tile to the shared-state helper

- removes the wrapper box and per-field set messages
```

### 8.24 Port the weather tile

```text
refactor(tiles): port the weather tile to the shared-state helper

- removes the wrapper box and per-field set messages
```

### 8.25 Port the brightness and volume tiles

- Give `ProgressTile` the same seeding behaviour and port both tiles.

```text
refactor(tiles): port the progress tiles to the shared-state helper

- brightness and volume repeated the same subscribe and seed code
```

### 8.26 Port the notifications tile

- Map state to the count instead of cloning the whole notification map.

```text
refactor(tiles): port the notifications tile to the shared-state helper

- every change cloned all notifications just to count them
```

### 8.27 Port the battery tile

```text
refactor(tiles): port the battery tile to the shared-state helper

- removes the wrapper box and per-field set messages
```

### 8.28 Add a shared bluetooth icon function and port the tile

- `bluetooth/state.rs`: `pub fn get_icon(state: &BluetoothState)` used by the
  tile and menu.

```text
refactor(tiles): port the bluetooth tile to the shared-state helper

- the tile and menu each had their own copy of the icon mapping
```

### 8.29 Port the network tile

```text
refactor(tiles): port the network tile to the shared-state helper

- removes the wrapper box and five set messages per update
```

### 8.30 Add a menu toggle helper

- `MenuWindow` gains `Toggle`, `Open` and `Close` inputs and owns its open
  state, emitting `Shown` and `Hidden`.

```text
feat(widgets): add a menu toggle helper

- tiles tracked open state themselves and had to handle MenuClosed
```

### 8.31 Use the menu toggle in network and bluetooth tiles

- Remove `menu_open` and `MenuClosed`; `NetworkMenu::SetOpen` follows
  `Shown` and `Hidden`, and the bluetooth menu stops discovery on `Hidden`.

```text
refactor(tiles): use the menu toggle in network and bluetooth tiles

- both tiles duplicated the same open state bookkeeping
```

### 8.32 Add shared menu header and prompt widgets

- New `src/widgets/menu_parts.rs`: `menu_root()`, `MenuHeader` (icon plus a
  switch with built-in signal blocking), `scrolled_content()`, and
  `PromptBox` with `gtk::PasswordEntry`, focus on show, `connect_submit` and
  `connect_cancel`.

```text
feat(widgets): add shared menu header and prompt widgets

- both menus built identical headers, switches and prompts
```

### 8.33 Add a menu list row with keyed sync

- New `src/widgets/menu_list_row.rs`: `MenuRowData`, `MenuRowOutput` and a
  generic `sync_rows` that removes, updates and inserts by key in a stable
  order. Unit tests for the diff.

```text
feat(widgets): add a menu list row with keyed sync

- both menus had the same row layout and the same quadratic diffing
```

### 8.34 Use the shared menu widgets in the network menu

```text
refactor(network-menu): use the shared menu widgets

- removes the duplicated header, prompt, row widget and diffing code
```

### 8.35 Split the network menu into submodules

- `network_menu.rs` keeps the component; move rendering helpers and the prompt
  handling into `network_menu/`.

```text
refactor(network-menu): split into submodules

- the file was well over the 500 line guideline
```

### 8.36 Use the shared menu widgets in the bluetooth menu

- Rows show the device type icon and sort by connected, then alias.

```text
refactor(bluetooth-menu): use the shared menu widgets

- removes the duplicated header, prompt, row widget and diffing code
- rows came out in hashmap order
```

### 8.37 Split the bluetooth menu into submodules

- Move the pairing prompt mapping into a pure function in
  `bluetooth_menu/pairing_prompt.rs`.

```text
refactor(bluetooth-menu): split into submodules

- the file was well over the 500 line guideline
```

### 8.38 Build bar groups as plain boxes

- `Bar::init` builds three `gtk::Box`es and keeps the tile controllers; delete
  `widgets/bar/left.rs`, `center.rs` and `right.rs` components.

```text
refactor(bar): build bar groups as plain boxes

- the group components only did layout and relayed messages
```

### 8.39 Make the bar a synchronous component

```text
refactor(bar): make the bar a synchronous component

- its async init never awaited anything
```

### 8.40 Subscribe the tray widget to the client directly

- Pass `Option<Arc<TrayClient>>` to `TrayWidget`; subscribe with
  `sender.command` and activate with `oneshot_command`. Remove the relay
  variants in app, bar and tray and the `tokio::Mutex`.

```text
refactor(tray): subscribe the tray widget to the client directly

- tray events and activations hopped through four components each way
- the client methods take &self, so the mutex was unnecessary
```

### 8.41 Move service startup into a services module

- New `src/services.rs` with `pub fn start_all(sender)` looping over boxed
  service futures.

```text
refactor(app): move service startup into a services module

- app.rs repeated the same command block for every service and mixed
  service startup with monitor handling
```

### 8.42 Handle notification events in update_cmd

- Remove `CadenzaShellMsg::NotificationEvent` and handle the event where it
  arrives.

```text
refactor(app): handle notification events in update_cmd

- events were received as command output only to be re-sent as input
```

## Phase 9: features

### 9.1 Track whether this shell owns the discovery session

- Publish `discovery_owned: bool` in `BluetoothState`, set while our discovery
  task holds the session. The scan button follows it.

```text
feat(bluetooth): track whether this shell owns discovery

- the scan button followed bluez's global flag, so another client's scan
  showed "stop scanning" while stopping did nothing
```

### 9.2 Report bluetooth action outcomes to the menu

- `BluetoothOutcome` events on an `EventBus`: started, succeeded and failed
  (with a friendly reason) per device and action.

```text
feat(bluetooth): report action outcomes to the menu

- connect, pair, remove and power failures were only logged
```

### 9.3 Show busy and error states for devices

- Rows show a spinner while an action is in flight and ignore further clicks;
  failures show an inline error that clears on the next action.

```text
feat(bluetooth-menu): show busy and error states for devices

- double clicks started concurrent connects and failures were invisible
```

### 9.4 Connect devices after pairing

- After a successful `pair`, mark the device trusted and connect it.

```text
feat(bluetooth): connect devices after pairing

- users had to click a freshly paired device a second time to use it
```

### 9.5 Prompt for service authorization

- Implement `authorize_service` with a new `PairingPrompt::AuthorizeService
{ device, service }` variant, mapping common UUIDs to readable names, and
  allow or deny buttons in the menu.

```text
feat(bluetooth): prompt for service authorization

- without a handler bluer rejects every request, so untrusted paired
  devices couldn't connect profiles
```

### 9.6 Open the bluetooth menu on the focused monitor for pairing **(manual check)**

- Pass each bluetooth tile its monitor connector. When `PAIRING_PROMPT` becomes
  `Some`, the tile whose connector matches niri's focused output opens its
  menu.

```text
feat(bluetooth): open the menu on the focused monitor for pairing

- incoming pairing requests timed out unseen when the menu was closed
```

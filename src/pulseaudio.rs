use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, bail};
use pulse::{
    callbacks::ListResult,
    context::{
        Context, FlagSet, State,
        introspect::{ServerInfo, SinkInfo},
        subscribe::{Facility, InterestMaskSet, Operation},
    },
    mainloop::standard::{IterateResult, Mainloop},
    proplist::Proplist,
    volume::{ChannelVolumes, Volume},
};
use relm4::SharedState;

use crate::utils::{backoff::Backoff, shared_state::set_if_changed};

pub static VOLUME_STATE: SharedState<PulseAudioData> = SharedState::new();

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PulseAudioData {
    pub volume: f64,
    pub muted: bool,
    pub default_sink_name: Option<String>,
}

/// The context is only ever touched from the session thread, so shared
/// ownership between callbacks doesn't need a lock.
type SharedContext = Rc<RefCell<Context>>;

/// A session that lasts this long is considered healthy, so the reconnect
/// backoff starts over afterwards.
const STABLE_SESSION: Duration = Duration::from_secs(60);

/// Runs the pulseaudio client forever, reconnecting whenever the sound
/// server goes away (e.g. pipewire-pulse restarting or not running yet at
/// login).
///
/// libpulse's standard main loop blocks while it waits for events, so each
/// session runs on a dedicated blocking thread instead of a relm4 runtime
/// worker.
pub async fn run_pulseaudio_loop() {
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));

    loop {
        let started = Instant::now();
        match tokio::task::spawn_blocking(run_session).await {
            Ok(Ok(())) => tracing::info!("pulseaudio session ended"),
            Ok(Err(e)) => tracing::warn!("pulseaudio session ended: {e:#}"),
            Err(e) => tracing::error!("pulseaudio session thread panicked: {e}"),
        }

        // hide the volume tile until we're connected again
        set_if_changed(&VOLUME_STATE, PulseAudioData::default());

        if started.elapsed() >= STABLE_SESSION {
            backoff.reset();
        }
        tokio::time::sleep(backoff.next()).await;
    }
}

/// Connects to the sound server and drives the main loop on the current
/// thread until the connection fails or terminates.
fn run_session() -> anyhow::Result<()> {
    let mut proplist = Proplist::new().context("couldn't create a pulseaudio proplist")?;
    if proplist
        .set_str("APPLICATION_NAME", "cadenza-shell")
        .is_err()
    {
        bail!("couldn't set the pulseaudio application name");
    }

    let mut mainloop = Mainloop::new().context("couldn't create a pulseaudio main loop")?;
    let context = Context::new_with_proplist(&mainloop, "cadenza-shell", &proplist)
        .context("couldn't create a pulseaudio context")?;
    let context = Rc::new(RefCell::new(context));

    let state_callback = Box::new({
        let context = context.clone();
        move || on_state_change(&context)
    });
    context
        .borrow_mut()
        .set_state_callback(Some(state_callback));

    // bind the result first so the mutable borrow ends before the state
    // callback can run again
    let connected = context
        .borrow_mut()
        .connect(None, FlagSet::NOAUTOSPAWN, None);

    let result = match connected {
        Ok(()) => drive(&mut mainloop, &context),
        Err(e) => Err(anyhow::anyhow!("couldn't connect to the sound server: {e}")),
    };

    // the callbacks hold clones of the context, so clear them to break the
    // reference cycle before the next session starts
    let mut context = context.borrow_mut();
    context.set_state_callback(None);
    context.set_subscribe_callback(None);
    context.disconnect();

    result
}

/// Iterates the main loop until the context fails or terminates.
fn drive(mainloop: &mut Mainloop, context: &SharedContext) -> anyhow::Result<()> {
    loop {
        match mainloop.iterate(true) {
            IterateResult::Success(_) => {}
            IterateResult::Err(e) => bail!("pulseaudio main loop error: {e}"),
            IterateResult::Quit(_) => return Ok(()),
        }

        match context.borrow().get_state() {
            State::Failed => bail!("lost the connection to the sound server"),
            State::Terminated => return Ok(()),
            _ => {}
        }
    }
}

fn on_state_change(context: &SharedContext) {
    // connect() can report state changes while it still holds the borrow
    let Ok(state) = context.try_borrow().map(|context| context.get_state()) else {
        return;
    };

    // the main loop notices failure and termination and ends the session
    if state != State::Ready {
        return;
    }

    tracing::info!("connected to pulseaudio server");

    let introspect = context.borrow().introspect();

    // get default sink info
    introspect.get_server_info({
        let context = context.clone();
        move |server_info| on_server_info(server_info, &context)
    });

    // subscribe to changes
    let subscribe_callback = Box::new({
        let context = context.clone();
        move |facility, op, index| on_event(&context, facility, op, index)
    });

    let mut context = context.borrow_mut();
    context.set_subscribe_callback(Some(subscribe_callback));
    context.subscribe(InterestMaskSet::SERVER | InterestMaskSet::SINK, |_| ());
}

/// Applies `update` to a copy of the volume state and publishes it only if
/// something changed, so unrelated sink events don't wake the tile.
fn update_volume_state(update: impl FnOnce(&mut PulseAudioData)) {
    let mut data = VOLUME_STATE.read().clone();
    update(&mut data);
    set_if_changed(&VOLUME_STATE, data);
}

fn on_server_info(server_info: &ServerInfo, context: &SharedContext) {
    let default_sink_name = server_info
        .default_sink_name
        .as_ref()
        .map(ToString::to_string);

    update_volume_state(|data| data.default_sink_name = default_sink_name.clone());

    // get sink info for the default sink
    if let Some(ref sink_name) = default_sink_name {
        let introspect = context.borrow().introspect();
        introspect.get_sink_info_by_name(sink_name, on_sink_info);
    }
}

fn on_sink_info(sink_info: ListResult<&SinkInfo>) {
    let ListResult::Item(info) = sink_info else {
        return;
    };

    // events arrive for every sink; only the default one drives the tile
    let is_default = VOLUME_STATE.read().default_sink_name.as_deref() == info.name.as_deref();
    if !is_default {
        return;
    }

    let volume = volume_to_percent(&info.volume);
    update_volume_state(|data| {
        data.volume = volume;
        data.muted = info.mute;
    });
}

fn on_event(
    context: &SharedContext,
    facility: Option<Facility>,
    operation: Option<Operation>,
    index: u32,
) {
    match facility {
        Some(Facility::Server) => {
            let introspect = context.borrow().introspect();
            introspect.get_server_info({
                let context = context.clone();
                move |server_info| on_server_info(server_info, &context)
            });
        }
        // removed sinks can't be queried; a removed default sink is followed
        // by a server event announcing the new default
        Some(Facility::Sink) if operation != Some(Operation::Removed) => {
            let introspect = context.borrow().introspect();
            introspect.get_sink_info_by_index(index, on_sink_info);
        }
        _ => {}
    }
}

/// Converts a sink's channel volumes into a percentage of normal volume.
fn volume_to_percent(channel_volumes: &ChannelVolumes) -> f64 {
    if channel_volumes.len() == 0 {
        return 0.0;
    }

    let avg = f64::from(channel_volumes.avg().0);
    let normal = f64::from(Volume::NORMAL.0);
    (avg * 100.0 / normal).round()
}

fn _percent_to_volume(target_percent: f64) -> u32 {
    let base_delta = (Volume::NORMAL.0 as f32 - Volume::MUTED.0 as f32) / 100.0;

    if target_percent < 0.0 {
        Volume::MUTED.0
    } else if target_percent == 100.0 {
        Volume::NORMAL.0
    } else if target_percent >= 150.0 {
        (Volume::NORMAL.0 as f32 * 1.5) as u32
    } else if target_percent < 100.0 {
        Volume::MUTED.0 + (target_percent * base_delta as f64) as u32
    } else {
        Volume::NORMAL.0 + ((target_percent - 100.0) * base_delta as f64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_volume_is_one_hundred_percent() {
        let mut volumes = ChannelVolumes::default();
        volumes.set(2, Volume::NORMAL);
        assert_eq!(volume_to_percent(&volumes), 100.0);
    }

    #[test]
    fn averages_channels() {
        let mut volumes = ChannelVolumes::default();
        volumes.set(2, Volume::NORMAL);
        volumes.get_mut()[1] = Volume::MUTED;
        assert_eq!(volume_to_percent(&volumes), 50.0);
    }

    #[test]
    fn no_channels_is_zero() {
        assert_eq!(volume_to_percent(&ChannelVolumes::default()), 0.0);
    }
}

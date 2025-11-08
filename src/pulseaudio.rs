use std::sync::{Arc, Mutex};

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

pub static VOLUME_STATE: SharedState<PulseAudioData> = SharedState::new();

#[derive(Debug, Clone)]
pub struct PulseAudioData {
    pub volume: f64,
    pub muted: bool,
    pub default_sink_name: Option<String>,
}

impl Default for PulseAudioData {
    fn default() -> Self {
        Self {
            volume: 0.0,
            muted: false,
            default_sink_name: None,
        }
    }
}

pub async fn run_pulseaudio_loop() {
    let Some(mut proplist) = Proplist::new() else {
        log::error!("failed to create pulseaudio proplist");
        return;
    };

    if proplist
        .set_str("APPLICATION_NAME", "cadenza-shell")
        .is_err()
    {
        log::error!("failed to update pulseaudio proplist");
        return;
    }

    let Some(mut mainloop) = Mainloop::new() else {
        log::error!("failed to create pulseaudio mainloop");
        return;
    };

    let Some(context) = Context::new_with_proplist(&mainloop, "cadenza-shell", &proplist) else {
        log::error!("failed to create pulseaudio context");
        return;
    };

    let context = Arc::new(Mutex::new(context));

    let state_callback = Box::new({
        let context = context.clone();
        move || on_state_change(&context)
    });

    context
        .lock()
        .unwrap()
        .set_state_callback(Some(state_callback));

    if let Err(err) = context
        .lock()
        .unwrap()
        .connect(None, FlagSet::NOAUTOSPAWN, None)
    {
        log::error!("failed to connect to pulse: {}", err);
        return;
    }

    // run mainloop
    loop {
        match mainloop.iterate(true) {
            IterateResult::Success(_) => {}
            IterateResult::Err(err) => {
                log::error!("pulse mainloop error: {:?}", err);
            }
            IterateResult::Quit(_) => break,
        }
    }
}

fn on_state_change(context: &Arc<Mutex<Context>>) {
    let Ok(state) = context.try_lock().map(|lock| lock.get_state()) else {
        return;
    };

    match state {
        State::Ready => {
            log::info!("connected to pulseaudio server");

            let introspect = context.lock().unwrap().introspect();

            // get default sink info
            introspect.get_server_info({
                let context = context.clone();

                move |server_info| on_server_info(server_info, &context)
            });

            // subscribe to changes
            let subscribe_callback = Box::new({
                let context = context.clone();

                move |facility, op, _i| on_event(&context, facility, op)
            });

            context
                .lock()
                .unwrap()
                .set_subscribe_callback(Some(subscribe_callback));
            context
                .lock()
                .unwrap()
                .subscribe(InterestMaskSet::SERVER | InterestMaskSet::SINK, |_| ());
        }
        State::Failed => {
            log::error!("failed to connect to pulseaudio server");
            VOLUME_STATE.write().default_sink_name = None;
        }
        State::Terminated => {
            log::warn!("connection to pulseaudio server terminated");
        }
        _ => {}
    }
}

fn on_server_info(server_info: &ServerInfo, context: &Arc<Mutex<Context>>) {
    let default_sink_name = server_info
        .default_sink_name
        .as_ref()
        .map(ToString::to_string);

    {
        let mut data_guard = VOLUME_STATE.write();
        data_guard.default_sink_name = default_sink_name.clone();
    }

    // get sink info for the default sink
    if let Some(ref sink_name) = default_sink_name {
        let introspect = context.lock().unwrap().introspect();
        introspect.get_sink_info_by_name(sink_name, on_sink_info);
    }
}

fn on_sink_info(sink_info: ListResult<&SinkInfo>) {
    let ListResult::Item(info) = sink_info else {
        return;
    };

    let volume_percent = volume_to_percent(&info.volume);

    let mut data_guard = VOLUME_STATE.write();
    data_guard.volume = volume_percent;
    data_guard.muted = info.mute;
}

fn on_event(context: &Arc<Mutex<Context>>, facility: Option<Facility>, _op: Option<Operation>) {
    let Some(facility) = facility else {
        return;
    };

    match facility {
        Facility::Server => {
            let introspect = context.lock().unwrap().introspect();
            introspect.get_server_info({
                let context = context.clone();

                move |server_info| on_server_info(server_info, &context)
            });
        }
        Facility::Sink => {
            // update default sink info
            if let Some(sink_name) = VOLUME_STATE.read().default_sink_name.clone() {
                let introspect = context.lock().unwrap().introspect();
                introspect.get_sink_info_by_name(&sink_name, {
                    move |sink_info| on_sink_info(sink_info)
                });
            }
        }
        _ => {}
    }
}

fn volume_to_percent(channel_volumes: &ChannelVolumes) -> f64 {
    if channel_volumes.len() == 0 {
        return 0.0;
    }

    let avg: u32 =
        channel_volumes.get().iter().map(|v| v.0).sum::<u32>() / channel_volumes.len() as u32;
    let base_delta = (Volume::NORMAL.0 - Volume::MUTED.0) as f64 / 100.0;

    ((avg - Volume::MUTED.0) as f64 / base_delta)
        .round()
        .max(0.0)
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

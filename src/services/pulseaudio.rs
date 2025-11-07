use std::sync::{Arc, Mutex};

use anyhow::Result;
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
use relm4::{Reducer, Reducible};

pub static VOLUME_STATE: Reducer<PulseAudioData> = Reducer::new();

#[derive(Debug)]
pub enum PulseAudioReduceMsg {
    Volume(f64),
    Mute(bool),
    DefaultSinkName(String),
}

#[derive(Default, Debug, Clone)]
pub struct PulseAudioData {
    pub volume: Option<f64>,
    pub muted: Option<bool>,
    pub default_sink_name: Option<String>,
}

impl Reducible for PulseAudioData {
    type Input = PulseAudioReduceMsg;

    fn init() -> Self {
        // start pulseaudio connection in a separate thread
        relm4::spawn(async move {
            if let Err(e) = run_pulse_loop() {
                log::error!("pulse loop error: {}", e);
            }
        });

        Default::default()
    }

    fn reduce(&mut self, message: Self::Input) -> bool {
        match message {
            PulseAudioReduceMsg::Volume(v) => {
                if Some(v) != self.volume {
                    self.volume = Some(v);
                    true
                } else {
                    false
                }
            }
            PulseAudioReduceMsg::Mute(m) => {
                if Some(m) != self.muted {
                    self.muted = Some(m);
                    true
                } else {
                    false
                }
            }
            PulseAudioReduceMsg::DefaultSinkName(d) => {
                if Some(&d) != self.default_sink_name.as_ref() {
                    self.default_sink_name = Some(d);
                    true
                } else {
                    false
                }
            }
        }
    }
}

fn run_pulse_loop() -> Result<()> {
    let Some(mut proplist) = Proplist::new() else {
        anyhow::bail!("failed to create pulseaudio proplist");
    };

    if proplist
        .set_str("APPLICATION_NAME", "cadenza-shell")
        .is_err()
    {
        anyhow::bail!("failed to update pulseaudio proplist");
    }

    let Some(mut mainloop) = Mainloop::new() else {
        anyhow::bail!("failed to create pulseaudio mainloop");
    };

    let Some(context) = Context::new_with_proplist(&mainloop, "cadenza-shell", &proplist) else {
        anyhow::bail!("failed to create pulseaudio context");
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
        anyhow::bail!("failed to connect to pulse: {}", err);
    }

    // run mainloop
    loop {
        match mainloop.iterate(true) {
            IterateResult::Success(_) => continue,
            IterateResult::Err(err) => {
                log::error!("pulse mainloop error: {:?}", err);
            }
            IterateResult::Quit(_) => break,
        }
    }

    Ok(())
}

fn on_state_change(context: &Arc<Mutex<Context>>, default_sink_name: Option<String>) {
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

                move |facility, op, _i| on_event(&context, facility, op, default_sink_name)
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

    // get sink info for the default sink
    if let Some(ref sink_name) = default_sink_name {
        VOLUME_STATE.emit(PulseAudioReduceMsg::DefaultSinkName(sink_name.to_string()));
        let introspect = context.lock().unwrap().introspect();
        introspect.get_sink_info_by_name(sink_name, move |sink_info| on_sink_info(sink_info));
    }
}

fn on_sink_info(sink_info: ListResult<&SinkInfo>) {
    let ListResult::Item(info) = sink_info else {
        return;
    };

    let volume_percent = volume_to_percent(&info.volume);

    VOLUME_STATE.emit(PulseAudioReduceMsg::Volume(volume_percent));
    VOLUME_STATE.emit(PulseAudioReduceMsg::Mute(info.mute));
}

fn on_event(
    context: &Arc<Mutex<Context>>,
    facility: Option<Facility>,
    _op: Option<Operation>,
    default_sink_name: Option<String>,
) {
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
            if let Some(sink_name) = default_sink_name {
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

fn percent_to_volume(target_percent: f64) -> u32 {
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

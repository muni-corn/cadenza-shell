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
use relm4::Worker;
use tokio::sync::{broadcast, mpsc};

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

#[derive(Debug)]
pub enum PulseAudioServiceMsg {
    SetVolume(f64),
    ToggleMute,
}

#[derive(Debug, Clone)]
pub enum PulseAudioServiceEvent {
    VolumeChanged(PulseAudioData),
    Error(String),
}

// internal command to send to pulse thread
#[derive(Debug)]
enum PulseCommand {
    SetVolume(f64),
    SetMute(bool),
}

#[derive(Debug)]
pub struct PulseAudioService {
    data: PulseAudioData,
    pulse_tx: Option<mpsc::UnboundedSender<PulseCommand>>,
    tx: broadcast::Sender<PulseAudioData>,
    _rx: broadcast::Receiver<PulseAudioData>,
}

impl Worker for PulseAudioService {
    type Init = ();
    type Input = PulseAudioServiceMsg;
    type Output = PulseAudioServiceEvent;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        let (tx, rx) = broadcast::channel(32);
        let (pulse_tx, pulse_rx) = mpsc::unbounded_channel();

        let worker = Self {
            data: PulseAudioData::default(),
            pulse_tx: Some(pulse_tx),
            tx,
            _rx: rx,
        };

        // start pulseaudio connection in a separate thread
        let sender_clone = sender.clone();
        let tx_clone = worker.tx.clone();

        relm4::spawn(async move {
            if let Err(e) = Self::run_pulse_loop(sender_clone, tx_clone, pulse_rx) {
                log::error!("pulse loop error: {}", e);
            }
        });

        worker
    }

    fn update(&mut self, message: Self::Input, _sender: relm4::ComponentSender<Self>) {
        if let Some(ref pulse_tx) = self.pulse_tx {
            let command = match message {
                PulseAudioServiceMsg::SetVolume(volume) => PulseCommand::SetVolume(volume),
                PulseAudioServiceMsg::ToggleMute => PulseCommand::SetMute(!self.data.muted),
            };

            if let Err(e) = pulse_tx.send(command) {
                log::error!("failed to send pulse command: {}", e);
            }
        }
    }
}

impl PulseAudioService {
    fn run_pulse_loop(
        sender: relm4::ComponentSender<PulseAudioService>,
        tx: broadcast::Sender<PulseAudioData>,
        mut pulse_rx: mpsc::UnboundedReceiver<PulseCommand>,
    ) -> Result<()> {
        let Some(mut proplist) = Proplist::new() else {
            anyhow::bail!("failed to create pa proplist");
        };

        if proplist.set_str("APPLICATION_NAME", "muse-shell").is_err() {
            anyhow::bail!("failed to update pa proplist");
        }

        let Some(mut mainloop) = Mainloop::new() else {
            anyhow::bail!("failed to create pa mainloop");
        };

        let Some(context) = Context::new_with_proplist(&mainloop, "muse-shell", &proplist) else {
            anyhow::bail!("failed to create pa context");
        };

        let context = Arc::new(Mutex::new(context));
        let data = Arc::new(Mutex::new(PulseAudioData::default()));
        let pending_commands = Arc::new(Mutex::new(Vec::<PulseCommand>::new()));

        let state_callback = Box::new({
            let context = context.clone();
            let data = data.clone();
            let tx = tx.clone();
            let sender = sender.clone();

            move || Self::on_state_change(&context, &data, &tx, &sender)
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

        // handle incoming commands
        let context_clone = context.clone();
        let data_clone = data.clone();
        let pending_commands_clone = pending_commands.clone();
        relm4::spawn(async move {
            while let Some(command) = pulse_rx.blocking_recv() {
                // either execute immediately if connected, or queue for later
                if let Ok(ctx) = context_clone.try_lock() {
                    if ctx.get_state() == State::Ready {
                        Self::execute_command(&ctx, &data_clone, command);
                    } else {
                        pending_commands_clone.lock().unwrap().push(command);
                    }
                } else {
                    pending_commands_clone.lock().unwrap().push(command);
                }
            }
        });

        // run mainloop
        loop {
            match mainloop.iterate(true) {
                IterateResult::Success(_) => {
                    // process any pending commands
                    if let (Ok(ctx), Ok(mut pending)) =
                        (context.try_lock(), pending_commands.try_lock())
                        && ctx.get_state() == State::Ready
                        && !pending.is_empty()
                    {
                        for command in pending.drain(..) {
                            Self::execute_command(&ctx, &data, command);
                        }
                    }
                }
                IterateResult::Err(err) => {
                    log::error!("pulse mainloop error: {:?}", err);
                }
                IterateResult::Quit(_) => break,
            }
        }

        Ok(())
    }

    fn execute_command(
        context: &Context,
        data: &Arc<Mutex<PulseAudioData>>,
        command: PulseCommand,
    ) {
        let default_sink_name = data.lock().unwrap().default_sink_name.clone();
        let Some(sink_name) = default_sink_name else {
            return;
        };

        let mut introspector = context.introspect();

        match command {
            PulseCommand::SetVolume(volume) => {
                let clamped_volume = volume.clamp(0.0, 150.0);
                let pulse_volume = Self::percent_to_volume(clamped_volume);

                let mut channel_volumes = ChannelVolumes::default();
                channel_volumes.set_len(2); // assume stereo
                for i in 0..channel_volumes.len() {
                    channel_volumes.get_mut()[i as usize] = Volume(pulse_volume);
                }

                introspector.set_sink_volume_by_name(&sink_name, &channel_volumes, None);
            }
            PulseCommand::SetMute(muted) => {
                introspector.set_sink_mute_by_name(&sink_name, muted, None);
            }
        }
    }

    fn on_state_change(
        context: &Arc<Mutex<Context>>,
        data: &Arc<Mutex<PulseAudioData>>,
        tx: &broadcast::Sender<PulseAudioData>,
        sender: &relm4::ComponentSender<PulseAudioService>,
    ) {
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
                    let data = data.clone();
                    let tx = tx.clone();
                    let sender = sender.clone();

                    move |server_info| {
                        Self::on_server_info(server_info, &context, &data, &tx, &sender)
                    }
                });

                // subscribe to changes
                let subscribe_callback = Box::new({
                    let context = context.clone();
                    let data = data.clone();
                    let tx = tx.clone();
                    let sender = sender.clone();

                    move |facility, op, _i| {
                        Self::on_event(&context, &data, &tx, &sender, facility, op)
                    }
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
                let _ = sender.output(PulseAudioServiceEvent::Error(
                    "failed to connect to pulseaudio server".to_string(),
                ));
            }
            State::Terminated => {
                log::warn!("connection to pulseaudio server terminated");
            }
            _ => {}
        }
    }

    fn on_server_info(
        server_info: &ServerInfo,
        context: &Arc<Mutex<Context>>,
        data: &Arc<Mutex<PulseAudioData>>,
        tx: &broadcast::Sender<PulseAudioData>,
        sender: &relm4::ComponentSender<PulseAudioService>,
    ) {
        let default_sink_name = server_info
            .default_sink_name
            .as_ref()
            .map(ToString::to_string);

        {
            let mut data_guard = data.lock().unwrap();
            data_guard.default_sink_name = default_sink_name.clone();
        }

        // get sink info for the default sink
        if let Some(ref sink_name) = default_sink_name {
            let introspect = context.lock().unwrap().introspect();
            introspect.get_sink_info_by_name(sink_name, {
                let data = data.clone();
                let tx = tx.clone();
                let sender = sender.clone();

                move |sink_info| Self::on_sink_info(sink_info, &data, &tx, &sender)
            });
        }
    }

    fn on_sink_info(
        sink_info: ListResult<&SinkInfo>,
        data: &Arc<Mutex<PulseAudioData>>,
        tx: &broadcast::Sender<PulseAudioData>,
        sender: &relm4::ComponentSender<PulseAudioService>,
    ) {
        let ListResult::Item(info) = sink_info else {
            return;
        };

        let volume_percent = Self::volume_to_percent(&info.volume);

        let mut data_guard = data.lock().unwrap();
        data_guard.volume = volume_percent;
        data_guard.muted = info.mute;

        let volume_data = data_guard.clone();
        drop(data_guard);

        let _ = tx.send(volume_data.clone());
        let _ = sender.output(PulseAudioServiceEvent::VolumeChanged(volume_data));
    }

    fn on_event(
        context: &Arc<Mutex<Context>>,
        data: &Arc<Mutex<PulseAudioData>>,
        tx: &broadcast::Sender<PulseAudioData>,
        sender: &relm4::ComponentSender<PulseAudioService>,
        facility: Option<Facility>,
        _op: Option<Operation>,
    ) {
        let Some(facility) = facility else {
            return;
        };

        match facility {
            Facility::Server => {
                let introspect = context.lock().unwrap().introspect();
                introspect.get_server_info({
                    let context = context.clone();
                    let data = data.clone();
                    let tx = tx.clone();
                    let sender = sender.clone();

                    move |server_info| {
                        Self::on_server_info(server_info, &context, &data, &tx, &sender)
                    }
                });
            }
            Facility::Sink => {
                // update default sink info
                let default_sink_name = data.lock().unwrap().default_sink_name.clone();
                if let Some(sink_name) = default_sink_name {
                    let introspect = context.lock().unwrap().introspect();
                    introspect.get_sink_info_by_name(&sink_name, {
                        let data = data.clone();
                        let tx = tx.clone();
                        let sender = sender.clone();

                        move |sink_info| Self::on_sink_info(sink_info, &data, &tx, &sender)
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
}

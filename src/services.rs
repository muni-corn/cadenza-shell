//! Starting the background services the shell's tiles and menus rely on.

use std::{future::Future, pin::Pin};

use relm4::{AsyncComponentSender, component::AsyncComponent};

use crate::{
    battery::start_battery_service, bluetooth::run_bluetooth_service,
    brightness::start_brightness_watcher, mpris::run_mpris_service, network::run_network_service,
    niri, notifications::run_notifications_service, pulseaudio::run_pulseaudio_loop,
    sleep_monitor::run_sleep_monitor, weather::start_weather_polling,
};

type Service = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Starts every service as a command of the component behind `sender`, so
/// they all stop when it shuts down.
///
/// Every service channel is created lazily on first use, so the order
/// services start in doesn't matter.
pub fn start_all<C: AsyncComponent>(sender: &AsyncComponentSender<C>) {
    let services: [Service; 10] = [
        Box::pin(run_sleep_monitor()),
        // registers the notifications daemon on the session bus
        Box::pin(run_notifications_service()),
        Box::pin(start_battery_service()),
        Box::pin(run_bluetooth_service()),
        Box::pin(start_brightness_watcher()),
        Box::pin(run_network_service()),
        Box::pin(start_weather_polling()),
        Box::pin(run_mpris_service()),
        Box::pin(niri::start_event_listener()),
        Box::pin(run_pulseaudio_loop()),
    ];

    for service in services {
        sender.command(|_, shutdown| shutdown.register(service).drop_on_shutdown());
    }
}

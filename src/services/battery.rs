use std::{thread, time::Duration};

use relm4::Worker;
use systemstat::{Platform, System};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum BatteryUpdate {
    Stats {
        percentage: f64,
        charging: bool,
        time_remaining: Duration,
    },
    Unavailable,
}

pub struct BatteryService;

impl Worker for BatteryService {
    type Init = ();
    type Input = ();
    type Output = BatteryUpdate;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        // use std::thread::spawn for the update loop
        thread::spawn(move || {
            let system = System::new();
            loop {
                match read_battery_state(&system) {
                    Ok((percentage, charging, time_remaining)) => {
                        sender
                            .output(BatteryUpdate::Stats {
                                percentage,
                                charging,
                                time_remaining,
                            })
                            .unwrap_or_else(|_| log::error!("couldn't send battery state update"));
                    }
                    Err(e) => {
                        log::error!("couldn't read battery state: {}", e);
                        sender
                            .output(BatteryUpdate::Unavailable)
                            .unwrap_or_else(|_| {
                                log::error!("couldn't send battery unavailability message")
                            });

                        // stop trying lol
                        break;
                    }
                }

                thread::sleep(Duration::from_secs(10));
            }
        });

        BatteryService
    }

    // inputs are ignored
    fn update(&mut self, _message: Self::Input, _sender: relm4::ComponentSender<Self>) {}
}

/// Returns the percentage remaining, whether the battery is charging, and how
/// much time is remaining.
fn read_battery_state(system: &System) -> anyhow::Result<(f64, bool, Duration)> {
    let battery_life = system.battery_life()?;

    // get percentage (0.0 to 1.0)
    let percentage = battery_life.remaining_capacity as f64;

    // get time remaining
    let time_remaining = battery_life.remaining_time;

    let charging = system.on_ac_power().ok().unwrap_or(false);

    Ok((percentage, charging, time_remaining))
}

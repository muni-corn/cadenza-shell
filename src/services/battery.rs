use std::{fs, path::Path, sync::mpsc, thread, time::Duration};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use relm4::Worker;
use systemstat::{Platform, System};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum BatteryUpdate {
    Stats {
        percentage: f32,
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
        // detect battery interface
        let Some(battery_interface) = detect_battery_interface() else {
            log::error!("couldn't detect battery interface");
            sender
                .output(BatteryUpdate::Unavailable)
                .unwrap_or_else(|_| log::error!("couldn't send unavailability message"));

            return Self;
        };

        let system = System::new();

        // read initial battery properties. if any fail, we will not consider the
        // service available.
        let Ok((percentage, charging, time_remaining)) = read_battery_state(&system)
            .map_err(|e| log::error!("couldn't read initial battery state: {}", e))
        else {
            sender
                .output(BatteryUpdate::Unavailable)
                .unwrap_or_else(|_| log::error!("couldn't send unavailability message"));
            return BatteryService;
        };

        // send initial update
        sender
            .output(BatteryUpdate::Stats {
                percentage,
                charging,
                time_remaining,
            })
            .unwrap_or_else(|_| {
                log::error!("couldn't send initial battery state");
            });

        relm4::spawn(async move {
            let (tx, rx) = mpsc::channel();

            // watch only the status file for instant updates
            let mut watcher = match notify::recommended_watcher(tx) {
                Ok(watcher) => watcher,
                Err(e) => {
                    log::error!("couldn't create watcher: {}", e);
                    return;
                }
            };

            // Watch status file for charging state changes
            let status_path = format!("/sys/class/power_supply/{}/status", battery_interface);

            if let Err(e) = watcher.watch(Path::new(&status_path), RecursiveMode::NonRecursive) {
                log::error!("couldn't set up watcher for {}: {}", status_path, e);
                return;
            }

            loop {
                // poll every 30 seconds
                thread::sleep(Duration::from_secs(30));

                // waits on file changes
                if let Err(e) = rx.recv() {
                    log::error!("battery status watcher died: {}", e);
                    break;
                };

                match read_battery_state(&system) {
                    Ok((percentage, charging, time_remaining)) => {
                        sender
                            .output(BatteryUpdate::Stats {
                                percentage,
                                charging,
                                time_remaining,
                            })
                            .unwrap_or_else(|_| {
                                log::error!("couldn't send battery update");
                            });
                    }
                    Err(e) => {
                        log::error!("couldn't read battery state: {}", e);
                    }
                }
            }
        });

        Self
    }

    // inputs are ignored
    fn update(&mut self, _message: Self::Input, _sender: relm4::ComponentSender<Self>) {}
}

/// Detect the battery interface by scanning /sys/class/power_supply/ for
/// devices with type "Battery"
fn detect_battery_interface() -> Option<String> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    // Read all entries in the power_supply directory
    fs::read_dir(power_supply_path).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();

        // Check if this is a directory
        if path.is_dir() {
            // Check if there's a type file
            let type_path = path.join("type");
            if type_path.exists() {
                // Read the type file
                let type_content = fs::read_to_string(&type_path).ok()?;
                if type_content.trim() == "Battery" {
                    // Found a battery device, return its name
                    Some(entry.file_name().to_string_lossy().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    })
}

/// Returns the percentage remaining, whether the battery is charging, and how
/// much time is remaining.
fn read_battery_state(system: &System) -> Result<(f32, bool, Duration)> {
    let battery_info = system.battery_life()?;

    let percentage = battery_info.remaining_capacity;
    let charging = system.on_ac_power()?;
    let time_remaining = battery_info.remaining_time;

    Ok((percentage, charging, time_remaining))
}

use std::{fs, path::Path, time::Duration};

use anyhow::Result;
use relm4::{Worker, prelude::*};
use tokio::time;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatteryStatus {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone)]
pub struct BatteryData {
    pub percentage: u32,
    pub status: BatteryStatus,
    pub available: bool,
    pub time_remaining: i32, // minutes, -1 if unknown
}

impl Default for BatteryData {
    fn default() -> Self {
        Self {
            percentage: 0,
            status: BatteryStatus::Unknown,
            available: false,
            time_remaining: -1,
        }
    }
}

#[derive(Debug)]
pub enum BatteryWorkerMsg {
    RequestUpdate,
}

#[derive(Debug)]
pub struct BatteryWorker {
    battery_path: Option<String>,
    last_data: BatteryData,
}

impl BatteryWorker {
    fn detect_battery() -> Result<String> {
        let power_supply_path = Path::new("/sys/class/power_supply");
        let entries = fs::read_dir(power_supply_path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Check if this is a battery (not AC adapter)
            if let Ok(type_content) = fs::read_to_string(path.join("type")) {
                if type_content.trim() == "Battery" {
                    return Ok(path.to_string_lossy().to_string());
                }
            }
        }

        anyhow::bail!("No battery found")
    }

    fn read_battery_state(&self) -> Result<BatteryData> {
        let Some(ref battery_path) = self.battery_path else {
            return Ok(BatteryData {
                available: false,
                ..Default::default()
            });
        };

        let base_path = Path::new(battery_path);

        // Read capacity (percentage)
        let capacity_path = base_path.join("capacity");
        let capacity_str = fs::read_to_string(capacity_path)?;
        let percentage = capacity_str.trim().parse::<u32>()?;

        // Read status
        let status_path = base_path.join("status");
        let status_str = fs::read_to_string(status_path)?;
        let status = match status_str.trim() {
            "Charging" => BatteryStatus::Charging,
            "Discharging" => BatteryStatus::Discharging,
            "Not charging" => BatteryStatus::NotCharging,
            "Full" => BatteryStatus::Full,
            _ => BatteryStatus::Unknown,
        };

        // Try to calculate time remaining
        let time_remaining =
            self.calculate_time_remaining(base_path, &status, percentage as f64)?;

        Ok(BatteryData {
            percentage,
            status,
            available: true,
            time_remaining,
        })
    }

    fn calculate_time_remaining(
        &self,
        base_path: &Path,
        status: &BatteryStatus,
        percentage: f64,
    ) -> Result<i32> {
        // Try to read power_now and energy_now for more accurate calculation
        let power_now_path = base_path.join("power_now");
        let energy_now_path = base_path.join("energy_now");
        let energy_full_path = base_path.join("energy_full");

        if let (Ok(power_str), Ok(energy_str), Ok(energy_full_str)) = (
            fs::read_to_string(power_now_path),
            fs::read_to_string(energy_now_path),
            fs::read_to_string(energy_full_path),
        ) {
            if let (Ok(power_now), Ok(energy_now), Ok(energy_full)) = (
                power_str.trim().parse::<f64>(),
                energy_str.trim().parse::<f64>(),
                energy_full_str.trim().parse::<f64>(),
            ) {
                if power_now > 0.0 {
                    let time_hours = match status {
                        BatteryStatus::Charging => (energy_full - energy_now) / power_now,
                        BatteryStatus::Discharging => energy_now / power_now,
                        _ => return Ok(-1),
                    };
                    return Ok((time_hours * 60.0) as i32);
                }
            }
        }

        // Fallback: simple estimation based on percentage
        match status {
            BatteryStatus::Discharging => {
                // Rough estimate: assume 8 hours at 100%
                Ok((percentage / 100.0 * 8.0 * 60.0) as i32)
            }
            BatteryStatus::Charging => {
                // Rough estimate: assume 2 hours to charge from 0% to 100%
                Ok(((100.0 - percentage) / 100.0 * 2.0 * 60.0) as i32)
            }
            _ => Ok(-1),
        }
    }
}

impl Worker for BatteryWorker {
    type Init = ();
    type Input = BatteryWorkerMsg;
    type Output = BatteryData;

    fn init(_init: Self::Init, sender: ComponentSender<Self>) -> Self {
        // Detect battery on initialization
        let battery_path = Self::detect_battery().ok();

        if battery_path.is_none() {
            log::warn!("No battery detected, battery worker unavailable");
        }

        // Start periodic updates
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                sender_clone.input(BatteryWorkerMsg::RequestUpdate);
            }
        });

        // Request immediate initial update
        sender.input(BatteryWorkerMsg::RequestUpdate);

        Self {
            battery_path,
            last_data: BatteryData::default(),
        }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BatteryWorkerMsg::RequestUpdate => {
                match self.read_battery_state() {
                    Ok(data) => {
                        // Only send updates if data has changed significantly
                        let should_update = data.percentage != self.last_data.percentage
                            || data.status as u8 != self.last_data.status as u8
                            || data.available != self.last_data.available
                            || (data.time_remaining - self.last_data.time_remaining).abs() > 1;

                        if should_update {
                            self.last_data = data.clone();
                            let _ = sender.output(data);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to read battery state: {}", e);
                        let data = BatteryData {
                            available: false,
                            ..Default::default()
                        };
                        if data.available != self.last_data.available {
                            self.last_data = data.clone();
                            let _ = sender.output(data);
                        }
                    }
                }
            }
        }
    }
}

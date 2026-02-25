//! Stores a user's historical power usage. This data is used to make informed
//! predictions on future battery drain and estimated time remaining.

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::battery::{ChargingStatus, sysfs::SysfsReading};

/// Records 15-minute time slots.
const TIME_SLOTS_PER_HOUR: u32 = 4;

const TIME_SLOTS_PER_DAY: u32 = TIME_SLOTS_PER_HOUR * 24;
const TIME_SLOTS_PER_WEEK: u32 = TIME_SLOTS_PER_DAY * 7;
const MINUTES_PER_TIME_SLOT: u32 = 60 / TIME_SLOTS_PER_HOUR;

/// Determines how much new power readings affect historial averages.
const LEARNING_RATE: f64 = 0.1;

#[derive(Deserialize, Serialize)]
pub struct HistoricalPowerUsage {
    overall_discharging_average: f64,

    /// Average power usage per day, in watts.
    #[serde(with = "BigArray")]
    daily_averages: [f64; TIME_SLOTS_PER_DAY as usize],

    /// Average power usage per week, in watts.
    #[serde(with = "BigArray")]
    weekly_averages: [f64; TIME_SLOTS_PER_WEEK as usize],

    /// The value of the slope that determines how much power will be drawn
    /// based on percentage while charging.
    charging_coefficient: f64,

    /// The last time history was persisted to disk.
    #[serde(skip)]
    last_save: DateTime<Local>,

    /// All readings of the battery state.
    #[serde(skip)]
    all_readings: Vec<SysfsReading>,
}

impl Default for HistoricalPowerUsage {
    fn default() -> Self {
        Self {
            overall_discharging_average: Default::default(),
            daily_averages: [0.0; TIME_SLOTS_PER_DAY as usize],
            weekly_averages: [0.0; TIME_SLOTS_PER_WEEK as usize],
            charging_coefficient: Default::default(),
            last_save: Local::now(),
            all_readings: Default::default(),
        }
    }
}

impl HistoricalPowerUsage {
    /// Updates historical records based on a current reading of the device's
    /// power state.
    pub fn update(&mut self, reading: &SysfsReading) {
        let power_now = reading.power_watts();

        match reading.status {
            ChargingStatus::Charging => {
                if let Some(percentage_now) = reading.percentage() {
                    self.update_charging(power_now, percentage_now)
                }
            }
            ChargingStatus::Discharging => {
                self.update_discharging(power_now);
            }

            // do nothing otherwise
            _ => return,
        }

        // save state if 5 minutes or more have passed
        let now = Local::now();
        if now.signed_duration_since(self.last_save) >= TimeDelta::minutes(5)
            && let Err(e) = self.save_to_disk()
        {
            log::error!("{e}");
        }
        self.last_save = now;
    }

    fn update_discharging(&mut self, power_now: f64) {
        let now = Local::now();

        let (slot_of_day, slot_of_week) = get_slots(now);

        // get the actual buckets we'll edit
        let day_bucket = &mut self.daily_averages[slot_of_day as usize];
        let week_bucket = &mut self.weekly_averages[slot_of_week as usize];

        // set the overall average to power right now if it's never been set before;
        // otherwise, calculate the moving average
        if self.overall_discharging_average == 0.0 {
            self.overall_discharging_average = power_now;
        } else {
            self.overall_discharging_average = self.overall_discharging_average
                * (1.0 - LEARNING_RATE)
                + power_now * LEARNING_RATE;
        }

        // set the average in this day bucket to the overall average if it's never
        // been set before; otherwise, calculate the moving average
        if *day_bucket == 0.0 {
            *day_bucket = self.overall_discharging_average;
        } else {
            *day_bucket = *day_bucket * (1.0 - LEARNING_RATE) + power_now * LEARNING_RATE;
        }

        // set the average in this week bucket to the daily average if it's never
        // been set before; otherwise, calculate the moving average
        if *week_bucket == 0.0 {
            *week_bucket = *day_bucket;
        } else {
            *week_bucket = *week_bucket * (1.0 - LEARNING_RATE) + power_now * LEARNING_RATE;
        }
    }

    fn update_charging(&mut self, power_now: f64, percentage_now: f64) {
        if percentage_now >= 1.0 {
            // don't update if the battery is full
            return;
        }

        // if charging rate is linear (y = mx) based on percentage, then
        // power = coefficient * (1.0 - percentage).
        //
        // to determine the coefficient, then,
        // coefficient = power_now / (1.0 - percentage_now)
        let new_coefficient = power_now / (1.0 - percentage_now);

        // add it to the weighted average (or just set the value if the average
        // doesn't exist)
        self.charging_coefficient = if self.charging_coefficient == 0.0 {
            new_coefficient
        } else {
            self.charging_coefficient * (1.0 - LEARNING_RATE) + new_coefficient * LEARNING_RATE
        }
    }

    /// Returns the historical average power usage, in watts, recorded at the
    /// given time.
    pub fn predict_discharging_power_at(&self, when: DateTime<Local>) -> f64 {
        let (slot_of_day, slot_of_week) = get_slots(when);
        let day_bucket = self.daily_averages[slot_of_day as usize];
        let week_bucket = self.weekly_averages[slot_of_week as usize];

        let week_opt = if week_bucket == 0.0 {
            None
        } else {
            Some(week_bucket)
        };

        let day_opt = if day_bucket == 0.0 {
            None
        } else {
            Some(day_bucket)
        };

        week_opt
            .or(day_opt)
            .unwrap_or(self.overall_discharging_average)
    }

    /// Returns the amount of time until the battery will be either full or
    /// empty based on the given sysfs reading.
    pub fn predict_time_remaining(
        &self,
        reading: &SysfsReading,
        from_when: DateTime<Local>,
    ) -> Duration {
        match reading.status {
            ChargingStatus::Charging => {
                let Some(percentage_now) = reading.percentage() else {
                    return Duration::ZERO;
                };

                let wh_capacity = reading.capacity_wh();

                self.predict_time_to_full(percentage_now, wh_capacity)
            }
            ChargingStatus::Discharging => {
                let wh_remaining = reading.remaining_wh();
                self.predict_time_to_empty(from_when, wh_remaining)
            }
            _ => Duration::ZERO,
        }
    }

    /// Uses the stored charging coefficient and current percentage to determine
    /// when the battery will be full.
    ///
    /// The charging model assumes `power = coefficient * (1.0 - percentage)`,
    /// where power tapers off as the battery approaches full. Integrating the
    /// inverse of power over the remaining capacity gives:
    /// `time = (wh_capacity / coefficient) * ln(1 / (1 - percentage_now))`.
    fn predict_time_to_full(&self, percentage_now: f64, wh_capacity: f64) -> Duration {
        if percentage_now == 1.0 || self.charging_coefficient == 0.0 {
            return Duration::MAX;
        }
        let estimated_power = self.charging_coefficient * (1.0 - percentage_now);
        let wh_to_go = wh_capacity * (1.0 - percentage_now);
        let hours_to_full = wh_to_go / estimated_power;
        Duration::from_secs_f64(hours_to_full * 3600.0)
    }

    /// Uses integration over stored historical time-slot data to determine how
    /// long it will take for the battery to deplete entirely.
    ///
    /// Steps forward through 15-minute slots starting from `from_when`,
    /// subtracting the predicted power draw each slot until `wh_remaining`
    /// reaches zero.
    fn predict_time_to_empty(&self, from_when: DateTime<Local>, mut wh_remaining: f64) -> Duration {
        if wh_remaining == 0.0 {
            return Duration::ZERO;
        }

        let hours_per_slot = 1.0 / TIME_SLOTS_PER_HOUR as f64;
        let mut elapsed = Duration::ZERO;
        let mut current_time = from_when;

        // step forward slot by slot until energy runs out or a week has passed
        // (guard against infinite loops when history is zero everywhere)
        for _ in 0..TIME_SLOTS_PER_WEEK {
            let power_watts = self.predict_discharging_power_at(current_time);

            let energy_this_slot = power_watts * hours_per_slot;
            if energy_this_slot >= wh_remaining {
                // battery drains partway through this slot — interpolate the
                // fraction of the slot consumed
                let fraction = wh_remaining / energy_this_slot;
                let slot_minutes = MINUTES_PER_TIME_SLOT as f64 * fraction;
                elapsed += Duration::from_secs_f64(slot_minutes * 60.);
                break;
            }

            wh_remaining -= energy_this_slot;
            elapsed += Duration::from_mins(MINUTES_PER_TIME_SLOT.into());
            current_time += Duration::from_mins(MINUTES_PER_TIME_SLOT.into());
        }

        elapsed
    }

    /// Get the path to the history file.
    fn get_state_path() -> Result<PathBuf> {
        Ok(get_state_directory()?.join("power_history.json"))
    }

    pub fn read_from_disk() -> Result<Self> {
        let path = Self::get_state_path()?;
        let json = fs::read_to_string(&path).context("couldn't read power history")?;

        Ok(serde_json::from_str(&json)?)
    }

    fn save_to_disk(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        let path = Self::get_state_path()?;
        fs::write(&path, json).context("couldn't write predictor state")?;
        log::debug!("saved power history state to {:?}", path);
        Ok(())
    }
}

fn get_state_directory() -> Result<PathBuf> {
    let state_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .context("couldn't find state directory")?;
    let cadenza_state = state_dir.join("cadenza-shell");
    fs::create_dir_all(&cadenza_state).context("couldn't create state directory")?;
    Ok(cadenza_state)
}

/// Returns the day and week slot that correspond to the given date and time.
fn get_slots(when: DateTime<Local>) -> (u32, u32) {
    let slot_of_hour = when.minute() / MINUTES_PER_TIME_SLOT;
    let hour_of_day = when.hour();
    let slot_of_day = hour_of_day * TIME_SLOTS_PER_HOUR + slot_of_hour;

    let day_of_week = when.weekday().num_days_from_monday();
    let slot_of_week = day_of_week * TIME_SLOTS_PER_DAY + slot_of_day;

    (slot_of_day, slot_of_week)
}

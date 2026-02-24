//! Stores a user's historical power usage. This data is used to make informed
//! predictions on future battery drain and estimated time remaining.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use super::sysfs::ChargingStatus;
use crate::battery::sysfs::SysfsReading;

/// Records 15-minute time slots.
pub const TIME_SLOTS_PER_HOUR: u32 = 4;

pub const TIME_SLOTS_PER_DAY: u32 = TIME_SLOTS_PER_HOUR * 24;
pub const TIME_SLOTS_PER_WEEK: u32 = TIME_SLOTS_PER_DAY * 7;
pub const MINUTES_PER_TIME_SLOT: u32 = 60 / TIME_SLOTS_PER_HOUR;

/// Determines how much new power readings affect historial averages.
pub const LEARNING_RATE: f64 = 0.1;

#[derive(Deserialize, Serialize)]
pub struct HistoricalPowerUsage {
    overall_discharging_average: f64,

    #[serde(with = "BigArray")]
    daily_averages: [f64; TIME_SLOTS_PER_DAY as usize],

    #[serde(with = "BigArray")]
    weekly_averages: [f64; TIME_SLOTS_PER_WEEK as usize],

    /// The value of the slope that determines how much power will be drawn
    /// based on percentage while charging.
    charging_coefficient: f64,
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
            _ => (),
        }
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

    /// Get the path to the history file.
    fn get_state_path() -> Result<PathBuf> {
        let state_dir = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .context("couldn't find state directory")?;

        let cadenza_state = state_dir.join("cadenza-shell");
        fs::create_dir_all(&cadenza_state)?;

        Ok(cadenza_state.join("power_history.json"))
    }
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

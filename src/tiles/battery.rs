use std::time::Duration;

use chrono::{DateTime, Local};
use relm4::prelude::*;

use crate::{
    battery::{
        BATTERY_STATE, BATTERY_THRESHOLD_CRITICAL, BATTERY_THRESHOLD_LOW, BatteryState,
        ChargingStatus,
    },
    tiles::Attention,
    utils::icons::{BATTERY_CHARGING_ICON_NAMES, BATTERY_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileState, state_tile},
};

const TIME_REMAINING_LOW_THRESHOLD: Duration = Duration::from_mins(30);
const TIME_REMAINING_CRITICAL_THRESHOLD: Duration = Duration::from_mins(15);

/// The battery level and how long it lasts, hidden without a battery.
pub fn battery_tile() -> Controller<Tile> {
    state_tile(&BATTERY_STATE, view).detach()
}

fn view(state: &Option<BatteryState>) -> Option<TileState> {
    let state = state.as_ref()?;
    Some(TileState {
        icon: Some(icon(state).to_string()),
        primary: Some(level_text(state)),
        secondary: Some(format_remaining(
            state.status,
            state.discharging_time_remaining,
            Local::now(),
        )),
        attention: attention(state),
        tooltip: None,
    })
}

fn icon(state: &BatteryState) -> &'static str {
    let icons = if state.status.is_charging() {
        BATTERY_CHARGING_ICON_NAMES
    } else {
        BATTERY_ICON_NAMES
    };
    percentage_to_icon_from_list(state.percentage.into(), icons)
}

fn level_text(state: &BatteryState) -> String {
    // sometimes, systems report "not charging" instead of "full" or
    // "charging" when the battery is full while plugged in. so we'll just
    // report "full" as long as the battery isn't *actively* discharging.
    if state.status != ChargingStatus::Discharging && state.percentage >= 0.995 {
        "Full".to_string()
    } else {
        format!("{}%", (state.percentage * 100.0) as u32)
    }
}

/// Warns when the battery runs low, but only while discharging: a
/// plugged-in battery that isn't charging (or reports an unknown status)
/// isn't running out.
fn attention(state: &BatteryState) -> Attention {
    if state.status != ChargingStatus::Discharging {
        return Attention::Normal;
    }

    let remaining = state.discharging_time_remaining;
    if state.percentage <= BATTERY_THRESHOLD_CRITICAL
        || remaining < TIME_REMAINING_CRITICAL_THRESHOLD
    {
        Attention::Alarm
    } else if state.percentage <= BATTERY_THRESHOLD_LOW || remaining < TIME_REMAINING_LOW_THRESHOLD
    {
        Attention::Warning
    } else {
        Attention::Normal
    }
}

/// Describes how long the battery lasts, e.g. "25 min left" or "Until 4:30
/// pm".
///
/// While charging, this is how long the battery would last if unplugged now,
/// phrased as "Good for ...".
fn format_remaining(status: ChargingStatus, remaining: Duration, now: DateTime<Local>) -> String {
    if status == ChargingStatus::NotCharging {
        return "Not charging".to_string();
    }

    let charging = status.is_charging();
    if remaining < TIME_REMAINING_LOW_THRESHOLD {
        let minutes = remaining.as_secs() / 60;
        if charging {
            format!("Good for {minutes} min")
        } else {
            format!("{minutes} min left")
        }
    } else if remaining < Duration::from_hours(24) {
        let until = (now + remaining).format("%-I:%M %P");
        if charging {
            format!("Good for {until}")
        } else {
            format!("Until {until}")
        }
    } else if remaining < Duration::from_hours(48) {
        if charging {
            "Good for tomorrow"
        } else {
            "Until tomorrow"
        }
        .to_string()
    } else {
        if charging {
            "Good for a while"
        } else {
            "Until someday"
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn noon() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn short_times_are_in_minutes() {
        assert_eq!(
            format_remaining(ChargingStatus::Discharging, Duration::from_mins(25), noon()),
            "25 min left"
        );
        assert_eq!(
            format_remaining(ChargingStatus::Charging, Duration::from_mins(25), noon()),
            "Good for 25 min"
        );
    }

    #[test]
    fn same_day_times_show_the_end_time() {
        assert_eq!(
            format_remaining(ChargingStatus::Discharging, Duration::from_hours(3), noon()),
            "Until 3:00 pm"
        );
    }

    #[test]
    fn long_times_are_vague() {
        assert_eq!(
            format_remaining(
                ChargingStatus::Discharging,
                Duration::from_hours(30),
                noon()
            ),
            "Until tomorrow"
        );
        assert_eq!(
            format_remaining(ChargingStatus::Full, Duration::from_hours(80), noon()),
            "Good for a while"
        );
    }

    #[test]
    fn not_charging_says_so() {
        assert_eq!(
            format_remaining(ChargingStatus::NotCharging, Duration::from_hours(3), noon()),
            "Not charging"
        );
    }
}

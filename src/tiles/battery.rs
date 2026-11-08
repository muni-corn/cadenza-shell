use std::time::Duration;

use chrono::{DateTime, Local};
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    battery::{
        BATTERY_STATE, BATTERY_THRESHOLD_CRITICAL, BATTERY_THRESHOLD_LOW, BatteryState,
        ChargingStatus,
    },
    tiles::Attention,
    utils::icons::{BATTERY_CHARGING_ICON_NAMES, BATTERY_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileInit, TileMsg},
};

const TIME_REMAINING_LOW_THRESHOLD: Duration = Duration::from_mins(30);
const TIME_REMAINING_CRITICAL_THRESHOLD: Duration = Duration::from_mins(15);

#[derive(Debug, Default)]
pub struct BatteryTile {
    available: bool,

    current_percentage: f32,
    status: ChargingStatus,
    discharging_time_remaining: Duration,
}

#[derive(Debug)]
pub enum BatteryMsg {
    StateUpdate(Option<BatteryState>),
}

#[derive(Debug)]
pub struct BatteryWidgets {
    root: <BatteryTile as Component>::Root,
    tile: Controller<Tile>,
}

impl SimpleComponent for BatteryTile {
    type Init = ();
    type Input = BatteryMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = BatteryWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // subscribe to the global battery state
        BATTERY_STATE.subscribe(sender.input_sender(), |new_state| {
            BatteryMsg::StateUpdate(*new_state)
        });

        // initialize model
        let model = BATTERY_STATE.read().map_or_default(|s| BatteryTile {
            available: true,

            current_percentage: s.percentage,
            status: s.status,
            discharging_time_remaining: s.discharging_time_remaining,
        });

        // hide the entire tile if battery isn't available
        root.set_visible(model.available);

        // setup tile
        let tile = Tile::builder()
            .launch(TileInit {
                icon_name: Some(model.get_icon().to_string()),
                primary: Some(model.get_text()),
                secondary: Some(model.get_readable_time()),
                attention: model.get_attention(),
                tooltip: None,
            })
            .detach();
        root.append(tile.widget());

        ComponentParts {
            model,
            widgets: BatteryWidgets { root, tile },
        }
    }

    fn update(&mut self, BatteryMsg::StateUpdate(o): Self::Input, _sender: ComponentSender<Self>) {
        if let Some(BatteryState {
            percentage,
            status,
            discharging_time_remaining,
        }) = o
        {
            self.current_percentage = percentage;
            self.status = status;
            self.discharging_time_remaining = discharging_time_remaining;
            self.available = true;
        } else {
            self.available = false;
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(self.available);

        if self.available {
            // update the tile with new data
            widgets
                .tile
                .emit(TileMsg::SetIcon(Some(self.get_icon().to_string())));
            widgets
                .tile
                .emit(TileMsg::SetPrimary(Some(self.get_text())));
            widgets
                .tile
                .emit(TileMsg::SetSecondary(Some(self.get_readable_time())));

            // update attention state
            let attention = self.get_attention();

            // update visibility and attention
            widgets.tile.emit(TileMsg::SetAttention(attention));
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::default()
    }
}

impl BatteryTile {
    fn get_icon(&self) -> &str {
        if self.status.is_charging() {
            percentage_to_icon_from_list(
                self.current_percentage.into(),
                BATTERY_CHARGING_ICON_NAMES,
            )
        } else {
            percentage_to_icon_from_list(self.current_percentage.into(), BATTERY_ICON_NAMES)
        }
    }

    fn get_text(&self) -> String {
        // sometimes, systems report "not charging" instead of "full" or
        // "charging" when the battery is full while plugged in. so we'll just
        // report "full" as long as the battery isn't *actively* discharging.
        if self.status != ChargingStatus::Discharging && self.current_percentage >= 0.995 {
            "Full".to_string()
        } else {
            format!("{}%", (self.current_percentage * 100.0) as u32)
        }
    }

    // a plugged-in battery that isn't charging (or reports an unknown
    // status) isn't running out, so only warn while discharging
    fn is_low(&self) -> bool {
        self.status == ChargingStatus::Discharging
            && (self.current_percentage <= BATTERY_THRESHOLD_LOW
                || self.discharging_time_remaining < TIME_REMAINING_LOW_THRESHOLD)
    }

    fn is_critical(&self) -> bool {
        self.status == ChargingStatus::Discharging
            && (self.current_percentage <= BATTERY_THRESHOLD_CRITICAL
                || self.discharging_time_remaining < TIME_REMAINING_CRITICAL_THRESHOLD)
    }

    fn get_readable_time(&self) -> String {
        format_remaining(self.status, self.discharging_time_remaining, Local::now())
    }

    fn get_attention(&self) -> Attention {
        if self.is_critical() {
            Attention::Alarm
        } else if self.is_low() {
            Attention::Warning
        } else {
            Attention::Normal
        }
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

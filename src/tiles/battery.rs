use std::time::Duration;

use gtk4::prelude::*;
use relm4::{Worker, prelude::*};

use crate::{
    icon_names::{BATTERY_CHARGE_REGULAR, BATTERY_CHECKMARK_REGULAR},
    services::battery::{BATTERY_STATE, BatteryService, BatteryState},
    tiles::Attention,
    utils::icons::{BATTERY_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileMsg},
};

pub struct BatteryTile {
    available: bool,

    current_percentage: f32,
    charging: bool,
    time_remaining: Duration,
}

#[derive(Debug)]
pub enum BatteryMsg {
    StateUpdate(BatteryState),
}

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
        let model = BatteryTile {
            available: false,

            current_percentage: 0.,
            charging: false,
            time_remaining: Duration::ZERO,
        };

        let tile = Tile::builder().launch(Default::default()).detach();
        root.append(tile.widget());

        BATTERY_STATE.subscribe(sender.input_sender(), |new_state| {
            BatteryMsg::StateUpdate(*new_state)
        });

        ComponentParts {
            model,
            widgets: BatteryWidgets { root, tile },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BatteryMsg::StateUpdate(m) => match m {
                BatteryState::Available {
                    percentage,
                    charging,
                    time_remaining,
                } => {
                    self.current_percentage = percentage;
                    self.charging = charging;
                    self.time_remaining = time_remaining;
                    self.available = true;
                }
                BatteryState::Unavailable => {
                    self.available = false;
                }
            },
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
            let attention = if self.is_critical() {
                Attention::Alarm
            } else if self.is_low() {
                Attention::Warning
            } else {
                Attention::Normal
            };

            // update visibility and attention
            widgets.tile.emit(TileMsg::SetAttention(attention));
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

impl BatteryTile {
    fn get_icon(&self) -> &str {
        if self.charging {
            if self.current_percentage > 0.99 {
                BATTERY_CHECKMARK_REGULAR
            } else {
                BATTERY_CHARGE_REGULAR
            }
        } else {
            percentage_to_icon_from_list(self.current_percentage.into(), BATTERY_ICON_NAMES)
        }
    }

    fn get_text(&self) -> String {
        if self.charging && self.current_percentage > 0.99 {
            "Full".to_string()
        } else {
            format!("{}%", (self.current_percentage * 100.0) as u32)
        }
    }

    fn is_low(&self) -> bool {
        let time_remaining_secs = self.time_remaining.as_secs();

        (self.current_percentage <= 0.2 || time_remaining_secs <= 3600) && !self.charging
    }

    fn is_critical(&self) -> bool {
        let time_remaining_secs = self.time_remaining.as_secs();

        (self.current_percentage <= 0.1 || time_remaining_secs <= 1800) && !self.charging
    }

    fn get_readable_time(&self) -> String {
        use chrono::Local;

        if self.charging && self.current_percentage > 0.99 {
            "Plugged in".to_string()
        } else {
            let time_remaining = self.time_remaining.as_secs();
            if time_remaining < 30 * 60 {
                format!("{} min left", time_remaining / 60)
            } else {
                // calculate actual completion time
                let now = Local::now();
                let completion_time = now + chrono::Duration::seconds(time_remaining as i64);

                // format as "h:mm am/pm"
                let formatted = completion_time.format("%-I:%M %P").to_string();

                if self.charging {
                    format!("Full at {}", formatted)
                } else {
                    format!("Until {}", formatted)
                }
            }
        }
    }
}

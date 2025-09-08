use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::{BATTERY_LEVEL_0_CHARGING, BATTERY_LEVEL_100_CHARGED, BATTERY_MISSING},
    services::battery::BatteryService,
    utils::icons::{BATTERY_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Attention, Tile, TileInit, TileMsg},
};

#[derive(Debug)]
pub struct BatteryTile {
    percentage: f64,
    charging: bool,
    available: bool,
    is_low: bool,
    is_critical: bool,
    service: BatteryService,
    tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum BatteryMsg {
    ServiceUpdate {
        percentage: f64,
        charging: bool,
        available: bool,
        is_low: bool,
        is_critical: bool,
    },
}

pub struct BatteryWidgets {
    root: <BatteryTile as Component>::Root,
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
        let service = BatteryService::new();

        // initialize the tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        let model = BatteryTile {
            percentage: 0.0,
            charging: false,
            available: false,
            is_low: false,
            is_critical: false,
            service: service.clone(),
            tile,
        };

        // connect to battery service property changes
        let update_closure = glib::clone!(
            #[strong]
            sender,
            move |service: &BatteryService| {
                sender.input(BatteryMsg::ServiceUpdate {
                    percentage: service.percentage(),
                    charging: service.charging(),
                    available: service.available(),
                    is_low: service.is_low(),
                    is_critical: service.is_critical(),
                });
            }
        );

        service.connect_percentage_notify(update_closure.clone());
        service.connect_charging_notify(update_closure.clone());
        service.connect_available_notify(update_closure.clone());
        service.connect_time_remaining_notify(update_closure.clone());

        // initial state update
        if service.available() {
            sender.input(BatteryMsg::ServiceUpdate {
                percentage: service.percentage(),
                charging: service.charging(),
                available: service.available(),
                is_low: service.is_low(),
                is_critical: service.is_critical(),
            });
        }

        ComponentParts {
            model,
            widgets: BatteryWidgets { root },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BatteryMsg::ServiceUpdate {
                percentage,
                charging,
                available,
                is_low,
                is_critical,
            } => {
                self.percentage = percentage;
                self.charging = charging;
                self.available = available;
                self.is_low = is_low;
                self.is_critical = is_critical;

                // update attention state
                let attention = if is_critical {
                    Attention::Alarm
                } else if is_low {
                    Attention::Warning
                } else {
                    Attention::Normal
                };

                // update the tile with new data
                self.tile.emit(TileMsg::UpdateData {
                    icon: Some(self.get_icon().to_string()),
                    primary: Some(self.get_text()),
                    secondary: self.get_secondary_text(),
                });

                // update visibility and attention
                self.tile.emit(TileMsg::SetVisible(available));
                self.tile.emit(TileMsg::SetAttention(attention));
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(self.available);
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

impl BatteryTile {
    fn get_icon(&self) -> &str {
        if !self.available {
            BATTERY_MISSING
        } else if self.charging {
            if self.service.percentage() > 0.99 {
                BATTERY_LEVEL_100_CHARGED
            } else {
                BATTERY_LEVEL_0_CHARGING
            }
        } else {
            percentage_to_icon_from_list(self.percentage, BATTERY_ICON_NAMES)
        }
    }

    fn get_text(&self) -> String {
        if !self.available {
            return "N/A".to_string();
        }

        if self.service.percentage() > 0.99 {
            "Full".to_string()
        } else {
            format!("{}%", (self.percentage * 100.0) as u32)
        }
    }

    fn get_secondary_text(&self) -> Option<String> {
        if !self.available {
            return None;
        }

        let readable_time = self.service.get_readable_time();
        if !readable_time.is_empty() {
            Some(readable_time)
        } else {
            None
        }
    }
}

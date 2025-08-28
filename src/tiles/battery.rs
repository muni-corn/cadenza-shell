use gtk4::prelude::*;
use relm4::prelude::*;

use crate::icon_names::{BATTERY_LEVEL_0_CHARGING, BATTERY_LEVEL_100_CHARGED, BATTERY_MISSING};
use crate::services::battery::{BatteryService, BatteryStatus};
use crate::utils::icons::{BATTERY_ICON_NAMES, percentage_to_icon_from_list};
use crate::widgets::tile::{Attention, TileOutput};

#[derive(Debug)]
pub struct BatteryTile {
    percentage: f64,
    charging: bool,
    available: bool,
    is_low: bool,
    is_critical: bool,
    service: BatteryService,
    attention: Attention,
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
    Click,
    UpdateDisplay,
}

pub struct BatteryWidgets {
    root: <BatteryTile as Component>::Root,
}

#[relm4::component(pub)]
impl SimpleComponent for BatteryTile {
    type Init = ();
    type Input = BatteryMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "battery",
            #[watch]
            set_visible: model.available,

            connect_clicked[sender] => move |_| {
                sender.input(BatteryMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                #[name = "battery_icon"]
                gtk::Image {
                    #[watch]
                    set_icon_name: Some(model.get_icon()),
                    add_css_class: "tile-icon",
                },

                #[name = "battery_label"]
                gtk::Label {
                    #[watch]
                    set_text: &model.get_text(),
                    add_css_class: "tile-text",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = BatteryService::new();

        let model = BatteryTile {
            percentage: 0.0,
            charging: false,
            available: false,
            is_low: false,
            is_critical: false,
            service: service.clone(),
            attention: Attention::Dim,
        };

        let widgets = view_output!();

        // Connect to battery service property changes
        service.connect_percentage_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                sender.input(BatteryMsg::ServiceUpdate {
                    percentage: service.percentage(),
                    charging: service.charging(),
                    available: service.available(),
                    is_low: service.is_low(),
                    is_critical: service.is_critical(),
                });
            }
        ));

        service.connect_charging_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                sender.input(BatteryMsg::ServiceUpdate {
                    percentage: service.percentage(),
                    charging: service.charging(),
                    available: service.available(),
                    is_low: service.is_low(),
                    is_critical: service.is_critical(),
                });
            }
        ));

        service.connect_available_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                sender.input(BatteryMsg::ServiceUpdate {
                    percentage: service.percentage(),
                    charging: service.charging(),
                    available: service.available(),
                    is_low: service.is_low(),
                    is_critical: service.is_critical(),
                });
            }
        ));

        // Initial state update
        if service.available() {
            sender.input(BatteryMsg::ServiceUpdate {
                percentage: service.percentage(),
                charging: service.charging(),
                available: service.available(),
                is_low: service.is_low(),
                is_critical: service.is_critical(),
            });
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
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

                // Update attention state
                self.attention = if is_critical {
                    Attention::Alarm
                } else if is_low {
                    Attention::Warning
                } else if charging {
                    Attention::Normal
                } else {
                    Attention::Dim
                };

                // Update CSS classes based on state
                self.update_css_classes();
            }
            BatteryMsg::Click => {
                sender.output(TileOutput::Clicked).ok();
            }
            BatteryMsg::UpdateDisplay => {
                // Trigger view update
            }
        }
    }
}

impl BatteryTile {
    fn get_icon(&self) -> &str {
        if !self.available {
            BATTERY_MISSING
        } else if self.charging {
            if self.service.status() == BatteryStatus::Full {
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

        format!("{}%", (self.percentage * 100.0) as u32)
    }

    fn get_secondary_text(&self) -> Option<String> {
        if !self.available {
            return None;
        }

        let status = if self.charging {
            "Charging"
        } else if self.is_critical {
            "Critical"
        } else if self.is_low {
            "Low"
        } else {
            return None;
        };

        Some(status.to_string())
    }

    fn update_css_classes(&self) {
        // This would be called to update CSS classes dynamically
        // In a real implementation, we'd need access to the widget
        // For now, we'll handle this in the view macro
    }
}

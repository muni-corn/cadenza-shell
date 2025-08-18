use gtk4::prelude::*;
use relm4::prelude::*;

use crate::services::battery::BatteryService;
use crate::utils::icons::{percentage_to_icon_from_list, BATTERY_CHARGING_ICONS, BATTERY_ICONS};
use crate::widgets::tile::Attention;

#[derive(Debug)]
pub struct BatteryWidget {
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
}

#[derive(Debug)]
pub enum BatteryOutput {
    Clicked,
}

#[relm4::component]
impl SimpleComponent for BatteryWidget {
    type Init = ();
    type Input = BatteryMsg;
    type Output = BatteryOutput;

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
                    set_icon_name: Some(&model.get_icon()),
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
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = BatteryService::new();
        
        let model = BatteryWidget {
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
        let sender_clone = sender.clone();
        service.connect_notify_local(Some("percentage"), move |service, _| {
            sender_clone.input(BatteryMsg::ServiceUpdate {
                percentage: service.percentage(),
                charging: service.charging(),
                available: service.available(),
                is_low: service.is_low(),
                is_critical: service.is_critical(),
            });
        });

        let sender_clone = sender.clone();
        service.connect_notify_local(Some("charging"), move |service, _| {
            sender_clone.input(BatteryMsg::ServiceUpdate {
                percentage: service.percentage(),
                charging: service.charging(),
                available: service.available(),
                is_low: service.is_low(),
                is_critical: service.is_critical(),
            });
        });

        let sender_clone = sender.clone();
        service.connect_notify_local(Some("available"), move |service, _| {
            sender_clone.input(BatteryMsg::ServiceUpdate {
                percentage: service.percentage(),
                charging: service.charging(),
                available: service.available(),
                is_low: service.is_low(),
                is_critical: service.is_critical(),
            });
        });

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
            }
            BatteryMsg::Click => {
                log::debug!("Battery tile clicked");
                let _ = sender.output(BatteryOutput::Clicked);
            }
        }
    }
}

impl BatteryWidget {
    pub fn new() -> Self {
        // This is a placeholder - the actual initialization happens in init()
        Self {
            percentage: 0.0,
            charging: false,
            available: false,
            is_low: false,
            is_critical: false,
            service: BatteryService::new(),
            attention: Attention::Dim,
        }
    }

    pub fn widget(&self) -> &gtk4::Widget {
        // This method is kept for compatibility but won't be used in Relm4
        unimplemented!("Use Relm4 component instead")
    }

    pub fn service(&self) -> &BatteryService {
        &self.service
    }

    fn get_icon(&self) -> String {
        if !self.available {
            return "battery-missing-symbolic".to_string();
        }

        let icons = if self.charging {
            BATTERY_CHARGING_ICONS
        } else {
            BATTERY_ICONS
        };

        percentage_to_icon_from_list(self.percentage, icons).to_string()
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
}
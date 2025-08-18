use gtk4::prelude::*;
use relm4::prelude::*;
use relm4::WorkerController;

use crate::simple_messages::TileOutput;
use crate::services::battery_worker::{BatteryWorker, BatteryData, BatteryStatus};

#[derive(Debug)]
struct BatteryWidget {
    percentage: u32,
    charging: bool,
    battery_worker: WorkerController<BatteryWorker>,
}

#[derive(Debug)]
pub enum BatteryMsg {
    Click,
    UpdateData(BatteryData),
}

#[relm4::component]
impl SimpleComponent for BatteryWidget {
    type Init = ();
    type Input = BatteryMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "battery",
            
            connect_clicked[sender] => move |_| {
                sender.input(BatteryMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(&model.get_battery_icon()),
                    add_css_class: "tile-icon",
                },

                gtk::Label {
                    #[watch]
                    set_text: &format!("{}%", model.percentage),
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
        // Initialize the battery worker
        let battery_worker = BatteryWorker::builder()
            .detach_worker(())
            .forward(sender.input_sender(), |data| BatteryMsg::UpdateData(data));

        let model = BatteryWidget {
            percentage: 50,
            charging: false,
            battery_worker,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BatteryMsg::Click => {
                log::debug!("Battery tile clicked");
                let _ = sender.output(TileOutput::Clicked("battery".to_string()));
            }
            BatteryMsg::UpdateData(data) => {
                if data.available {
                    self.percentage = data.percentage;
                    self.charging = matches!(data.status, BatteryStatus::Charging);
                }
            }
        }
    }
}

impl BatteryWidget {
    fn get_battery_icon(&self) -> String {
        use crate::utils::icons::{BATTERY_ICONS, BATTERY_CHARGING_ICONS};
        
        let icons = if self.charging {
            BATTERY_CHARGING_ICONS
        } else {
            BATTERY_ICONS
        };
        
        let index = ((self.percentage as f64 / 100.0) * (icons.len() - 1) as f64).round() as usize;
        icons.get(index).unwrap_or(&"battery-symbolic").to_string()
    }
}

pub fn create_battery_widget() -> gtk4::Widget {
    let controller = BatteryWidget::builder()
        .launch(())
        .detach();
    controller.widget().clone().into()
}


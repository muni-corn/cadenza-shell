use gtk4::prelude::*;
use relm4::prelude::*;

use crate::simple_messages::TileOutput;

#[derive(Debug)]
struct BatteryWidget {
    percentage: u32,
    charging: bool,
}

#[derive(Debug)]
pub enum BatteryMsg {
    Click,
    UpdateData(u32, bool), // percentage, charging
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
                    set_icon_name: Some("battery-symbolic"),
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
        let model = BatteryWidget {
            percentage: 50,
            charging: false,
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
            BatteryMsg::UpdateData(percentage, charging) => {
                self.percentage = percentage;
                self.charging = charging;
            }
        }
    }
}

pub fn create_battery_widget() -> gtk4::Widget {
    let controller = BatteryWidget::builder()
        .launch(())
        .detach();
    controller.widget().clone().into()
}


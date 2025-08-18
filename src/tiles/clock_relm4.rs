use chrono::{DateTime, Local};
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;
use crate::services::clock::ClockService;

#[derive(Debug)]
pub struct ClockTile {
    time: DateTime<Local>,
    service: ClockService,
}

#[derive(Debug)]
pub enum ClockMsg {
    Click,
    TimeUpdate(DateTime<Local>),
}

#[relm4::component(pub)]
impl SimpleComponent for ClockTile {
    type Init = ();
    type Input = ClockMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "clock",

            connect_clicked[sender] => move |_| {
                sender.input(ClockMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_halign: gtk::Align::Center,

                gtk::Label {
                    #[watch]
                    set_text: &model.get_time_text(),
                    add_css_class: "clock-time",
                },

                gtk::Label {
                    #[watch]
                    set_text: &model.get_date_text(),
                    add_css_class: "clock-date",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = ClockService::new();
        let current_time = Local::now();

        let model = ClockTile {
            time: current_time,
            service: service.clone(),
        };

        let widgets = view_output!();

        // Connect to clock service updates
        service.connect_time_notify(glib::clone!(
            #[weak]
            sender,
            move |service| {
                sender.input(ClockMsg::TimeUpdate(service.current_time()));
            }
        ));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ClockMsg::Click => {
                sender.output(TileOutput::Clicked("clock".to_string())).ok();
            }
            ClockMsg::TimeUpdate(time) => {
                self.time = time;
            }
        }
    }
}

impl ClockTile {
    fn get_time_text(&self) -> String {
        self.time.format("%H:%M").to_string()
    }

    fn get_date_text(&self) -> String {
        self.time.format("%m/%d").to_string()
    }
}

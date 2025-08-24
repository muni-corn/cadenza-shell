use std::time::Duration;

use chrono::{DateTime, Local};
use gtk4::prelude::*;
use relm4::prelude::*;
use tokio::time::interval;

use crate::icon_names;
use crate::messages::TileOutput;

#[derive(Debug)]
pub struct ClockTile {
    time: DateTime<Local>,
}

#[derive(Debug)]
pub enum ClockMsg {
    Click,
    UpdateTime,
}

#[derive(Debug)]
pub struct ClockWidgets {
    time_label: gtk::Label,
    date_label: gtk::Label,
}

impl SimpleComponent for ClockTile {
    type Init = ();
    type Input = ClockMsg;
    type Output = TileOutput;
    type Root = gtk::Button;
    type Widgets = ClockWidgets;

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current_time = Local::now();

        let model = ClockTile { time: current_time };

        let hbox = &gtk::Box::new(gtk4::Orientation::Horizontal, 16);

        let icon = gtk::Image::builder()
            .icon_name(icon_names::CLOCK_ALT)
            .build();

        let time_label = gtk::Label::builder()
            .css_classes(["clock-time"])
            .label(model.get_time_text())
            .build();

        let date_label = gtk::Label::builder()
            .css_classes(["clock-date"])
            .label(model.get_date_text())
            .build();

        hbox.append(&icon);
        hbox.append(&time_label);
        hbox.append(&date_label);
        root.set_child(Some(hbox));

        let sender_clone = sender.clone();
        root.connect_clicked(move |_| {
            sender_clone.input(ClockMsg::Click);
        });

        // Update from service every second via property notify
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                sender_clone.input(ClockMsg::UpdateTime);
            }
        });

        let widgets = ClockWidgets {
            date_label,
            time_label,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ClockMsg::Click => {
                sender.output(TileOutput::Clicked("clock".to_string())).ok();
            }
            ClockMsg::UpdateTime => self.time = Local::now(),
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.time_label.set_label(&self.get_time_text());
        widgets.date_label.set_label(&self.get_date_text());
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().css_classes(["tile"]).build()
    }
}

impl ClockTile {
    fn get_time_text(&self) -> String {
        self.time.format("%-I:%M %P").to_string()
    }

    fn get_date_text(&self) -> String {
        self.time.format("%a, %b %-d").to_string()
    }
}

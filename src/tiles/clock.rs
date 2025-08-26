use std::time::Duration;

use chrono::{DateTime, Local};
use gtk4::prelude::*;
use relm4::prelude::*;
use tokio::time::interval;

use crate::icon_names;
use crate::widgets::tile::{Attention, Tile, TileInit, TileMsg, TileOutput};

#[derive(Debug)]
pub struct ClockTile {
    time: DateTime<Local>,
    tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum ClockMsg {
    Clicked,
    Nothing,
}

impl SimpleComponent for ClockTile {
    type Init = ();
    type Input = ClockMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = ();

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current_time = Local::now();

        // initialize the Tile component
        let tile = Tile::builder()
            .launch(TileInit {
                name: "clock".to_string(),
                icon_name: Some(icon_names::CLOCK_ALT.to_string()),
                primary: Some(format_time(&current_time)),
                secondary: Some(format_date(&current_time)),
                visible: true,
                attention: Attention::Normal,
                extra_classes: vec!["clock".to_string()],
            })
            .forward(sender.input_sender(), |output| match output {
                TileOutput::Clicked => ClockMsg::Clicked,
                _ => ClockMsg::Nothing,
            });

        root.append(tile.widget());

        // start time updates
        let tile_sender_clone = tile.sender().clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let now = Local::now();
                tile_sender_clone.emit(TileMsg::UpdateData {
                    icon: Some(icon_names::CLOCK_ALT.to_string()),
                    primary: Some(format_time(&now)),
                    secondary: Some(format_date(&now)),
                });
            }
        });

        let model = ClockTile {
            time: current_time,
            tile,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {
        // TODO: handle tile clicks
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

fn format_time(time: &DateTime<Local>) -> String {
    time.format("%-I:%M %P").to_string()
}

fn format_date(time: &DateTime<Local>) -> String {
    time.format("%a, %b %-d").to_string()
}

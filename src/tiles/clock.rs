use std::time::Duration;

use chrono::{DateTime, Local};
use gtk4::prelude::*;
use relm4::prelude::*;
use tokio::time::interval;

use crate::{
    icon_names,
    widgets::tile::{Tile, TileInit, TileMsg},
};

#[derive(Debug)]
pub struct ClockTile {
    _tile: Controller<Tile>,
}

impl SimpleComponent for ClockTile {
    type Init = ();
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init(
        _: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current_time = Local::now();

        // initialize the Tile component
        let tile = Tile::builder()
            .launch(TileInit {
                icon_name: Some(icon_names::CLOCK_REGULAR.to_string()),
                primary: Some(format_time(&current_time)),
                secondary: Some(format_date(&current_time)),
                ..Default::default()
            })
            .detach();

        root.append(tile.widget());

        // start time updates
        let tile_sender_clone = tile.sender().clone();
        relm4::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let now = Local::now();
                tile_sender_clone.emit(TileMsg::SetIcon(Some(
                    icon_names::CLOCK_REGULAR.to_string(),
                )));
                tile_sender_clone.emit(TileMsg::SetPrimary(Some(format_time(&now))));
                tile_sender_clone.emit(TileMsg::SetSecondary(Some(format_date(&now))));
            }
        });

        let model = ClockTile { _tile: tile };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

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

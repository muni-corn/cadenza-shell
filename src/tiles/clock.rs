use std::time::Duration;

use chrono::{DateTime, Local, Timelike};
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names,
    widgets::tile::{Tile, TileInit, TileMsg},
};

#[derive(Debug)]
pub struct ClockTile {
    tile: Controller<Tile>,
}

impl Component for ClockTile {
    type CommandOutput = DateTime<Local>;
    type Init = ();
    type Input = ();
    type Output = ();
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
                icon_name: Some(icon_names::CLOCK.to_string()),
                primary: Some(format_time(&current_time)),
                secondary: Some(format_date(&current_time)),
                ..Default::default()
            })
            .detach();

        root.append(tile.widget());

        // the labels only show minutes, so waking at each minute boundary is
        // enough; the command is cancelled when the tile shuts down
        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    loop {
                        tokio::time::sleep(until_next_minute(&Local::now())).await;
                        if out.send(Local::now()).is_err() {
                            break;
                        }
                    }
                })
                .drop_on_shutdown()
        });

        let model = ClockTile { tile };

        ComponentParts { model, widgets: () }
    }

    fn update_cmd(
        &mut self,
        now: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        self.tile.emit(TileMsg::SetPrimary(Some(format_time(&now))));
        self.tile
            .emit(TileMsg::SetSecondary(Some(format_date(&now))));
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

/// Returns how long to wait until just after the next minute starts.
fn until_next_minute(now: &DateTime<Local>) -> Duration {
    let into_minute = Duration::new(u64::from(now.second()), now.nanosecond() % 1_000_000_000);
    // a small margin makes sure the wakeup lands inside the new minute
    Duration::from_secs(60).saturating_sub(into_minute) + Duration::from_millis(50)
}

fn format_time(time: &DateTime<Local>) -> String {
    time.format("%-I:%M %P").to_string()
}

fn format_date(time: &DateTime<Local>) -> String {
    time.format("%a, %b %-d").to_string()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn waits_until_just_after_the_next_minute() {
        let now = Local.with_ymd_and_hms(2026, 1, 1, 12, 30, 45).unwrap();
        assert_eq!(until_next_minute(&now), Duration::from_millis(15_050));
    }

    #[test]
    fn waits_a_full_minute_at_the_start_of_a_minute() {
        let now = Local.with_ymd_and_hms(2026, 1, 1, 12, 30, 0).unwrap();
        assert_eq!(until_next_minute(&now), Duration::from_millis(60_050));
    }
}

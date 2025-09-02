use gtk4::prelude::BoxExt;
use relm4::prelude::*;

use crate::{
    settings::BarConfig,
    tiles::{clock::ClockTile, weather::WeatherTile},
};

#[derive(Debug)]
pub struct CenterGroup;

#[derive(Debug)]
pub struct CenterWidgets {
    _clock: Controller<ClockTile>,
    _weather: Controller<WeatherTile>,
    // _media: Controller<MprisTile>,
}

impl SimpleComponent for CenterGroup {
    type Init = BarConfig;
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = CenterWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }

    fn init(
        bar_config: Self::Init,
        root: Self::Root,
        _sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let clock = ClockTile::builder().launch(()).detach();
        let weather = WeatherTile::builder().launch(()).detach();

        root.append(clock.widget());
        root.append(weather.widget());

        ComponentParts {
            model: CenterGroup,
            widgets: CenterWidgets {
                _clock: clock,
                _weather: weather,
            },
        }
    }
}

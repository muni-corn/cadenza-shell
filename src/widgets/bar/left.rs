use gtk4::prelude::BoxExt;
use relm4::prelude::*;

use crate::{settings::BarConfig, tiles::niri::NiriTile};

#[derive(Debug)]
pub struct LeftGroup;

#[derive(Debug)]
pub struct LeftWidgets {
    _niri_tile: relm4::Controller<NiriTile>,
}

impl SimpleComponent for LeftGroup {
    type Init = (gdk4::Monitor, BarConfig);
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = LeftWidgets;

    fn init(
        (_monitor, bar_config): Self::Init,
        root: Self::Root,
        _sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let niri_tile = NiriTile::builder().launch(bar_config).detach();

        root.append(niri_tile.widget());

        let widgets = LeftWidgets {
            _niri_tile: niri_tile,
        };

        ComponentParts {
            model: LeftGroup,
            widgets,
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

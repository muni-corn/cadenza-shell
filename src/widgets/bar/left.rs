use gdk4::Monitor;
use gtk4::prelude::BoxExt;
use relm4::prelude::*;

use crate::{
    settings::BarConfig,
    tiles::niri::{NiriInit, NiriTile},
};

pub struct LeftGroupInit {
    pub bar_config: BarConfig,
    pub monitor: Monitor,
}

#[derive(Debug)]
pub struct LeftGroup;

#[derive(Debug)]
pub struct LeftWidgets {
    _niri_tile: relm4::Controller<NiriTile>,
}

impl SimpleComponent for LeftGroup {
    type Init = LeftGroupInit;
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = LeftWidgets;

    fn init(
        LeftGroupInit {
            bar_config,
            monitor,
        }: Self::Init,
        root: Self::Root,
        _sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let niri_tile = NiriTile::builder()
            .launch(NiriInit {
                bar_config,
                monitor,
            })
            .detach();

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

use relm4::prelude::*;

use crate::{settings::BarConfig, tiles::niri::NiriTile};

#[derive(Debug)]
pub struct LeftGroup;

#[derive(Debug)]
pub struct LeftWidgets {}

impl SimpleComponent for LeftGroup {
    type Input = ();
    type Output = ();
    type Init = gdk4::Monitor;
    type Root = gtk::Box;
    type Widgets = LeftWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        ComponentParts {
            model: LeftGroup,
            widgets: LeftWidgets {},
        }
    }
}

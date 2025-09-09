use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::{BATTERY_LEVEL_0_CHARGING, BATTERY_LEVEL_100_CHARGED, BATTERY_MISSING},
    services::battery::{BatteryService, BatteryState},
    utils::icons::{BATTERY_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Attention, Tile, TileInit, TileMsg},
};

pub struct BatteryTile {
    state: Option<BatteryState>,
    _service: BatteryService,
    tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum BatteryMsg {
    ServiceUpdate(Option<BatteryState>),
}

pub struct BatteryWidgets {
    root: <BatteryTile as Component>::Root,
}

impl SimpleComponent for BatteryTile {
    type Init = ();
    type Input = BatteryMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = BatteryWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let _service =
            BatteryService::launch().with(move |b| sender.input(BatteryMsg::ServiceUpdate(b)));

        // initialize the tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        let model = BatteryTile {
            state: None,
            _service,
            tile,
        };

        ComponentParts {
            model,
            widgets: BatteryWidgets { root },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BatteryMsg::ServiceUpdate(new_state) => {
                self.state = new_state;

                // update attention state
                let attention = self
                    .state
                    .as_ref()
                    .map(|s| {
                        if s.is_critical() {
                            Attention::Alarm
                        } else if s.is_low() {
                            Attention::Warning
                        } else {
                            Attention::Normal
                        }
                    })
                    .unwrap_or(Attention::Dim);

                // update the tile with new data
                self.tile.emit(TileMsg::UpdateData {
                    icon: Some(self.get_icon().to_string()),
                    primary: self.get_text(),
                    secondary: self.get_secondary_text(),
                });

                // update visibility and attention
                self.tile.emit(TileMsg::SetVisible(self.state.is_some()));
                self.tile.emit(TileMsg::SetAttention(attention));
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(self.available);
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

impl BatteryTile {
    fn get_icon(&self) -> &str {
        self.state
            .as_ref()
            .map(|s| {
                if s.charging {
                    if s.percentage > 0.99 {
                        BATTERY_LEVEL_100_CHARGED
                    } else {
                        BATTERY_LEVEL_0_CHARGING
                    }
                } else {
                    percentage_to_icon_from_list(s.percentage, BATTERY_ICON_NAMES)
                }
            })
            .unwrap_or(BATTERY_MISSING)
    }

    fn get_text(&self) -> Option<String> {
        self.state.as_ref().map(|s| {
            if s.charging && s.percentage > 0.99 {
                "Full".to_string()
            } else {
                format!("{}%", (s.percentage * 100.0) as u32)
            }
        })
    }

    fn get_secondary_text(&self) -> Option<String> {
        self.state.as_ref().map(BatteryState::get_readable_time)
    }
}

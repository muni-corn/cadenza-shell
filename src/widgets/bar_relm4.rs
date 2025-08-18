use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::tiles::battery_relm4::BatteryTile;
use crate::tiles::bluetooth_relm4::BluetoothTile;
use crate::tiles::brightness_relm4::BrightnessTile;
use crate::tiles::clock_relm4::ClockTile;
use crate::tiles::hyprland_relm4::{WorkspacesTile, FocusedClientTile};
use crate::tiles::network_relm4::NetworkTile;
use crate::tiles::volume_relm4::VolumeTile;

pub struct BarModel {
    monitor: Monitor,
    // Tile controllers
    battery: Controller<BatteryTile>,
    bluetooth: Controller<BluetoothTile>,
    brightness: Controller<BrightnessTile>,
    clock: Controller<ClockTile>,
    workspaces: Controller<WorkspacesTile>,
    focused_client: Controller<FocusedClientTile>,
    network: Controller<NetworkTile>,
    volume: Controller<VolumeTile>,
}

#[derive(Debug)]
pub enum BarMsg {
    TileClicked(String),
    UpdateLayout,
}

#[derive(Debug)]
pub enum BarOutput {
    // Currently no outputs needed
}

#[relm4::component]
impl SimpleComponent for BarModel {
    type Init = Monitor;
    type Input = BarMsg;
    type Output = BarOutput;

    view! {
        #[root]
        window = gtk::ApplicationWindow {
            set_title: Some("Muse Shell Bar"),
            #[watch]
            set_visible: true,
            
            // Configure as layer shell window
            init => LayerShell::init_layer_shell,
            set_layer: Layer::Top,
            set_exclusive_zone: 32,
            set_anchor: (Edge::Top, true),
            set_anchor: (Edge::Left, true),
            set_anchor: (Edge::Right, true),
            #[watch]
            set_monitor: Some(&model.monitor),

            #[name = "bar_container"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                add_css_class: "bar",
                set_height_request: 32,

                // Left section - workspaces and focused client
                #[name = "left_section"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 20,
                    
                    model.workspaces.widget(),
                    model.focused_client.widget(),
                },

                // Center section - clock
                #[name = "center_section"] 
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Center,
                    set_hexpand: true,
                    
                    model.clock.widget(),
                },

                // Right section - system tiles
                #[name = "right_section"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_halign: gtk::Align::End,
                    set_hexpand: true,
                    
                    model.brightness.widget(),
                    model.volume.widget(),
                    model.bluetooth.widget(),
                    model.network.widget(),
                    model.battery.widget(),
                },
            }
        }
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize all tile components
        let battery = BatteryTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                // Handle tile outputs if needed
                BarMsg::TileClicked("battery".to_string())
            });

        let bluetooth = BluetoothTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("bluetooth".to_string())
            });

        let brightness = BrightnessTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("brightness".to_string())
            });

        let clock = ClockTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("clock".to_string())
            });

        let workspaces = WorkspacesTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("workspaces".to_string())
            });

        let focused_client = FocusedClientTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("focused_client".to_string())
            });

        let network = NetworkTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("network".to_string())
            });

        let volume = VolumeTile::builder()
            .launch(())
            .forward(sender.input_sender(), |_output| {
                BarMsg::TileClicked("volume".to_string())
            });

        let model = BarModel {
            monitor,
            battery,
            bluetooth,
            brightness,
            clock,
            workspaces,
            focused_client,
            network,
            volume,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BarMsg::TileClicked(tile_name) => {
                log::debug!("Tile clicked: {}", tile_name);
                // Handle tile clicks - could show popups, menus, etc.
            }
            BarMsg::UpdateLayout => {
                // Handle layout updates if needed
                log::debug!("Updating bar layout");
            }
        }
    }
}

// Re-export for easier importing
pub type BarComponent = BarModel;
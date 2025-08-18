use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;

#[derive(Debug)]
pub struct TrayWidget {
    item_count: u32,
    visible: bool,
}

#[derive(Debug)]
pub enum TrayMsg {
    Click,
    UpdateItems(u32), // number of tray items
}

#[relm4::component(pub)]
impl SimpleComponent for TrayWidget {
    type Init = ();
    type Input = TrayMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "tray",
            #[watch]
            set_visible: model.visible,

            connect_clicked[sender] => move |_| {
                sender.input(TrayMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Label {
                    set_label: "󰅌", // System tray icon
                    add_css_class: "tile-icon",
                },

                gtk::Label {
                    #[watch]
                    set_text: &if model.item_count > 0 {
                        model.item_count.to_string()
                    } else {
                        "".to_string()
                    },
                    #[watch]
                    set_visible: model.item_count > 0,
                    add_css_class: "tile-text",
                    add_css_class: "tray-count",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = TrayWidget {
            item_count: 0,
            visible: false,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            TrayMsg::Click => {
                log::debug!("Tray tile clicked");
                let _ = sender.output(TileOutput::Clicked("tray".to_string()));
            }
            TrayMsg::UpdateItems(count) => {
                self.item_count = count;
                self.visible = count > 0;
            }
        }
    }
}

pub fn create_tray_widget() -> gtk4::Widget {
    let controller = TrayWidget::builder().launch(()).detach();
    controller.widget().clone().into()
}
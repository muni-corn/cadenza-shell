use gtk4::prelude::*;
use relm4::prelude::*;
use system_tray::data::BaseMap;

use crate::widgets::tray_item::{TrayEvent, TrayItem, TrayItemOutput};

#[derive(Debug)]
pub struct TrayWidget {
    items: AsyncFactoryVecDeque<TrayItem>,
    visible: bool,
    expanded: bool,
}

#[derive(Debug)]
pub enum TrayMsg {
    ToggleExpanded,
    TrayEvent(TrayEvent),
}

#[relm4::component(pub, async)]
impl SimpleAsyncComponent for TrayWidget {
    type Init = BaseMap;
    type Input = TrayMsg;
    type Output = TrayItemOutput;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
            #[watch]
            set_visible: model.visible,

            #[name(revealer)]
            gtk::Revealer {
                #[watch]
                set_reveal_child: model.expanded,
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_transition_duration: 200,

                #[local_ref]
                items_box -> gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 2,
                    set_margin_end: 4,
                }
            },

            gtk::Button {
                add_css_class: "tile",
                add_css_class: "tray",

                connect_clicked[sender] => move |_| {
                    sender.input(TrayMsg::ToggleExpanded);
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_halign: gtk::Align::Center,

                    gtk::Label {
                        #[watch]
                        set_label: if model.expanded { "󰅂" } else { "󰅁" }, // Arrow icons
                        add_css_class: "tile-icon",
                    },

                    gtk::Label {
                        #[watch]
                        set_text: &if !model.items.is_empty() {
                            model.items.len().to_string()
                        } else {
                            "".to_string()
                        },
                        #[watch]
                        set_visible: !model.items.is_empty(),
                        add_css_class: "tile-text",
                        add_css_class: "tray-count",
                    },
                }
            }
        }
    }

    async fn init(
        current_tray_items: Self::Init,
        _root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let mut model = TrayWidget {
            items: AsyncFactoryVecDeque::builder()
                .launch(gtk::Box::default())
                .forward(sender.output_sender(), |output| output),
            visible: true,
            expanded: true,
        };

        for (address, (item, menu)) in current_tray_items.iter() {
            model
                .items
                .guard()
                .push_back((address.clone(), item.clone(), menu.clone()));
        }

        let items_box = model.items.widget();
        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        match msg {
            TrayMsg::ToggleExpanded => self.expanded = !self.expanded,
            TrayMsg::TrayEvent(event) => match event {
                TrayEvent::Add(address, status_notifier_item) => {
                    // TODO: get TrayMenu here
                    self.items
                        .guard()
                        .push_back((address, *status_notifier_item, None));
                }
                TrayEvent::Update(address, update_event) => {
                    log::debug!("tray item {} updated: {:?}", address, update_event);
                    // match update_event {
                    //     UpdateEvent::AttentionIcon(_) => todo!(),
                    //     UpdateEvent::Icon {
                    //         icon_name,
                    //         icon_pixmap,
                    //     } => todo!(),
                    //     UpdateEvent::OverlayIcon(_) => todo!(),
                    //     UpdateEvent::Status(status) => todo!(),
                    //     UpdateEvent::Title(_) => todo!(),
                    //     UpdateEvent::Tooltip(tooltip) => todo!(),
                    //     UpdateEvent::Menu(tray_menu) => todo!(),
                    //     UpdateEvent::MenuDiff(menu_diffs) => todo!(),
                    //     UpdateEvent::MenuConnect(_) => todo!(),
                    // }
                }
                TrayEvent::Remove(name) => {
                    let index_opt = self
                        .items
                        .iter()
                        .position(|o| o.is_some_and(|i| *i.address() == name));

                    if let Some(index) = index_opt {
                        self.items.guard().remove(index);
                    }
                }
            },
        }
    }
}

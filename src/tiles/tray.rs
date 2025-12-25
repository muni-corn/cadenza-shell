use gtk4::prelude::*;
use relm4::prelude::*;
use system_tray::{data::BaseMap, item::StatusNotifierItem};

use crate::widgets::tray_item::{TrayEvent, TrayItem, TrayItemInput, TrayItemOutput};

#[derive(Debug)]
pub struct TrayWidget {
    items: FactoryVecDeque<TrayItem>,
    visible: bool,
    expanded: bool,
}

#[derive(Debug)]
pub enum TrayMsg {
    ToggleExpanded,
    TrayEvent(TrayEvent),
}

impl TrayWidget {
    fn replace_item(&mut self, address: &str, content: StatusNotifierItem) {
        if let Some(item) = self
            .items
            .guard()
            .iter_mut()
            .find(|item| *item.address() == address)
        {
            item.replace_inner(content)
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for TrayWidget {
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
            },

            gtk::Button {
                add_css_class: "tile",
                add_css_class: "tray",

                connect_clicked[sender] => move |_| {
                    sender.input(TrayMsg::ToggleExpanded);
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Center,

                    gtk::Label {
                        #[watch]
                        set_label: if model.expanded { "󰅂" } else { "󰅁" }, // Arrow icons
                        add_css_class: "tile-icon",
                    },
                }
            }
        }
    }

    fn init(
        current_tray_items: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = TrayWidget {
            items: FactoryVecDeque::builder()
                .launch(gtk::Box::default())
                .forward(sender.output_sender(), |output| output),
            visible: true,
            expanded: false,
        };

        for (address, (item, menu)) in current_tray_items.iter() {
            model
                .items
                .guard()
                .push_back((address.clone(), item.clone(), menu.clone()));
        }

        let widgets = view_output!();
        widgets.revealer.set_child(Some(model.items.widget()));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            TrayMsg::ToggleExpanded => self.expanded = !self.expanded,
            TrayMsg::TrayEvent(event) => match event {
                TrayEvent::Add(address, status_notifier_item) => {
                    let already_exists = self.items.iter().any(|i| *i.address() == address);

                    if already_exists {
                        self.replace_item(&address, *status_notifier_item);
                    } else {
                        self.items
                            .guard()
                            .push_back((address, *status_notifier_item, None));
                    }
                }
                TrayEvent::Update(address, update_event) => {
                    log::debug!("tray item {} updated: {:?}", address, update_event);
                    let index_opt = self
                        .items
                        .iter()
                        .find(|item| *item.address() == address)
                        .map(|item| item.index().current_index());

                    if let Some(index_to_update) = index_opt {
                        log::debug!("sending update to tray item at index {}", index_to_update);
                        self.items
                            .send(index_to_update, TrayItemInput::DataUpdate(update_event));
                    } else {
                        log::warn!("couldn't find tray item {} to send update", address);
                    }
                }
                TrayEvent::Remove(address) => {
                    let index_opt = self
                        .items
                        .iter()
                        .find(|item| *item.address() == address)
                        .map(|item| item.index().current_index());

                    if let Some(index) = index_opt {
                        log::debug!("removing tray item found for {address}");
                        self.items.guard().remove(index);
                    } else {
                        log::warn!("couldn't find tray item for {address}");
                    }
                }
            },
        }
    }
}

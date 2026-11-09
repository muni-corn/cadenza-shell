use std::sync::Arc;

use gtk4::prelude::*;
use relm4::prelude::*;
use system_tray::item::StatusNotifierItem;

use crate::{
    icon_names::{PAN_END, PAN_START},
    widgets::tray_item::{TrayClient, TrayEvent, TrayItem, TrayItemInput, TrayItemOutput},
};

/// The system tray: a toggle that reveals the status notifier items.
///
/// Each tray subscribes to the shared client itself and activates items
/// through it, so nothing has to relay tray traffic through the bar.
#[derive(Debug)]
pub struct TrayWidget {
    client: Arc<TrayClient>,
    items: FactoryVecDeque<TrayItem>,
    expanded: bool,
}

#[derive(Debug)]
pub enum TrayMsg {
    ToggleExpanded,
    Item(TrayItemOutput),
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
impl Component for TrayWidget {
    type CommandOutput = TrayEvent;
    type Init = Arc<TrayClient>;
    type Input = TrayMsg;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
            // there's nothing to reveal without items
            #[watch]
            set_visible: !model.items.is_empty(),

            #[name(revealer)]
            gtk::Revealer {
                add_css_class: "tray-revealer",
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

                    gtk::Image {
                        #[watch]
                        set_icon_name: Some(if model.expanded { PAN_END } else { PAN_START }),
                        add_css_class: "tile-icon",
                    },
                }
            }
        }
    }

    fn init(
        client: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // subscribe before reading the current items, so nothing added in
        // between is missed
        let mut events = client.subscribe();
        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    loop {
                        match events.recv().await {
                            Ok(event) => {
                                if out.send(event).is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(missed_events = n, "tray lagged on tray events");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                })
                .drop_on_shutdown()
        });

        let mut model = TrayWidget {
            items: FactoryVecDeque::builder()
                .launch(gtk::Box::default())
                .forward(sender.input_sender(), TrayMsg::Item),
            client,
            expanded: false,
        };

        let current_items = model.client.items();
        let current_items = current_items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (address, (item, menu)) in current_items.iter() {
            model
                .items
                .guard()
                .push_back((address.clone(), item.clone(), menu.clone()));
        }
        drop(current_items);

        let widgets = view_output!();
        widgets.revealer.set_child(Some(model.items.widget()));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            TrayMsg::ToggleExpanded => self.expanded = !self.expanded,
            TrayMsg::Item(TrayItemOutput::Activate(request)) => {
                let client = self.client.clone();
                // the activation produces no event, so nothing is sent back
                sender.command(|_, shutdown| {
                    shutdown
                        .register(async move {
                            if let Err(e) = client.activate(request).await {
                                tracing::error!("couldn't activate tray item: {e}");
                            }
                        })
                        .drop_on_shutdown()
                });
            }
        }
    }

    fn update_cmd(
        &mut self,
        event: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match event {
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
                let index_opt = self
                    .items
                    .iter()
                    .find(|item| *item.address() == address)
                    .map(|item| item.index().current_index());

                if let Some(index_to_update) = index_opt {
                    self.items
                        .send(index_to_update, TrayItemInput::DataUpdate(update_event));
                } else {
                    tracing::warn!("couldn't find tray item {} to send update", address);
                }
            }
            TrayEvent::Remove(address) => {
                let index_opt = self
                    .items
                    .iter()
                    .find(|item| *item.address() == address)
                    .map(|item| item.index().current_index());

                if let Some(index) = index_opt {
                    self.items.guard().remove(index);
                } else {
                    tracing::warn!("couldn't find tray item for {address}");
                }
            }
        }
    }
}

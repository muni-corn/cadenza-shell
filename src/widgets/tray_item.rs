use gtk4::prelude::*;
use relm4::{factory::FactoryView, prelude::*};
pub(crate) use system_tray::client::{Client as TrayClient, Event as TrayEvent};
use system_tray::{
    client::{ActivateRequest, UpdateEvent},
    data::apply_menu_diffs,
    item::{Status, StatusNotifierItem},
    menu::{MenuItem, MenuType, TrayMenu},
};

#[derive(Debug)]
pub struct TrayItem {
    inner: StatusNotifierItem,
    index: DynamicIndex,

    address: String,
    menu: Option<TrayMenu>,
}

pub struct TrayItemWidgets {
    popover: gtk::PopoverMenu,
    action_group: gio::SimpleActionGroup,
}

impl TrayItem {
    pub fn address(&self) -> &String {
        &self.address
    }

    pub fn index(&self) -> &DynamicIndex {
        &self.index
    }
}

#[derive(Debug)]
pub enum TrayItemInput {
    DataUpdate(UpdateEvent),
}

#[derive(Debug)]
pub enum TrayItemOutput {
    Activate(ActivateRequest),
    RequestMenu,
}

impl AsyncFactoryComponent for TrayItem {
    type CommandOutput = ();
    type Init = (String, StatusNotifierItem, Option<TrayMenu>);
    type Input = TrayItemInput;
    type Output = TrayItemOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = TrayItemWidgets;

    fn init_root() -> Self::Root {
        gtk::Button::builder().build()
    }

    async fn init_model(
        (address, inner, menu): Self::Init,
        index: &DynamicIndex,
        _sender: AsyncFactorySender<Self>,
    ) -> Self {
        log::info!(
            "initializing tray item: address={}, has_menu={}",
            address,
            menu.is_some()
        );

        Self {
            address,
            index: index.clone(),
            inner,
            menu,
        }
    }

    async fn update(&mut self, message: Self::Input, _sender: AsyncFactorySender<Self>) {
        match message {
            TrayItemInput::DataUpdate(update_event) => match update_event {
                UpdateEvent::AttentionIcon(attention_icon_name) => {
                    self.inner.attention_icon_name = attention_icon_name
                }
                UpdateEvent::Icon {
                    icon_name,
                    icon_pixmap,
                } => {
                    self.inner.icon_name = icon_name;
                    self.inner.icon_pixmap = icon_pixmap;
                }
                UpdateEvent::OverlayIcon(name) => self.inner.overlay_icon_name = name,
                UpdateEvent::Status(status) => self.inner.status = status,
                UpdateEvent::Title(title) => self.inner.title = title,
                UpdateEvent::Tooltip(tooltip) => self.inner.tool_tip = tooltip,
                UpdateEvent::Menu(tray_menu) => self.menu = Some(tray_menu),
                UpdateEvent::MenuDiff(menu_diffs) => {
                    if let Some(menu) = self.menu.as_mut() {
                        for _diff in &menu_diffs {
                            apply_menu_diffs(menu, &menu_diffs);
                        }
                    }
                }
                UpdateEvent::MenuConnect(menu_path) => {
                    log::info!(
                        "menu connected for tray item '{}': menu_path='{}', has_menu={}",
                        self.address,
                        menu_path,
                        self.menu.is_some()
                    );
                    self.inner.menu = Some(menu_path.clone());
                }
            },
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as FactoryView>::ReturnedWidget,
        sender: AsyncFactorySender<Self>,
    ) -> Self::Widgets {
        let (menu_model, action_group) = if let Some(ref menu) = self.menu
            && let Some(ref menu_path) = self.inner.menu
        {
            log::info!(
                "initializing menu with actions for '{}': menu_path='{}'",
                self.address,
                menu_path
            );
            menu.as_menu_with_actions(&sender, &self.address, menu_path)
        } else {
            log::warn!(
                "initializing tray item '{}' without menu (has_menu={}, has_menu_path={})",
                self.address,
                self.menu.is_some(),
                self.inner.menu.is_some()
            );
            (gio::Menu::new(), gio::SimpleActionGroup::new())
        };

        let popover = gtk::PopoverMenu::from_model(Some(&menu_model));
        popover.set_parent(&root);

        root.insert_action_group("tray", Some(&action_group));

        // set up the button styling
        root.add_css_class("tray-item");
        root.set_width_request(24);
        root.set_height_request(24);

        // add status-specific CSS classes
        match self.inner.status {
            Status::Active => root.add_css_class("tray-active"),
            Status::NeedsAttention => root.add_css_class("tray-needs-attention"),
            _ => {} // default styling
        }

        // Create enhanced tooltip with more information
        let tooltip_text = if let Some(tooltip) = &self.inner.tool_tip {
            format!("{:?}\n{}", self.inner.title, tooltip.description)
        } else {
            format!("{:?}\n{}", self.inner.title, self.inner.id)
        };
        root.set_tooltip_text(Some(&tooltip_text));

        // Create image or label for the button
        if let Some(icon_name) = &self.inner.icon_name {
            let image = gtk::Image::from_icon_name(icon_name);
            image.set_pixel_size(16);
            image.set_halign(gtk::Align::Center);
            image.set_valign(gtk::Align::Center);
            root.set_child(Some(&image));
        } else if let Some(_pixmap) = &self.inner.icon_pixmap {
            // TODO: Implement pixmap icon rendering in Phase 4
            // For now, fallback to text
            let label = gtk::Label::new(Some(&self.inner.id.chars().take(2).collect::<String>()));
            root.set_child(Some(&label));
        } else {
            // Fallback to text
            let label = gtk::Label::new(Some(&self.inner.id.chars().take(2).collect::<String>()));
            root.set_child(Some(&label));
        }

        // TODO: Left click - activate
        let address_clone = self.address.clone();
        let sender_clone = sender.clone();
        root.connect_clicked(move |_| {
            log::debug!("tray activate requested: {}", address_clone.clone());
            sender_clone
                .output(TrayItemOutput::Activate(ActivateRequest::Default {
                    address: address_clone.clone(),
                    x: 0,
                    y: 0,
                }))
                .unwrap_or_else(|_| log::error!("couldn't activate tray item {}", address_clone));
        });

        // right click for context menu
        // create a gesture for right-click detection
        let right_click_gesture = gtk::GestureClick::new();
        right_click_gesture.set_button(3); // right click
        let popover_clone = popover.clone();
        right_click_gesture.connect_pressed(move |_gesture, _, _, _| {
            popover_clone.popup();
        });
        root.add_controller(right_click_gesture);

        // middle click for context menu
        // create a gesture for middle-click detection
        let middle_click_gesture = gtk::GestureClick::new();
        middle_click_gesture.set_button(2); // middle click
        let popover_clone = popover.clone();
        middle_click_gesture.connect_pressed(move |_gesture, _, _x, _y| {
            popover_clone.popup();
        });
        root.add_controller(middle_click_gesture);

        TrayItemWidgets {
            popover,
            action_group,
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, sender: AsyncFactorySender<Self>) {
        if let Some(ref menu) = self.menu
            && let Some(ref menu_path) = self.inner.menu
        {
            log::info!(
                "updating menu view for '{}': menu_path='{}'",
                self.address,
                menu_path
            );
            let (menu_model, new_action_group) =
                menu.as_menu_with_actions(&sender, &self.address, menu_path);
            widgets.popover.set_menu_model(Some(&menu_model));

            // replace the old action group with the new one
            if let Some(parent) = widgets.popover.parent() {
                parent.insert_action_group("tray", Some(&new_action_group));
            }
            widgets.action_group = new_action_group;
        } else {
            log::debug!(
                "update_view called for '{}' but menu not ready (has_menu={}, has_menu_path={})",
                self.address,
                self.menu.is_some(),
                self.inner.menu.is_some()
            );
            widgets.popover.set_menu_model(None::<&gio::Menu>);
        }
    }
}

trait AsMenuWithActions {
    fn as_menu_with_actions(
        &self,
        sender: &AsyncFactorySender<TrayItem>,
        address: &str,
        menu_path: &str,
    ) -> (gio::Menu, gio::SimpleActionGroup);
}

impl AsMenuWithActions for TrayMenu {
    fn as_menu_with_actions(
        &self,
        sender: &AsyncFactorySender<TrayItem>,
        address: &str,
        menu_path: &str,
    ) -> (gio::Menu, gio::SimpleActionGroup) {
        create_menu_from_items(&self.submenus, sender, address, menu_path)
    }
}

fn clean_menu_label(label: &str) -> String {
    // handle underscore escaping: "__" becomes "_", single "_" are accelerator
    // markers
    label.replace("__", "\x01") // temporary replacement
         .replace('_', "") // remove single underscores (accelerator markers)
         .replace('\x01', "_") // restore double underscores as single
}

fn create_menu_from_items(
    items: &[MenuItem],
    sender: &AsyncFactorySender<TrayItem>,
    address: &str,
    menu_path: &str,
) -> (gio::Menu, gio::SimpleActionGroup) {
    log::debug!(
        "creating menu for address='{}', menu_path='{}', item_count={}",
        address,
        menu_path,
        items.len()
    );
    let action_group = gio::SimpleActionGroup::new();
    let menu = gio::Menu::new();

    let sections: Vec<_> = items
        .split(|i| i.menu_type == MenuType::Separator)
        .collect();

    for section_items in sections {
        let section_menu = gio::Menu::new();

        for item in section_items {
            if !item.visible || item.menu_type == MenuType::Separator {
                continue;
            }

            if let Some(label) = &item.label {
                let clean_label = clean_menu_label(label);

                if !item.submenu.is_empty() {
                    let (submenu, submenu_actions) =
                        create_menu_from_items(&item.submenu, sender, address, menu_path);
                    section_menu.append_submenu(Some(&clean_label), &submenu);

                    // merge submenu actions into parent action group
                    for action_name in submenu_actions.list_actions() {
                        if let Some(action) = submenu_actions.lookup_action(&action_name) {
                            action_group.add_action(&action);
                        }
                    }
                } else {
                    let action_name = format!("item-{}", item.id);
                    let action = gio::SimpleAction::new(&action_name, None);
                    action.set_enabled(item.enabled);

                    let sender_clone = sender.clone();
                    let address_clone = address.to_string();
                    let menu_path_clone = menu_path.to_string();
                    let submenu_id = item.id;

                    action.connect_activate(move |_, _| {
                        log::info!(
                            "menu item activated: address='{}', menu_path='{}', submenu_id={}",
                            address_clone,
                            menu_path_clone,
                            submenu_id
                        );
                        sender_clone
                            .output(TrayItemOutput::Activate(ActivateRequest::MenuItem {
                                address: address_clone.clone(),
                                menu_path: menu_path_clone.clone(),
                                submenu_id,
                            }))
                            .unwrap_or_else(|_| {
                                log::error!("failed to activate menu item {}", submenu_id)
                            });
                    });

                    action_group.add_action(&action);

                    let menu_item = gio::MenuItem::new(
                        Some(&clean_label),
                        Some(&format!("tray.{}", action_name)),
                    );
                    section_menu.append_item(&menu_item);
                }
            }
        }

        menu.append_section(None, &section_menu);
    }

    (menu, action_group)
}

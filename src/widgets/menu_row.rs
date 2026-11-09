//! A clickable row for the lists in the quick-settings menus.

use relm4::{
    factory::{FactoryVecDeque, FactoryView},
    gtk::{self, gdk, prelude::*},
    prelude::*,
};

/// What a menu row shows.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuRowData {
    /// Identifies the row's item, e.g. an ssid or a device address.
    pub key: String,
    /// An icon name; rows without one, or whose icon isn't in the theme,
    /// show no icon.
    pub icon: Option<String>,
    pub title: String,
    /// Dimmed text on the right, e.g. "Connected".
    pub status: Option<&'static str>,
    /// The remove button's tooltip; rows without one have no remove button.
    pub remove_tooltip: Option<&'static str>,
}

/// What the user did with a row, identified by its key.
#[derive(Debug)]
pub enum MenuRowOutput {
    Activated(String),
    Remove(String),
}

#[derive(Debug)]
pub enum MenuRowMsg {
    Activate,
    Remove,
    Update(MenuRowData),
}

#[derive(Debug)]
pub struct MenuRow {
    data: MenuRowData,
}

pub struct MenuRowWidgets {
    icon: gtk::Image,
    title: gtk::Label,
    status: gtk::Label,
    remove: gtk::Button,
}

impl FactoryComponent for MenuRow {
    type CommandOutput = ();
    type Index = DynamicIndex;
    type Init = MenuRowData;
    type Input = MenuRowMsg;
    type Output = MenuRowOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = MenuRowWidgets;

    fn init_model(data: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { data }
    }

    fn init_root(&self) -> Self::Root {
        gtk::Box::builder().spacing(4).build()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let select = gtk::Button::builder().hexpand(true).build();
        select.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(MenuRowMsg::Activate)
        });

        let content = gtk::Box::builder()
            .spacing(8)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let icon = gtk::Image::builder().width_request(24).build();
        let title = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let status = gtk::Label::builder()
            .css_classes(["dim"])
            .halign(gtk::Align::End)
            .build();
        content.append(&icon);
        content.append(&title);
        content.append(&status);
        select.set_child(Some(&content));

        let remove = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .build();
        remove.connect_clicked(move |_| sender.input(MenuRowMsg::Remove));

        root.append(&select);
        root.append(&remove);

        let widgets = MenuRowWidgets {
            icon,
            title,
            status,
            remove,
        };
        self.render(&widgets);
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            MenuRowMsg::Activate => {
                let _ = sender.output(MenuRowOutput::Activated(self.data.key.clone()));
            }
            MenuRowMsg::Remove => {
                let _ = sender.output(MenuRowOutput::Remove(self.data.key.clone()));
            }
            MenuRowMsg::Update(data) => self.data = data,
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: FactorySender<Self>) {
        self.render(widgets);
    }
}

impl MenuRow {
    fn render(&self, widgets: &MenuRowWidgets) {
        let icon = self.data.icon.as_deref().filter(|icon| has_icon(icon));
        widgets.icon.set_visible(icon.is_some());
        widgets.icon.set_icon_name(icon);

        widgets.title.set_label(&self.data.title);

        widgets.status.set_visible(self.data.status.is_some());
        widgets
            .status
            .set_label(self.data.status.unwrap_or_default());

        widgets
            .remove
            .set_visible(self.data.remove_tooltip.is_some());
        widgets.remove.set_tooltip_text(self.data.remove_tooltip);
    }
}

/// Whether the icon theme has an icon by this name.
fn has_icon(name: &str) -> bool {
    gdk::Display::default()
        .is_some_and(|display| gtk::IconTheme::for_display(&display).has_icon(name))
}

/// Makes `rows` show `data`, in order.
///
/// Rows are updated in place when the keys are unchanged, which is the
/// common case between refreshes; otherwise the list is rebuilt so the
/// order is always right.
pub fn sync_rows(rows: &mut FactoryVecDeque<MenuRow>, data: Vec<MenuRowData>) {
    let current_keys: Vec<&str> = rows.iter().map(|row| row.data.key.as_str()).collect();
    if same_keys(&current_keys, &data) {
        for (index, row) in data.into_iter().enumerate() {
            rows.send(index, MenuRowMsg::Update(row));
        }
    } else {
        let mut guard = rows.guard();
        guard.clear();
        for row in data {
            guard.push_back(row);
        }
    }
}

/// Whether `data` has exactly the keys in `current`, in the same order.
fn same_keys(current: &[&str], data: &[MenuRowData]) -> bool {
    current.len() == data.len() && current.iter().zip(data).all(|(key, row)| *key == row.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str) -> MenuRowData {
        MenuRowData {
            key: key.to_string(),
            icon: None,
            title: key.to_string(),
            status: None,
            remove_tooltip: None,
        }
    }

    #[test]
    fn same_keys_in_order_match() {
        assert!(same_keys(&["a", "b"], &[row("a"), row("b")]));
    }

    #[test]
    fn reordered_or_resized_keys_do_not_match() {
        assert!(!same_keys(&["a", "b"], &[row("b"), row("a")]));
        assert!(!same_keys(&["a"], &[row("a"), row("b")]));
    }
}

//! A clickable row for the lists in the quick-settings menus.

use relm4::{
    factory::FactoryVecDeque,
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
    /// Why the last action on the row's item failed, shown under the title.
    pub error: Option<String>,
    /// Whether an action on the row's item is in flight; busy rows show a
    /// spinner and ignore clicks so the action isn't started twice.
    pub busy: bool,
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

#[relm4::factory(pub)]
impl FactoryComponent for MenuRow {
    type CommandOutput = ();
    type Init = MenuRowData;
    type Input = MenuRowMsg;
    type Output = MenuRowOutput;
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Box {
            set_spacing: 4,

            gtk::Button {
                set_hexpand: true,
                connect_clicked => MenuRowMsg::Activate,

                gtk::Box {
                    set_spacing: 8,
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,

                    gtk::Image {
                        set_width_request: 24,
                        #[watch]
                        set_visible: self.icon().is_some(),
                        #[watch]
                        set_icon_name: self.icon(),
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_label: &self.data.title,
                        },

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            set_css_classes: &["warning"],
                            set_wrap: true,
                            set_xalign: 0.0,
                            #[watch]
                            set_visible: self.data.error.is_some(),
                            #[watch]
                            set_label: self.data.error.as_deref().unwrap_or_default(),
                        },
                    },

                    gtk::Label {
                        set_css_classes: &["dim"],
                        set_halign: gtk::Align::End,
                        #[watch]
                        set_visible: self.data.status.is_some() && !self.data.busy,
                        #[watch]
                        set_label: self.data.status.unwrap_or_default(),
                    },

                    gtk::Spinner {
                        #[watch]
                        set_visible: self.data.busy,
                        #[watch]
                        set_spinning: self.data.busy,
                    },
                },
            },

            gtk::Button {
                set_icon_name: "user-trash-symbolic",
                connect_clicked => MenuRowMsg::Remove,
                #[watch]
                set_visible: self.data.remove_tooltip.is_some(),
                #[watch]
                set_tooltip_text: self.data.remove_tooltip,
            },
        }
    }

    fn init_model(data: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { data }
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            MenuRowMsg::Activate | MenuRowMsg::Remove if self.data.busy => {}
            MenuRowMsg::Activate => {
                let _ = sender.output(MenuRowOutput::Activated(self.data.key.clone()));
            }
            MenuRowMsg::Remove => {
                let _ = sender.output(MenuRowOutput::Remove(self.data.key.clone()));
            }
            MenuRowMsg::Update(data) => self.data = data,
        }
    }
}

impl MenuRow {
    /// The row's icon, if it has one that the theme can show.
    fn icon(&self) -> Option<&str> {
        self.data.icon.as_deref().filter(|icon| has_icon(icon))
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
            error: None,
            busy: false,
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

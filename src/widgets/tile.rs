use gtk4::glib;
use gtk4::subclass::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attention {
    Alarm = 0,
    Warning = 1,
    Normal = 2,
    Dim = 3,
}

impl Default for Attention {
    fn default() -> Self {
        Self::Normal
    }
}

impl From<i32> for Attention {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Alarm,
            1 => Self::Warning,
            2 => Self::Normal,
            3 => Self::Dim,
            _ => Self::Normal,
        }
    }
}

impl From<Attention> for i32 {
    fn from(attention: Attention) -> Self {
        attention as i32
    }
}

impl Attention {
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Alarm => "alarm",
            Self::Warning => "warning",
            Self::Normal => "",
            Self::Dim => "dim",
        }
    }
}

mod imp {
    use super::Attention;
    use gtk4::glib::{self, Properties};
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use gtk4::{Label, Orientation};
    use std::cell::{Cell, RefCell};

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::Tile)]
    pub struct Tile {
        #[property(get, set, nullable)]
        icon: RefCell<Option<String>>,

        #[property(get, set, nullable)]
        primary: RefCell<Option<String>>,

        #[property(get, set, nullable)]
        secondary: RefCell<Option<String>>,

        #[property(get, set, minimum = 0, maximum = 3)]
        attention: Cell<i32>,

        #[property(get, set)]
        tile_visible: Cell<bool>,

        // UI elements
        icon_label: Label,
        primary_label: Label,
        secondary_label: Label,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Tile {
        const NAME: &'static str = "MuseShellTile";
        type Type = super::Tile;
        type ParentType = gtk4::Box;

        fn new() -> Self {
            let icon_label = Label::builder()
                .css_classes(vec!["icon"])
                .width_request(16)
                .build();

            let primary_label = Label::builder().css_classes(vec!["primary"]).build();

            let secondary_label = Label::builder().css_classes(vec!["secondary"]).build();

            Self {
                icon: RefCell::new(None),
                primary: RefCell::new(None),
                secondary: RefCell::new(None),
                attention: Cell::new(Attention::Normal.into()),
                tile_visible: Cell::new(true),
                icon_label,
                primary_label,
                secondary_label,
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Tile {
        fn constructed(&self) {
            self.parent_constructed();

            // Configure the box (self.obj() is the Box)
            let obj = self.obj();
            obj.set_orientation(Orientation::Horizontal);
            obj.set_spacing(12);
            obj.add_css_class("tile");

            // Add children to the box
            obj.append(&self.icon_label);
            obj.append(&self.primary_label);
            obj.append(&self.secondary_label);

            // Initial display update
            self.update_display();
        }
    }

    impl WidgetImpl for Tile {}

    impl BoxImpl for Tile {}

    impl Tile {
        pub fn update_display(&self) {
            let attention = Attention::from(self.attention.get());

            // Update icon
            if let Some(icon) = self.icon.borrow().as_ref() {
                let truncated = truncate(icon, 32);
                self.icon_label.set_text(&truncated);
                self.icon_label.set_visible(!truncated.is_empty());
            } else {
                self.icon_label.set_visible(false);
            }

            // Update primary text
            if let Some(primary) = self.primary.borrow().as_ref() {
                let truncated = truncate(primary, 32);
                self.primary_label.set_text(&truncated);
                self.primary_label.set_visible(!truncated.is_empty());
            } else {
                self.primary_label.set_visible(false);
            }

            // Update secondary text
            if let Some(secondary) = self.secondary.borrow().as_ref() {
                let truncated = truncate(secondary, 32);
                self.secondary_label.set_text(&truncated);
                self.secondary_label.set_visible(!truncated.is_empty());
            } else {
                self.secondary_label.set_visible(false);
            }

            // Update attention CSS classes
            for label in [&self.icon_label, &self.primary_label, &self.secondary_label] {
                // Remove existing attention classes
                for att in [Attention::Alarm, Attention::Warning, Attention::Dim] {
                    label.remove_css_class(att.css_class());
                }
                // Add current attention class
                if attention != Attention::Normal {
                    label.add_css_class(attention.css_class());
                }
            }

            // Update visibility
            self.obj().set_visible(self.tile_visible.get());
        }
    }

    fn truncate(s: &str, n: usize) -> String {
        if s.len() > n {
            format!("{}…", &s[..n])
        } else {
            s.to_string()
        }
    }
}

glib::wrapper! {
    pub struct Tile(ObjectSubclass<imp::Tile>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Orientable;
}

// Attention enum is defined at module level

impl Default for Tile {
    fn default() -> Self {
        Self::new()
    }
}

impl Tile {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn builder() -> TileBuilder {
        TileBuilder::new()
    }

    // Custom setters that trigger display updates
    pub fn set_tile_icon(&self, icon: Option<String>) {
        self.set_icon(icon);
        self.imp().update_display();
    }

    pub fn set_tile_primary(&self, primary: Option<String>) {
        self.set_primary(primary);
        self.imp().update_display();
    }

    pub fn set_tile_secondary(&self, secondary: Option<String>) {
        self.set_secondary(secondary);
        self.imp().update_display();
    }

    pub fn set_tile_attention(&self, attention: Attention) {
        let value: i32 = attention.into();
        self.set_attention(value);
        self.imp().update_display();
    }

    pub fn set_tile_visibility(&self, visible: bool) {
        self.set_tile_visible(visible);
        self.imp().update_display();
    }
}

// Builder pattern for easy construction
pub struct TileBuilder {
    icon: Option<String>,
    primary: Option<String>,
    secondary: Option<String>,
    attention: Attention,
    visible: bool,
}

impl TileBuilder {
    fn new() -> Self {
        Self {
            icon: None,
            primary: None,
            secondary: None,
            attention: Attention::Normal,
            visible: true,
        }
    }

    pub fn icon<S: Into<String>>(mut self, icon: S) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn primary<S: Into<String>>(mut self, primary: S) -> Self {
        self.primary = Some(primary.into());
        self
    }

    pub fn secondary<S: Into<String>>(mut self, secondary: S) -> Self {
        self.secondary = Some(secondary.into());
        self
    }

    pub fn attention(mut self, attention: Attention) -> Self {
        self.attention = attention;
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn build(self) -> Tile {
        let tile = Tile::new();
        tile.set_tile_icon(self.icon);
        tile.set_tile_primary(self.primary);
        tile.set_tile_secondary(self.secondary);
        tile.set_tile_attention(self.attention);
        tile.set_tile_visibility(self.visible);
        tile
    }
}

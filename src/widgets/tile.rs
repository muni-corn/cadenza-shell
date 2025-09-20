use gtk4::prelude::*;
use relm4::prelude::*;

use crate::tiles::Attention;

#[derive(Clone, Debug)]
pub struct Tile {
    icon: Option<String>,
    primary: Option<String>,
    secondary: Option<String>,
    attention: Attention,
}

#[derive(Debug)]
pub enum TileMsg {
    Click,
    SetIcon(Option<String>),
    SetPrimary(Option<String>),
    SetSecondary(Option<String>),
    SetAttention(Attention),
}

// Tile-specific messages
#[derive(Debug)]
pub enum TileMessage {
    Click,
    RightClick,
    MiddleClick,
    ScrollUp,
    ScrollDown,
    ShowPopup,
    HidePopup,
}

#[derive(Debug)]
pub enum TileMenuType {
    Context,
    Settings,
    Actions,
}

#[derive(Debug)]
pub enum TilePopupType {
    Details,
    Controls,
    Menu,
}

#[derive(Debug)]
pub enum TileOutput {
    Clicked,
    MenuRequested(String, TileMenuType),
    PopupRequested(String, TilePopupType),
}

#[derive(Debug)]
pub struct TileWidgets {
    icon: gtk::Image,
    primary_label: gtk::Label,
    secondary_label: gtk::Label,
}

pub struct TileInit {
    pub icon_name: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub attention: Attention,
}

impl Default for TileInit {
    fn default() -> Self {
        Self {
            icon_name: None,
            primary: None,
            secondary: None,
            attention: Attention::Normal,
        }
    }
}

impl SimpleComponent for Tile {
    type Init = TileInit;
    type Input = TileMsg;
    type Output = TileOutput;
    type Root = gtk::Button;
    type Widgets = TileWidgets;

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Tile {
            icon: init.icon_name,
            primary: init.primary,
            secondary: init.secondary,
            attention: init.attention,
        };

        // create container
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // create widgets
        let icon = gtk::Image::builder()
            .css_classes(vec!["icon", model.attention.css_class()])
            .pixel_size(20)
            .width_request(20)
            .build();

        let primary_label = gtk::Label::builder()
            .css_classes(vec!["primary", model.attention.css_class()])
            .build();

        let secondary_label = gtk::Label::builder()
            .css_classes(vec!["secondary", model.attention.css_class()])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(20)
            .build();

        // add all widgets to container
        container.append(&icon);
        container.append(&primary_label);
        container.append(&secondary_label);

        // add the container to the button
        root.set_child(Some(&container));

        // Setup click handler
        let sender_clone = sender.clone();
        root.connect_clicked(move |_| {
            sender_clone.input(TileMsg::Click);
        });

        let widgets = TileWidgets {
            icon,
            primary_label,
            secondary_label,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            TileMsg::Click => {
                let _ = sender.output(TileOutput::Clicked);
            }
            TileMsg::SetIcon(icon) => {
                self.icon = icon;
            }
            TileMsg::SetPrimary(primary) => {
                self.primary = primary;
            }
            TileMsg::SetSecondary(secondary) => {
                self.secondary = secondary;
            }
            TileMsg::SetAttention(attention) => {
                self.attention = attention;
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        // Update attention CSS classes
        let attention_class = self.attention.css_class();
        widgets.icon.set_css_classes(&["icon", attention_class]);
        widgets
            .primary_label
            .set_css_classes(&["primary", attention_class]);
        widgets
            .secondary_label
            .set_css_classes(&["secondary", attention_class]);

        // Update icon
        if let Some(icon_name) = &self.icon {
            widgets.icon.set_icon_name(Some(icon_name));
            widgets.icon.set_visible(true);
        } else {
            widgets.icon.set_visible(false);
        }

        // Update primary label
        if let Some(primary_text) = &self.primary {
            widgets.primary_label.set_label(primary_text);
            widgets.primary_label.set_visible(true);
        } else {
            widgets.primary_label.set_visible(false);
        }

        // Update secondary label
        if let Some(secondary_text) = &self.secondary {
            widgets.secondary_label.set_label(secondary_text);
            widgets.secondary_label.set_visible(true);
        } else {
            widgets.secondary_label.set_visible(false);
        }
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().css_classes(["tile"]).build()
    }
}

use gtk4::prelude::*;
use relm4::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Attention {
    Normal,
    Dim,
    Bright,
}

impl Attention {
    pub fn css_class(&self) -> &'static str {
        match self {
            Attention::Normal => "",
            Attention::Dim => "dim",
            Attention::Bright => "bright",
        }
    }
}

#[derive(Debug)]
pub struct ProgressTile {
    icon: Option<String>,
    progress: f64,
    visible: bool,
    attention: Attention,
    active: bool,
    extra_classes: Vec<String>,
    fade_timeout_source: Option<glib::SourceId>,
}

#[derive(Debug)]
pub enum ProgressTileMsg {
    /// Sets the icon of the ProgressTile.
    SetIcon(Option<String>),

    /// Sets the fraction of the ProgressTile, expanding it with an animation to
    /// show its fraction.
    SetProgress(f64),

    /// Sets the fraction of the ProgressTile without activating its animation.
    SetProgressSilently(f64),

    SetVisible(bool),

    SetAttention(Attention),

    Click,

    FadeTimeout,
}

#[derive(Debug)]
pub enum ProgressTileOutput {
    Clicked,
}

#[derive(Debug)]
pub struct ProgressTileWidgets {
    icon: gtk::Image,
    progress_bar: gtk::ProgressBar,
}

pub struct ProgressTileInit {
    pub icon_name: Option<String>,
    pub progress: f64,
    pub visible: bool,
    pub attention: Attention,
    pub extra_classes: Vec<String>,
}

impl Default for ProgressTileInit {
    fn default() -> Self {
        Self {
            icon_name: None,
            progress: 0.0,
            visible: true,
            attention: Attention::Normal,
            extra_classes: Vec::new(),
        }
    }
}

impl SimpleComponent for ProgressTile {
    type Init = ProgressTileInit;
    type Input = ProgressTileMsg;
    type Output = ProgressTileOutput;
    type Root = gtk::Button;
    type Widgets = ProgressTileWidgets;

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ProgressTile {
            icon: init.icon_name,
            progress: init.progress,
            visible: init.visible,
            attention: init.attention,
            extra_classes: init.extra_classes,
            fade_timeout_source: None,
            active: false,
        };

        // create container
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // create icon
        let icon = gtk::Image::builder()
            .css_classes(vec!["icon", model.attention.css_class()])
            .pixel_size(20)
            .width_request(20)
            .build();

        // create progress bar
        let progress_bar = gtk::ProgressBar::builder()
            .css_classes(vec![model.attention.css_class()])
            .fraction(model.progress)
            .valign(gtk::Align::Center)
            .build();

        // add widgets to container
        container.append(&icon);
        container.append(&progress_bar);

        root.set_child(Some(&container));

        // set initial values
        if let Some(icon_name) = &model.icon {
            icon.set_icon_name(Some(icon_name));
            icon.set_visible(true);
        } else {
            icon.set_visible(false);
        }

        let widgets = ProgressTileWidgets { icon, progress_bar };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ProgressTileMsg::Click => {
                // propagate click event to parent
                let _ = sender.output(ProgressTileOutput::Clicked);
            }
            ProgressTileMsg::SetIcon(icon) => {
                self.icon = icon;
            }
            ProgressTileMsg::SetProgress(progress) => {
                self.progress = progress;

                // trigger fade effect
                self.activate(&sender);
            }
            ProgressTileMsg::SetProgressSilently(progress) => {
                self.progress = progress;
            }
            ProgressTileMsg::SetVisible(visible) => {
                self.visible = visible;
            }
            ProgressTileMsg::SetAttention(attention) => {
                self.attention = attention;
            }
            ProgressTileMsg::FadeTimeout => {
                self.active = false;
                self.fade_timeout_source = None;
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        // update attention CSS classes
        let attention_class = self.attention.css_class();
        widgets.icon.set_css_classes(&["icon", attention_class]);
        widgets.progress_bar.set_css_classes(&[attention_class]);

        if self.active {
            widgets.icon.add_css_class("active");
            widgets.progress_bar.add_css_class("active");
        }

        // update icon
        if let Some(icon_name) = &self.icon {
            widgets.icon.set_icon_name(Some(icon_name));
            widgets.icon.set_visible(true);
        } else {
            widgets.icon.set_visible(false);
        }

        // update progress bar
        widgets.progress_bar.set_fraction(self.progress);
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().css_classes(["tile"]).build()
    }
}

impl ProgressTile {
    fn activate(&mut self, sender: &ComponentSender<Self>) {
        // cancel existing timeout
        if let Some(source_id) = self.fade_timeout_source.take() {
            source_id.remove();
        }

        self.active = true;

        // schedule fade back to dim
        let sender_clone = sender.clone();
        let source_id = glib::timeout_add_seconds_local(3, move || {
            sender_clone.input(ProgressTileMsg::FadeTimeout);
            glib::ControlFlow::Break
        });
        self.fade_timeout_source = Some(source_id);
    }
}

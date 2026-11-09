//! Building blocks shared by the quick-settings menus.

use std::rc::Rc;

use relm4::gtk::{self, glib, prelude::*};

/// The vertical box every menu is built in.
pub fn menu_root() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .vexpand(true)
        .width_request(320)
        .height_request(512)
        .build()
}

/// A scrollable column for a menu's content, returned as the scroller to add
/// to the menu and the box to fill.
pub fn scrolled_content() -> (gtk::ScrolledWindow, gtk::Box) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();
    (scroller, content)
}

/// A menu's title row: a large icon and an on/off switch.
#[derive(Debug)]
pub struct MenuHeader {
    pub root: gtk::Box,
    icon: gtk::Image,
    switch: gtk::Switch,
    handler: glib::SignalHandlerId,
}

impl MenuHeader {
    /// Builds the header; `on_toggle` runs whenever the user flips the
    /// switch.
    pub fn new(icon: &str, active: bool, on_toggle: impl Fn(bool) + 'static) -> Self {
        let root = gtk::Box::builder()
            .spacing(20)
            .hexpand(true)
            .css_classes(["content-title"])
            .build();
        let icon = gtk::Image::builder()
            .icon_name(icon)
            .icon_size(gtk::IconSize::Large)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build();
        let switch = gtk::Switch::builder()
            .active(active)
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .build();
        let handler = switch.connect_state_set(move |_, state| {
            on_toggle(state);
            glib::Propagation::Proceed
        });

        root.append(&icon);
        root.append(&switch);

        Self {
            root,
            icon,
            switch,
            handler,
        }
    }

    pub fn set_icon(&self, icon: &str) {
        self.icon.set_icon_name(Some(icon));
    }

    /// Moves the switch to `active` without reporting it as a toggle;
    /// `set_active` emits `state-set`, which would otherwise loop back.
    pub fn sync_switch(&self, active: bool) {
        if self.switch.is_active() == active {
            return;
        }
        self.switch.block_signal(&self.handler);
        self.switch.set_active(active);
        self.switch.unblock_signal(&self.handler);
    }
}

/// An inline prompt: a label, an optional text entry, an error line, and
/// Cancel and confirm buttons.
#[derive(Debug)]
pub struct PromptBox {
    pub root: gtk::Box,
    label: gtk::Label,
    entry: gtk::Entry,
    error: gtk::Label,
    buttons: gtk::Box,
    cancel: gtk::Button,
    confirm: gtk::Button,
}

impl PromptBox {
    /// Builds a hidden prompt.
    ///
    /// `on_submit` gets the entry's text when the entry is shown and `None`
    /// for confirm-only prompts; `secret` hides what's typed.
    pub fn new(
        confirm_label: &str,
        placeholder: &str,
        secret: bool,
        on_submit: impl Fn(Option<String>) + 'static,
        on_cancel: impl Fn() + 'static,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .visible(false)
            .build();
        let label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .wrap(true)
            .build();
        let entry = gtk::Entry::builder()
            .placeholder_text(placeholder)
            .visibility(!secret)
            .build();
        if secret {
            entry.set_input_purpose(gtk::InputPurpose::Password);
        }
        let error = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .css_classes(["warning"])
            .wrap(true)
            .visible(false)
            .build();

        let buttons = gtk::Box::builder().spacing(8).build();
        let cancel = gtk::Button::builder().label("Cancel").build();
        let confirm = gtk::Button::builder().label(confirm_label).build();
        buttons.append(&cancel);
        buttons.append(&confirm);

        cancel.connect_clicked(move |_| on_cancel());
        let on_submit = Rc::new(on_submit);
        let submit = {
            let entry = entry.clone();
            move || on_submit(WidgetExt::is_visible(&entry).then(|| entry.text().to_string()))
        };
        let submit = Rc::new(submit);
        confirm.connect_clicked({
            let submit = submit.clone();
            move |_| submit()
        });
        entry.connect_activate(move |_| submit());

        root.append(&label);
        root.append(&entry);
        root.append(&error);
        root.append(&buttons);

        Self {
            root,
            label,
            entry,
            error,
            buttons,
            cancel,
            confirm,
        }
    }

    /// Relabels the cancel and confirm buttons, e.g. to "Deny" and "Allow".
    pub fn set_button_labels(&self, cancel: &str, confirm: &str) {
        self.cancel.set_label(cancel);
        self.confirm.set_label(confirm);
    }

    /// Shows the prompt with `text`, with or without the entry and buttons.
    pub fn show(&self, text: &str, with_entry: bool, with_buttons: bool) {
        self.root.set_visible(true);
        self.label.set_label(text);
        self.entry.set_visible(with_entry);
        self.buttons.set_visible(with_buttons);
    }

    /// Hides the prompt and forgets what was typed.
    pub fn hide(&self) {
        self.root.set_visible(false);
        self.entry.set_text("");
        self.set_error(None);
    }

    /// Clears the entry and focuses it, for a fresh prompt.
    pub fn reset_entry(&self) {
        self.entry.set_text("");
        if WidgetExt::is_visible(&self.entry) {
            self.entry.grab_focus();
        }
    }

    /// Shows or hides the error line.
    pub fn set_error(&self, error: Option<&str>) {
        self.error.set_visible(error.is_some());
        self.error.set_label(error.unwrap_or_default());
    }
}

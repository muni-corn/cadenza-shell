

use gdk4::Monitor;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use gtk4::{ApplicationWindow, Button, Calendar, Image, Label, Orientation, ScrolledWindow};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::rc::Rc;

pub struct NotificationCenter {
    window: ApplicationWindow,
    container: gtk4::Box,
    notifications_container: gtk4::Box,
    cards: Rc<RefCell<HashMap<u32, NotificationCard>>>,
    service: NotificationService,
    visible: RefCell<bool>,
}

impl NotificationCenter {
    pub fn new(app: &gtk4::Application, monitor: &Monitor, service: NotificationService) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Muse Shell Notification Center")
            .visible(false)
            .build();

        // Configure layer shell
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_exclusive_zone(-1);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_monitor(Some(monitor));
        window.set_margin(Edge::Top, 8);
        window.set_margin(Edge::Right, 8);
        window.set_margin(Edge::Bottom, 8);

        let main_container = gtk4::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(32)
            .width_request(400)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        window.set_child(Some(&main_container));

        let center = Self {
            window,
            container: main_container.clone(),
            notifications_container: gtk4::Box::new(Orientation::Vertical, 8),
            cards: Rc::new(RefCell::new(HashMap::new())),
            service,
            visible: RefCell::new(false),
        };

        center.build_header();
        center.build_content();
        center.setup_service_connections();

        center
    }

    fn build_header(&self) {
        let header = gtk4::Box::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["notification-center-header"])
            .hexpand(true)
            .build();

        // Digital clock and date section
        let clock_section = gtk4::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .halign(gtk4::Align::Start)
            .valign(gtk4::Align::End)
            .hexpand(true)
            .build();

        let time_label = Label::builder()
            .css_classes(vec!["big-clock"])
            .halign(gtk4::Align::Start)
            .build();

        let date_label = Label::builder().halign(gtk4::Align::Start).build();

        // Update time and date every second
        let time_label_clone = time_label.clone();
        let date_label_clone = date_label.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            let now = chrono::Local::now();
            time_label_clone.set_text(&now.format("%-I:%M %P").to_string());
            date_label_clone.set_text(&now.format("%A, %B %-d, %Y").to_string());
            glib::ControlFlow::Continue
        });

        // Set initial time
        let now = chrono::Local::now();
        time_label.set_text(&now.format("%-I:%M %P").to_string());
        date_label.set_text(&now.format("%A, %B %-d, %Y").to_string());

        clock_section.append(&time_label);
        clock_section.append(&date_label);

        // Analog clock on the right
        let analog_clock_container = gtk4::Box::builder().halign(gtk4::Align::End).build();

        let analog_clock = AnalogClock::new(60);
        analog_clock_container.append(analog_clock.widget());

        header.append(&clock_section);
        header.append(&analog_clock_container);

        self.container.append(&header);
    }

    fn build_content(&self) {
        let scrolled = ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let content = gtk4::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .build();

        // Calendar
        let calendar = Calendar::builder().hexpand(true).build();
        content.append(&calendar);

        // Notifications header
        let notifications_header = gtk4::Box::builder()
            .orientation(Orientation::Horizontal)
            .build();

        let notifications_title = Label::builder()
            .label("Notifications")
            .css_classes(vec!["content-title"])
            .hexpand(true)
            .halign(gtk4::Align::Start)
            .build();

        let clear_all_button = Button::builder()
            .label("Clear all")
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::End)
            .build();

        let service_clone = self.service.clone();
        clear_all_button.connect_clicked(move |_| {
            service_clone.clear_all_notifications();
        });

        // Update clear button visibility based on notification count
        self.service
            .bind_property("has-notifications", &clear_all_button, "visible")
            .sync_create()
            .build();

        notifications_header.append(&notifications_title);
        notifications_header.append(&clear_all_button);
        content.append(&notifications_header);

        // Notifications container
        content.append(&self.notifications_container);

        // Empty state
        let empty_state = gtk4::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .valign(gtk4::Align::Center)
            .css_classes(vec!["empty-state"])
            .build();

        let empty_icon = Image::from_icon_name("notification-symbolic");
        empty_icon.set_pixel_size(48);
        let empty_label = Label::new(Some("No new notifications"));

        empty_state.append(&empty_icon);
        empty_state.append(&empty_label);

        // Show empty state when no notifications
        self.service
            .bind_property("has-notifications", &empty_state, "visible")
            .transform_to(|_, has_notifications: bool| Some(!has_notifications))
            .sync_create()
            .build();

        content.append(&empty_state);

        scrolled.set_child(Some(&content));
        self.container.append(&scrolled);
    }

    fn setup_service_connections(&self) {
        let notifications_container = self.notifications_container.clone();
        let cards_ref = self.cards.clone();
        let service = self.service.clone();

        // Monitor for notification changes
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            Self::update_notifications(&notifications_container, &cards_ref, &service);
            glib::ControlFlow::Continue
        });
    }

    fn update_notifications(
        container: &gtk4::Box,
        cards: &Rc<RefCell<HashMap<u32, NotificationCard>>>,
        service: &NotificationService,
    ) {
        let current_notifications = service.get_notifications();
        let mut cards_map = cards.borrow_mut();

        // Remove notifications that are no longer present
        let current_ids: std::collections::HashSet<u32> =
            current_notifications.iter().map(|n| n.id).collect();

        let mut to_remove = Vec::new();
        for (id, card) in cards_map.iter() {
            if !current_ids.contains(id) {
                container.remove(card.widget());
                to_remove.push(*id);
            }
        }

        for id in to_remove {
            cards_map.remove(&id);
        }

        // Add new notifications (sorted by timestamp, newest first)
        for notification in current_notifications {
            if !cards_map.contains_key(&notification.id) {
                let card = NotificationCard::new(notification.clone());
                container.prepend(card.widget()); // Add to top
                cards_map.insert(notification.id, card);
            }
        }
    }

    pub fn toggle_visibility(&self) {
        let is_visible = *self.visible.borrow();
        let new_visibility = !is_visible;

        self.window.set_visible(new_visibility);
        *self.visible.borrow_mut() = new_visibility;

        if new_visibility {
            self.window.present();
        }
    }

    pub fn set_visible(&self, visible: bool) {
        self.window.set_visible(visible);
        *self.visible.borrow_mut() = visible;

        if visible {
            self.window.present();
        }
    }

    pub fn is_visible(&self) -> bool {
        *self.visible.borrow()
    }

    pub fn window(&self) -> &ApplicationWindow {
        &self.window
    }
}
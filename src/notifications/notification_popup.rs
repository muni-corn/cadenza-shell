
use super::notification_card::NotificationCard;
use crate::services::notifications::NotificationService;
use gdk4::Monitor;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Box, Orientation};
use gtk4_layer_shell::{LayerShell, Layer, Edge};
use std::cell::RefCell;
use std::collections::HashMap;

pub struct NotificationPopup {
    window: ApplicationWindow,
    container: Box,
    cards: RefCell<HashMap<u32, NotificationCard>>,
    service: NotificationService,
}

impl NotificationPopup {
    pub fn new(app: &gtk4::Application, monitor: &Monitor, service: NotificationService) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Muse Shell Notifications")
            .build();

        // Configure layer shell
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_exclusive_zone(-1); // Don't reserve space
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_monitor(Some(monitor));
        window.set_margin(Edge::Top, 8);
        window.set_margin(Edge::Right, 8);

        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .width_request(400)
            .build();

        window.set_child(Some(&container));

        let popup = Self {
            window,
            container,
            cards: RefCell::new(HashMap::new()),
            service,
        };

        popup.setup_service_connections();
        popup.update_visibility();

        popup
    }

    fn setup_service_connections(&self) {
        let container = self.container.clone();
        let cards = self.cards.clone();
        let window = self.window.clone();

        // Connect to notification count changes to update visibility
        self.service.connect_notification_count_notify(
            glib::clone!(@weak window => move |service| {
                let has_notifications = service.notification_count() > 0;
                window.set_visible(has_notifications);
            }),
        );

        // Monitor for new notifications
        // Note: In a real implementation, we'd need to connect to the actual notification signals
        // For now, we'll poll periodically to check for new notifications
        let service = self.service.clone();
        let cards_ref = self.cards.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            Self::update_notifications(&container, &cards_ref, &service);
            glib::ControlFlow::Continue
        });
    }

    fn update_notifications(
        container: &Box,
        cards: &RefCell<HashMap<u32, NotificationCard>>,
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

        // Add new notifications
        for notification in current_notifications {
            if !cards_map.contains_key(&notification.id) {
                let card = NotificationCard::new(notification.clone());
                container.prepend(card.widget()); // Add to top
                cards_map.insert(notification.id, card);

                // Auto-dismiss after 10 seconds for non-critical notifications
                if notification.urgency < 2 {
                    let service_clone = service.clone();
                    let notification_id = notification.id;
                    glib::timeout_add_local_once(std::time::Duration::from_secs(10), move || {
                        // Remove from service (this will trigger the update loop to remove the card)
                        use gtk4::subclass::prelude::ObjectSubclassIsExt;
                        service_clone.imp().remove_notification(notification_id);
                    });
                }
            }
        }
    }

    fn update_visibility(&self) {
        let has_notifications = self.service.notification_count() > 0;
        self.window.set_visible(has_notifications);
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn window(&self) -> &ApplicationWindow {
        &self.window
    }
}
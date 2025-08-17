use chrono::{DateTime, Local};
use gtk4::glib;

mod imp {
    use chrono::{DateTime, Local, Timelike};
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::ClockService)]
    pub struct ClockService {
        #[property(get, set)]
        time_string: RefCell<String>,

        #[property(get, set)]
        date_string: RefCell<String>,

        #[property(get, set)]
        hour: RefCell<u32>,

        #[property(get, set)]
        minute: RefCell<u32>,

        #[property(get, set)]
        second: RefCell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClockService {
        const NAME: &'static str = "MuseShellClockService";
        type Type = super::ClockService;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ClockService {
        fn constructed(&self) {
            self.parent_constructed();

            // Initial time update
            self.update_time();

            // Start monitoring time changes every second
            self.start_monitoring();
        }
    }

    impl ClockService {
        fn update_time(&self) {
            let now: DateTime<Local> = Local::now();

            // Format time string (24-hour format)
            let time_str = now.format("%H:%M").to_string();

            // Format date string
            let date_str = now.format("%a, %b %d").to_string();

            // Update properties and notify
            self.obj().set_time_string(time_str);
            self.obj().set_date_string(date_str);
            self.obj().set_hour(now.hour());
            self.obj().set_minute(now.minute());
            self.obj().set_second(now.second());
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();

            // Update time every second
            glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                obj.imp().update_time();
                glib::ControlFlow::Continue
            });
        }
    }
}

glib::wrapper! {
    pub struct ClockService(ObjectSubclass<imp::ClockService>);
}

impl ClockService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn format_time_12h(&self) -> String {
        let now: DateTime<Local> = Local::now();
        now.format("%I:%M %p").to_string()
    }

    pub fn format_time_with_seconds(&self) -> String {
        let now: DateTime<Local> = Local::now();
        now.format("%H:%M:%S").to_string()
    }

    pub fn format_full_date(&self) -> String {
        let now: DateTime<Local> = Local::now();
        now.format("%A, %B %d, %Y").to_string()
    }

    pub fn is_am(&self) -> bool {
        self.hour() < 12
    }

    pub fn is_pm(&self) -> bool {
        self.hour() >= 12
    }
}

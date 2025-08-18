use chrono::{DateTime, Local};
use gtk4::prelude::*;
use relm4::prelude::*;
use crate::services::clock::ClockService;

// Clock icons matching the TypeScript version
const CLOCK_ICONS: &[&str] = &[
    "\u{F1456}", // 12 o'clock
    "\u{F144B}", // 1 o'clock
    "\u{F144C}", // 2 o'clock
    "\u{F144D}", // 3 o'clock
    "\u{F144E}", // 4 o'clock
    "\u{F144F}", // 5 o'clock
    "\u{F1450}", // 6 o'clock
    "\u{F1451}", // 7 o'clock
    "\u{F1452}", // 8 o'clock
    "\u{F1453}", // 9 o'clock
    "\u{F1454}", // 10 o'clock
    "\u{F1455}", // 11 o'clock
];

#[derive(Debug)]
struct ClockWidget {
    time: DateTime<Local>,
    service: ClockService,
}

#[derive(Debug)]
pub enum ClockMsg {
    Click,
    TimeUpdate(DateTime<Local>),
}

#[derive(Debug)]
pub enum ClockOutput {
    Clicked,
}

#[relm4::component]
impl SimpleComponent for ClockWidget {
    type Init = ();
    type Input = ClockMsg;
    type Output = ClockOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "clock",

            connect_clicked[sender] => move |_| {
                sender.input(ClockMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_halign: gtk::Align::Center,

                gtk::Label {
                    #[watch]
                    set_text: &model.get_icon(),
                    add_css_class: "clock-icon",
                },

                gtk::Label {
                    #[watch]
                    set_text: &model.get_time_text(),
                    add_css_class: "clock-time",
                },

                gtk::Label {
                    #[watch]
                    set_text: &model.get_date_text(),
                    add_css_class: "clock-date",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = ClockService::new();
        let current_time = Local::now();
        
        let model = ClockWidget {
            time: current_time,
            service: service.clone(),
        };

        let widgets = view_output!();

        // Connect to clock service updates - use a timer for now
        let sender_clone = sender.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            sender_clone.input(ClockMsg::TimeUpdate(Local::now()));
            glib::ControlFlow::Continue
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ClockMsg::Click => {
                let _ = sender.output(ClockOutput::Clicked);
            }
            ClockMsg::TimeUpdate(time) => {
                self.time = time;
            }
        }
    }
}

impl ClockWidget {
    pub fn new() -> Self {
        // Placeholder for compatibility - actual init happens via Relm4
        Self {
            time: Local::now(),
            service: ClockService::new(),
        }
    }

    pub fn widget(&self) -> &gtk4::Widget {
        // This method is kept for compatibility but won't be used in Relm4
        unimplemented!("Use Relm4 component instead")
    }

    pub fn service(&self) -> &ClockService {
        &self.service
    }

    fn get_icon(&self) -> String {
        let hour = self.time.hour() % 12;
        let icon = CLOCK_ICONS.get(hour as usize).unwrap_or(&CLOCK_ICONS[0]);
        icon.to_string()
    }

    fn get_time_text(&self) -> String {
        self.time.format("%H:%M").to_string()
    }

    fn get_date_text(&self) -> String {
        self.time.format("%m/%d").to_string()
    }
}

pub type ClockController = Controller<ClockWidget>;

pub fn create_clock_widget() -> relm4::ComponentBuilder<ClockWidget> {
    ClockWidget::builder()
}
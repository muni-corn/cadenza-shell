use gdk4::Display;
use gtk4::{Application, ApplicationWindow, prelude::*};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

use crate::style::load_css;
use crate::widgets::bar::Bar;

pub struct MuseShell {
    app: Application,
}

impl MuseShell {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("com.muse.shell")
            .build();

        Self { app }
    }

    pub fn run(self) -> gtk4::glib::ExitCode {
        let bars = Rc::new(RefCell::new(Vec::<ApplicationWindow>::new()));

        self.app.connect_activate({
            let bars = bars.clone();
            move |app| {
                Self::setup_ui(app, &bars);
            }
        });

        self.app.run()
    }

    fn setup_ui(app: &Application, bars: &Rc<RefCell<Vec<ApplicationWindow>>>) {
        // Load CSS styles
        load_css();

        let display = Display::default().expect("Could not get default display");
        let monitors = display.monitors();

        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            Self::create_bar(app, &monitor, bars);
        }
    }

    fn create_bar(
        app: &Application,
        monitor: &gdk4::Monitor,
        bars: &Rc<RefCell<Vec<ApplicationWindow>>>,
    ) {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Muse Shell Bar")
            .build();

        // Configure layer shell
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_exclusive_zone(32);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_monitor(Some(monitor));

        // Create bar content
        let bar = Bar::new(monitor);
        window.set_child(Some(bar.widget()));

        window.present();
        bars.borrow_mut().push(window);
    }
}
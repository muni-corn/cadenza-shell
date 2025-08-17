use gdk4::Display;
use glib::clone;

use crate::style::load_css;
use crate::widgets::bar::Bar;

pub struct MuseShell {
    app: Application,
    bars: Vec<ApplicationWindow>,
}

impl MuseShell {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("com.muse.shell")
            .build();

        Self {
            app,
            bars: Vec::new(),
        }
    }

    pub fn run(mut self) -> glib::ExitCode {
        self.app.connect_activate(clone!(@strong self as this => move |app| {
            this.setup_ui(app);
        }));

        self.app.run()
    }

    fn setup_ui(&mut self, app: &Application) {
        // Load CSS styles
        load_css();

        let display = Display::default().expect("Could not get default display");
        let monitors = display.monitors();

        for monitor in monitors.iter::<gdk4::Monitor>() {
            let monitor = monitor.unwrap();
            self.create_bar(app, &monitor);
        }
    }

    fn create_bar(&mut self, app: &Application, monitor: &gdk4::Monitor) {
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
        window.set_child(Some(&bar.widget()));

        window.present();
        self.bars.push(window);
    }
}

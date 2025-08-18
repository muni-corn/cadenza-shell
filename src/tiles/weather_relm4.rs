use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;

const WEATHER_ICONS: &[&str] = &["☀️", "⛅", "☁️", "🌧️", "⛈️", "❄️", "🌫️", "🌪️", "🔥", "💨"];

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temperature: i32,
    pub condition: String,
    pub icon: String,
    pub location: String,
}

impl Default for WeatherData {
    fn default() -> Self {
        Self {
            temperature: 20,
            condition: "Unknown".to_string(),
            icon: "☀️".to_string(),
            location: "Unknown".to_string(),
        }
    }
}

#[derive(Debug)]
struct WeatherWidget {
    weather_data: WeatherData,
    loading: bool,
}

#[derive(Debug)]
pub enum WeatherMsg {
    Click,
    UpdateWeather(WeatherData),
    StartLoading,
    StopLoading,
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherWidget {
    type Init = ();
    type Input = WeatherMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "weather",

            connect_clicked[sender] => move |_| {
                sender.input(WeatherMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                if model.loading {
                    gtk::Spinner {
                        set_spinning: true,
                        add_css_class: "tile-icon",
                    }
                } else {
                    gtk::Label {
                        #[watch]
                        set_label: &model.weather_data.icon,
                        add_css_class: "tile-icon",
                    }
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,

                    gtk::Label {
                        #[watch]
                        set_text: &format!("{}°C", model.weather_data.temperature),
                        add_css_class: "tile-text",
                        add_css_class: "weather-temp",
                    },

                    gtk::Label {
                        #[watch]
                        set_text: &model.weather_data.condition,
                        add_css_class: "tile-text",
                        add_css_class: "weather-condition",
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 12,
                    },
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WeatherWidget {
            weather_data: WeatherData::default(),
            loading: false,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            WeatherMsg::Click => {
                log::debug!("Weather tile clicked");
                let _ = sender.output(TileOutput::Clicked("weather".to_string()));
            }
            WeatherMsg::UpdateWeather(data) => {
                self.weather_data = data;
                self.loading = false;
            }
            WeatherMsg::StartLoading => {
                self.loading = true;
            }
            WeatherMsg::StopLoading => {
                self.loading = false;
            }
        }
    }
}

pub fn create_weather_widget() -> gtk4::Widget {
    let controller = WeatherWidget::builder().launch(()).detach();
    controller.widget().clone().into()
}
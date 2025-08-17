use anyhow::Result;
use chrono::{DateTime, Timelike, Utc};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WttrReport {
    pub current_condition: Vec<CurrentCondition>,
    pub weather: Vec<Weather>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentCondition {
    pub humidity: String,
    #[serde(rename = "FeelsLikeC")]
    pub feels_like_c: String,
    #[serde(rename = "FeelsLikeF")]
    pub feels_like_f: String,
    pub observation_time: String,
    pub temp_C: String,
    pub temp_F: String,
    #[serde(rename = "weatherCode")]
    pub weather_code: String,
    #[serde(rename = "weatherDesc")]
    pub weather_desc: Vec<WeatherDesc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherDesc {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weather {
    pub astronomy: Vec<Astronomy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Astronomy {
    pub sunrise: String,
    pub sunset: String,
}

// Weather icons using Material Design Icons
const DAY_WEATHER_ICONS: &[(&str, &str)] = &[
    ("113", "󰖙"), // Clear/Sunny
    ("116", "󰖕"), // Partly cloudy
    ("119", "󰖐"), // Cloudy
    ("122", "󰖐"), // Overcast
    ("143", "󰖑"), // Mist
    ("176", "󰖗"), // Patchy rain nearby
    ("179", "󰖘"), // Patchy snow nearby
    ("182", "󰖒"), // Patchy sleet nearby
    ("185", "󰖗"), // Patchy freezing drizzle nearby
    ("200", "󰖓"), // Thundery outbreaks nearby
    ("227", "󰖞"), // Blowing snow
    ("230", "󰖩"), // Blizzard
    ("248", "󰖑"), // Fog
    ("260", "󰖑"), // Freezing fog
    ("263", "󰖗"), // Patchy light drizzle
    ("266", "󰖗"), // Light drizzle
    ("281", "󰖗"), // Freezing drizzle
    ("284", "󰖖"), // Heavy freezing drizzle
    ("293", "󰖗"), // Patchy light rain
    ("296", "󰖗"), // Light rain
    ("299", "󰖖"), // Moderate rain at times
    ("302", "󰖖"), // Moderate rain
    ("305", "󰖖"), // Heavy rain at times
    ("308", "󰖖"), // Heavy rain
    ("311", "󰖗"), // Light freezing rain
    ("314", "󰖖"), // Moderate or Heavy freezing rain
    ("317", "󰙿"), // Light sleet
    ("320", "󰙿"), // Moderate or heavy sleet
    ("323", "󰖘"), // Patchy light snow
    ("326", "󰖘"), // Light snow
    ("329", "󰖘"), // Patchy moderate snow
    ("332", "󰖘"), // Moderate snow
    ("335", "󰖘"), // Patchy heavy snow
    ("338", "󰖘"), // Heavy snow
    ("350", "󰖒"), // Ice pellets
    ("353", "󰖗"), // Light rain shower
    ("356", "󰖖"), // Moderate or heavy rain shower
    ("359", "󰖖"), // Torrential rain shower
    ("362", "󰙿"), // Light sleet showers
    ("365", "󰙿"), // Moderate or heavy sleet showers
    ("368", "󰖘"), // Light snow showers
    ("371", "󰖘"), // Moderate or heavy snow showers
    ("374", "󰖒"), // Light showers of ice pellets
    ("377", "󰖒"), // Moderate or heavy showers of ice pellets
    ("386", "󰖓"), // Patchy light rain with thunder
    ("389", "󰖓"), // Moderate or heavy rain with thunder
    ("392", "󰖓"), // Patchy light snow with thunder
    ("395", "󰖓"), // Moderate or heavy snow with thunder
];

const NIGHT_WEATHER_ICONS: &[(&str, &str)] = &[
    ("113", "󰖔"), // Clear
    ("116", "󰖑"), // Partly cloudy
    ("323", "󰖘"), // Patchy light snow
    ("329", "󰖘"), // Patchy moderate snow
    ("335", "󰖘"), // Patchy heavy snow
    ("386", "󰖗"), // Patchy light rain with thunder
    ("392", "󰖗"), // Patchy light snow with thunder
];

const UNKNOWN_ICON: &str = "󰿹";

pub struct WeatherWidget {
    container: Box,
    icon_label: Label,
    temp_label: Label,
    desc_label: Label,
    last_update: RefCell<Option<DateTime<Utc>>>,
    current_weather: RefCell<Option<WttrReport>>,
}

impl WeatherWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .visible(false)
            .build();

        let icon_label = Label::builder()
            .css_classes(vec!["icon"])
            .label(UNKNOWN_ICON)
            .width_request(16)
            .build();

        let content_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .valign(gtk4::Align::Center)
            .build();

        let temp_label = Label::builder()
            .css_classes(vec!["primary"])
            .halign(gtk4::Align::Start)
            .label("--°")
            .build();

        let desc_label = Label::builder()
            .css_classes(vec!["secondary"])
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(20)
            .build();

        content_box.append(&temp_label);
        content_box.append(&desc_label);

        container.append(&icon_label);
        container.append(&content_box);

        let widget = Self {
            container,
            icon_label,
            temp_label,
            desc_label,
            last_update: RefCell::new(None),
            current_weather: RefCell::new(None),
        };

        widget.start_weather_updates();

        widget
    }

    fn start_weather_updates(&self) {
        // Update weather immediately
        self.update_weather();

        // Set up periodic updates every 10 minutes
        let widget_weak = glib::object::ObjectExt::downgrade(&self.container);
        let icon_label = self.icon_label.clone();
        let temp_label = self.temp_label.clone();
        let desc_label = self.desc_label.clone();
        let last_update = self.last_update.clone();
        let current_weather = self.current_weather.clone();

        glib::timeout_add_local(std::time::Duration::from_secs(60), move || {
            if widget_weak.upgrade().is_none() {
                return glib::ControlFlow::Break;
            }

            // Check if we need to update (every 10 minutes)
            let should_update = {
                let last = last_update.borrow();
                match *last {
                    Some(last_time) => {
                        let now = Utc::now();
                        (now - last_time).num_minutes() >= 10
                    }
                    None => true,
                }
            };

            if should_update {
                Self::fetch_weather_data(
                    icon_label.clone(),
                    temp_label.clone(),
                    desc_label.clone(),
                    last_update.clone(),
                    current_weather.clone(),
                );
            }

            glib::ControlFlow::Continue
        });
    }

    fn update_weather(&self) {
        Self::fetch_weather_data(
            self.icon_label.clone(),
            self.temp_label.clone(),
            self.desc_label.clone(),
            self.last_update.clone(),
            self.current_weather.clone(),
        );
    }

    fn fetch_weather_data(
        icon_label: Label,
        temp_label: Label,
        desc_label: Label,
        last_update: RefCell<Option<DateTime<Utc>>>,
        current_weather: RefCell<Option<WttrReport>>,
    ) {
        glib::spawn_future_local(async move {
            match Self::get_weather_data().await {
                Ok(weather) => {
                    Self::update_display(&weather, &icon_label, &temp_label, &desc_label);
                    *last_update.borrow_mut() = Some(Utc::now());
                    *current_weather.borrow_mut() = Some(weather);
                }
                Err(e) => {
                    log::warn!("Failed to fetch weather data: {}", e);
                    // Keep showing last known data if available
                }
            }
        });
    }

    async fn get_weather_data() -> Result<WttrReport> {
        let output = tokio::process::Command::new("curl")
            .args(&["-s", "https://v2.wttr.in/?format=j1"])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!("curl command failed");
        }

        let json_str = String::from_utf8(output.stdout)?;
        let weather: WttrReport = serde_json::from_str(&json_str)?;

        Ok(weather)
    }

    fn update_display(
        weather: &WttrReport,
        icon_label: &Label,
        temp_label: &Label,
        desc_label: &Label,
    ) {
        if weather.current_condition.is_empty() || weather.weather.is_empty() {
            return;
        }

        let current = &weather.current_condition[0];
        let astronomy = &weather.weather[0].astronomy[0];

        // Get weather icon
        let icon = Self::get_weather_icon(&current.weather_code, astronomy);
        icon_label.set_text(icon);

        // Set temperature
        temp_label.set_text(&format!("{}°", current.temp_F));

        // Set description
        if let Some(desc) = current.weather_desc.first() {
            desc_label.set_text(&desc.value);
        }

        // Show the widget
        icon_label.parent().unwrap().set_visible(true);
    }

    fn get_weather_icon(code: &str, astronomy: &Astronomy) -> &'static str {
        let is_night = Self::is_dark_time(astronomy);

        if is_night {
            // Try night icons first
            for (night_code, night_icon) in NIGHT_WEATHER_ICONS {
                if *night_code == code {
                    return night_icon;
                }
            }
        }

        // Fall back to day icons
        for (day_code, day_icon) in DAY_WEATHER_ICONS {
            if *day_code == code {
                return day_icon;
            }
        }

        UNKNOWN_ICON
    }

    fn is_dark_time(astronomy: &Astronomy) -> bool {
        let now = chrono::Local::now();
        let current_hour = now.hour();
        let current_minute = now.minute();

        // Parse sunrise and sunset times
        let (sunrise_hour, sunrise_minute) = Self::parse_time(&astronomy.sunrise);
        let (sunset_hour, sunset_minute) = Self::parse_time(&astronomy.sunset);

        // Check if current time is before sunrise or after sunset
        let current_minutes = current_hour * 60 + current_minute;
        let sunrise_minutes = sunrise_hour * 60 + sunrise_minute;
        let sunset_minutes = sunset_hour * 60 + sunset_minute;

        current_minutes < sunrise_minutes || current_minutes > sunset_minutes
    }

    fn parse_time(time_str: &str) -> (u32, u32) {
        // Parse time format like "06:30 AM" or "18:45 PM"
        let parts: Vec<&str> = time_str.split_whitespace().collect();
        if parts.len() != 2 {
            return (12, 0); // Default to noon if parsing fails
        }

        let time_part = parts[0];
        let meridiem = parts[1];

        let time_components: Vec<&str> = time_part.split(':').collect();
        if time_components.len() != 2 {
            return (12, 0);
        }

        let mut hours: u32 = time_components[0].parse().unwrap_or(12);
        let minutes: u32 = time_components[1].parse().unwrap_or(0);

        // Convert to 24-hour format
        if meridiem.eq_ignore_ascii_case("PM") && hours != 12 {
            hours += 12;
        } else if meridiem.eq_ignore_ascii_case("AM") && hours == 12 {
            hours = 0;
        }

        (hours, minutes)
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}

impl Default for WeatherWidget {
    fn default() -> Self {
        Self::new()
    }
}
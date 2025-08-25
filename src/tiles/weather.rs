use std::time::Duration;

use chrono::{Local, Timelike};
use gtk4::prelude::*;
use relm4::prelude::*;
use serde::Deserialize;
use tokio::time::sleep;

use crate::icon_names;
use crate::messages::TileOutput;

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temperature: i32,
    pub condition: String,
    pub icon: String,
}

#[derive(Debug)]
pub struct WeatherTile {
    weather_data: Option<WeatherData>,
    loading: bool,
}

#[derive(Debug)]
pub enum WeatherMsg {
    Click,
    StartLoading,
    StopLoading,
    UpdateWeather(WeatherData),
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherTile {
    type Init = ();
    type Input = WeatherMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "weather",

            #[watch]
            set_visible: model.weather_data.is_some(),

            connect_clicked[sender] => move |_| {
                sender.input(WeatherMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(
                        if let Some(data) = &model.weather_data {
                            data.icon.as_str()
                        } else {
                            icon_names::CLOUDS_OUTLINE
                        }
                    ),
                },


                gtk::Label {
                    #[watch]
                    set_label: &if let Some(data) = &model.weather_data {
                        format!("{}°C", data.temperature)
                    } else {
                        "--".to_string()
                    },
                    add_css_class: "tile-text",
                    add_css_class: "weather-temp"
                },
                gtk::Label {
                    #[watch]
                    set_label: if let Some(data) = &model.weather_data {
                        data.condition.as_str()
                    } else if model.loading {
                        "Loading…"
                    } else {
                        "Unknown"
                    },
                    add_css_class: "tile-text",
                    add_css_class: "weather-condition",
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 12
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WeatherTile {
            weather_data: None,
            loading: false,
        };

        // start polling in background
        start_polling(sender.clone());

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            WeatherMsg::Click => {
                let _ = sender.output(TileOutput::Clicked("weather".to_string()));
            }
            WeatherMsg::StartLoading => self.loading = true,
            WeatherMsg::StopLoading => self.loading = false,
            WeatherUpdateMsg::Finish(data) => {
                self.weather_data = Some(data);
                self.loading = false;
            }
        }
    }
}

#[derive(Deserialize, Debug)]
struct WttrDesc {
    value: String,
}

#[derive(Deserialize, Debug)]
struct WttrCondition {
    #[serde(rename = "temp_C")]
    temp_c: String,
    #[serde(rename = "weatherCode")]
    weather_code: String,
    #[serde(rename = "weatherDesc")]
    weather_desc: Vec<WttrDesc>,
}

#[derive(Deserialize, Debug)]
struct WttrAstronomy {
    sunrise: String,
    sunset: String,
}

#[derive(Deserialize, Debug)]
struct WttrDay {
    astronomy: Vec<WttrAstronomy>,
}

#[derive(Deserialize, Debug)]
struct WttrReport {
    current_condition: Vec<WttrCondition>,
    weather: Vec<WttrDay>,
}

fn is_dark_now(sunrise: &str, sunset: &str) -> bool {
    let now = Local::now();
    let (sh, sm) = parse_time_12h(sunrise).unwrap_or((6, 0));
    let (eh, em) = parse_time_12h(sunset).unwrap_or((18, 0));
    let h = now.hour();
    let m = now.minute();
    let before_sunrise = h < sh || (h == sh && m < sm);
    let after_sunset = h > eh || (h == eh && m > em);
    before_sunrise || after_sunset
}

fn parse_time_12h(s: &str) -> Option<(u32, u32)> {
    let (time, mer) = s.split_once(' ')?;
    let (h, m) = time.split_once(':')?;
    let mut h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    let mer = mer.trim();
    if mer.eq_ignore_ascii_case("PM") && h != 12 {
        h += 12;
    }
    if mer.eq_ignore_ascii_case("AM") && h == 12 {
        h = 0;
    }
    Some((h, m))
}

fn map_icon(code: &str, dark: bool) -> &'static str {
    use crate::icon_names::*;
    match code {
        "113" => {
            if dark {
                MOON_OUTLINE
            } else {
                SUN_OUTLINE
            }
        }
        "116" => {
            if dark {
                MOON_CLOUDS_OUTLINE
            } else {
                FEW_CLOUDS_OUTLINE
            }
        }
        "119" | "122" => {
            if dark {
                MOON_CLOUDS_OUTLINE
            } else {
                CLOUDS_OUTLINE
            }
        }
        "143" | "248" | "260" => FOG,
        "176" | "263" | "266" | "293" | "296" | "353" => RAIN_SCATTERED_OUTLINE,
        "299" | "302" | "305" | "308" | "311" | "314" | "356" | "359" => RAIN_OUTLINE,
        "200" | "386" | "389" | "392" | "395" => STORM_OUTLINE,
        "182" | "185" | "317" | "320" | "350" | "362" | "365" => SNOW_OUTLINE,
        "179" | "223" | "227" | "230" | "323" | "326" | "329" | "332" | "335" | "338" | "368"
        | "371" => SNOW,
        _ => {
            if dark {
                MOON_OUTLINE
            } else {
                CLOUDS_OUTLINE
            }
        }
    }
}

async fn fetch_wttr() -> anyhow::Result<WeatherData> {
    let body = reqwest::get("https://v2.wttr.in/?format=j1")
        .await?
        .text()
        .await?;
    let parsed: WttrReport = serde_json::from_str(&body)?;
    let current = parsed
        .current_condition
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing current_condition"))?;
    let day0 = parsed
        .weather
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing weather[0]"))?;
    let astro = day0
        .astronomy
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing astronomy[0]"))?;
    let temp_c = current.temp_c.parse::<i32>().unwrap_or(0);
    let desc = current
        .weather_desc
        .first()
        .map(|d| d.value.clone())
        .unwrap_or_else(|| "Unknown".into());
    let icon = map_icon(
        &current.weather_code,
        is_dark_now(&astro.sunrise, &astro.sunset),
    )
    .to_string();

    Ok(WeatherData {
        temperature: temp_c,
        condition: desc,
        icon,
    })
}

fn start_polling(sender: ComponentSender<WeatherTile>) {
    relm4::spawn(async move {
        let mut backoff: Option<u64> = None; // None => 600s normal cadence
        loop {
            match fetch_wttr().await {
                Ok(data) => {
                    sender.input(WeatherMsg::UpdateWeather(data));
                    backoff = None;
                    continue;
                }
                Err(e) => {
                    log::error!("weather fetch failed: {e:?}");
                    sender.input(WeatherMsg::StopLoading);
                    backoff = Some(next_backoff(backoff));
                }
            }
            let wait = backoff.unwrap_or(600);
            sleep(Duration::from_secs(wait)).await;
            sender.input(WeatherMsg::StartLoading);
        }
    });
}

fn next_backoff(prev: Option<u64>) -> u64 {
    match prev {
        None => 60,
        Some(s) => (s * 2).clamp(60, 300), // cap at 5 minutes
    }
}

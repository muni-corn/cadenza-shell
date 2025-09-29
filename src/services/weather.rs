use std::time::Duration;

use chrono::{Local, Timelike};
use relm4::{ComponentSender, SharedState, Worker};
use tokio::time::sleep;

use crate::{
    icon_names::{
        WEATHER_CLOUDY_REGULAR, WEATHER_FOG_REGULAR, WEATHER_MOON_REGULAR,
        WEATHER_PARTLY_CLOUDY_DAY_REGULAR, WEATHER_PARTLY_CLOUDY_NIGHT_REGULAR,
        WEATHER_RAIN_REGULAR, WEATHER_RAIN_SHOWERS_DAY_REGULAR, WEATHER_SNOW_REGULAR,
        WEATHER_SNOW_SHOWER_DAY_REGULAR, WEATHER_SUNNY_REGULAR, WEATHER_THUNDERSTORM_REGULAR,
    },
    weather::types::{WeatherState, WttrReport},
};

pub static WEATHER_STATE: SharedState<Option<WeatherState>> = SharedState::new();

pub struct WeatherService;

#[derive(Debug)]
pub enum WeatherServiceMsg {
    Abort,
    Finish(WeatherState),
}

impl Worker for WeatherService {
    type Init = ();
    type Input = WeatherServiceMsg;
    type Output = Option<WeatherState>;

    fn init(_: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        Self::start_polling(sender);
        Self
    }

    fn update(&mut self, msg: Self::Input, _sender: relm4::ComponentSender<Self>) {
        match msg {
            WeatherServiceMsg::Abort => {
                *WEATHER_STATE.write() = None;
            }
            WeatherServiceMsg::Finish(data) => {
                *WEATHER_STATE.write() = Some(WeatherState {
                    temperature: data.temperature,
                    condition: data.condition,
                    icon: data.icon,
                });
            }
        }
    }
}

impl WeatherService {
    fn start_polling(sender: ComponentSender<WeatherService>) {
        relm4::spawn(async move {
            let mut backoff: Option<u64> = None; // None => 600s normal cadence
            loop {
                match fetch_wttr().await {
                    Ok(data) => {
                        sender.input(WeatherServiceMsg::Finish(data));
                        backoff = None;
                        continue;
                    }
                    Err(e) => {
                        log::error!("weather fetch failed: {e:?}");
                        sender.input(WeatherServiceMsg::Abort);
                        backoff = Some(next_backoff(backoff));
                    }
                }
                let wait = backoff.unwrap_or(600);
                sleep(Duration::from_secs(wait)).await;
            }
        });
    }
}

pub fn parse_time_12h(s: &str) -> Option<(u32, u32)> {
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
    match code {
        "113" => {
            if dark {
                WEATHER_MOON_REGULAR
            } else {
                WEATHER_SUNNY_REGULAR
            }
        }
        "116" => {
            if dark {
                WEATHER_PARTLY_CLOUDY_NIGHT_REGULAR
            } else {
                WEATHER_PARTLY_CLOUDY_DAY_REGULAR
            }
        }
        "119" | "122" => {
            if dark {
                WEATHER_PARTLY_CLOUDY_NIGHT_REGULAR
            } else {
                WEATHER_CLOUDY_REGULAR
            }
        }
        "143" | "248" | "260" => WEATHER_FOG_REGULAR,
        "176" | "263" | "266" | "293" | "296" | "353" => WEATHER_RAIN_SHOWERS_DAY_REGULAR,
        "299" | "302" | "305" | "308" | "311" | "314" | "356" | "359" => WEATHER_RAIN_REGULAR,
        "200" | "386" | "389" | "392" | "395" => WEATHER_THUNDERSTORM_REGULAR,
        "182" | "185" | "317" | "320" | "350" | "362" | "365" => WEATHER_SNOW_SHOWER_DAY_REGULAR,
        "179" | "223" | "227" | "230" | "323" | "326" | "329" | "332" | "335" | "338" | "368"
        | "371" => WEATHER_SNOW_REGULAR,
        _ => {
            if dark {
                WEATHER_MOON_REGULAR
            } else {
                WEATHER_CLOUDY_REGULAR
            }
        }
    }
}

async fn fetch_wttr() -> anyhow::Result<WeatherState> {
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
    let temp_f = current.temp_f.parse::<i32>().unwrap_or(0);
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

    Ok(WeatherState {
        temperature: temp_f,
        condition: desc,
        icon,
    })
}

fn next_backoff(prev: Option<u64>) -> u64 {
    match prev {
        None => 60,
        Some(s) => (s * 2).clamp(60, 300), // cap at 5 minutes
    }
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

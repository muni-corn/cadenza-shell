pub mod types;

use std::time::Duration;

use chrono::{Local, Timelike};
use relm4::SharedState;
use tokio::time::sleep;

use crate::{
    icon_names::{
        DISPLAY_BRIGHTNESS, FEW_CLOUDS, FOG, MOON, MOON_CLOUD, RAIN, RAINDROPS, ROUND_CLOUD, SNOW,
        SNOWFLAKE, STORM,
    },
    utils::{backoff::Backoff, shared_state::set_if_changed},
    weather::types::{WeatherState, WttrReport},
};

/// The global weather state that can be subscribed to.
pub static WEATHER_STATE: SharedState<Option<WeatherState>> = SharedState::new();

/// How often to refresh the weather after a successful fetch.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Fetches the weather periodically, retrying failures with a backoff.
///
/// A failed fetch keeps showing the last good report rather than hiding
/// the tile over a brief outage.
#[tracing::instrument]
pub async fn start_weather_polling() {
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("couldn't create an http client for weather: {e}");
            return;
        }
    };

    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(5 * 60));
    loop {
        let wait = match fetch_wttr(&client).await {
            Ok(weather) => {
                backoff.reset();
                set_if_changed(&WEATHER_STATE, Some(weather));
                REFRESH_INTERVAL
            }
            Err(e) => {
                tracing::warn!("couldn't fetch the weather from wttr.in: {e:#}");
                backoff.next()
            }
        };
        sleep(wait).await;
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
                MOON
            } else {
                DISPLAY_BRIGHTNESS
            }
        }
        "116" => {
            if dark {
                MOON_CLOUD
            } else {
                FEW_CLOUDS
            }
        }
        "119" | "122" => ROUND_CLOUD,
        "143" | "248" | "260" => FOG,
        "176" | "263" | "266" | "293" | "296" | "353" => RAIN,
        "299" | "302" | "305" | "308" | "311" | "314" | "356" | "359" => RAINDROPS,
        "200" | "386" | "389" | "392" | "395" => STORM,
        "182" | "185" | "317" | "320" | "350" | "362" | "365" => SNOW,
        "179" | "223" | "227" | "230" | "323" | "326" | "329" | "332" | "335" | "338" | "368"
        | "371" => SNOWFLAKE,
        _ => {
            if dark {
                MOON_CLOUD
            } else {
                FEW_CLOUDS
            }
        }
    }
}

#[tracing::instrument(skip(client))]
async fn fetch_wttr(client: &reqwest::Client) -> anyhow::Result<WeatherState> {
    let parsed: WttrReport = client
        .get("https://v2.wttr.in/?format=j1")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    tracing::debug!(report = ?parsed, "parsed weather report");

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

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
    sleep_monitor,
    utils::{backoff::Backoff, shared_state::set_if_changed},
    weather::types::{WeatherState, WttrReport},
};

/// The global weather state that can be subscribed to.
pub static WEATHER_STATE: SharedState<Option<WeatherState>> = SharedState::new();

/// How long to wait after resuming before fetching, so the network has
/// reconnected.
const WAKE_SETTLE_DELAY: Duration = Duration::from_secs(5);

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
    let mut wake = sleep_monitor::subscribe_wake();
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

        // timers pause during suspend, so refetch soon after waking
        tokio::select! {
            () = sleep(wait) => {}
            Ok(()) = wake.recv() => {
                // give the network a moment to come back up
                sleep(WAKE_SETTLE_DELAY).await;
            }
        }
    }
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
    // sunrise and sunset are local to the forecast location, so compare them
    // with the observation's local time rather than this machine's clock
    let now = current
        .local_obs_date_time
        .split_once(' ')
        .and_then(|(_, time)| parse_time_12h(time))
        .unwrap_or_else(|| {
            let now = Local::now();
            (now.hour(), now.minute())
        });
    let icon = map_icon(
        &current.weather_code,
        is_dark(now, &astro.sunrise, &astro.sunset),
    )
    .to_string();

    Ok(WeatherState {
        temperature: temp_f,
        condition: desc,
        icon,
    })
}

/// Whether `now` (hours, minutes) is before sunrise or after sunset, given
/// wttr.in's 12-hour times. Unparsable times fall back to 6 am and 6 pm.
fn is_dark(now: (u32, u32), sunrise: &str, sunset: &str) -> bool {
    let minutes = |(hours, minutes): (u32, u32)| hours * 60 + minutes;
    let now = minutes(now);
    let sunrise = minutes(parse_time_12h(sunrise).unwrap_or((6, 0)));
    let sunset = minutes(parse_time_12h(sunset).unwrap_or((18, 0)));
    now < sunrise || now > sunset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_12_hour_times() {
        assert_eq!(parse_time_12h("07:05 AM"), Some((7, 5)));
        assert_eq!(parse_time_12h("12:30 AM"), Some((0, 30)));
        assert_eq!(parse_time_12h("12:30 PM"), Some((12, 30)));
        assert_eq!(parse_time_12h("05:45 PM"), Some((17, 45)));
        assert_eq!(parse_time_12h("nonsense"), None);
    }

    #[test]
    fn dark_before_sunrise_and_after_sunset() {
        assert!(is_dark((5, 59), "06:00 AM", "06:00 PM"));
        assert!(!is_dark((12, 0), "06:00 AM", "06:00 PM"));
        assert!(is_dark((18, 1), "06:00 AM", "06:00 PM"));
    }
}

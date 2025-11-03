fn main() {
    relm4_icons_build::bundle_icons(
        // name of the file that will be generated at `OUT_DIR`
        "icon_names.rs",
        // optional app id
        Some("com.musicaloft.cadenza-shell"),
        // custom base resource path
        None::<&str>,
        // directory with custom icons (if any)
        None::<&str>,
        // list of icons to include
        [
            // notifications
            "alert-regular",
            "alert-badge-regular",
            // volume
            "speaker-mute-regular",
            "speaker-off-regular",
            "speaker-0-regular",
            "speaker-1-regular",
            "speaker-2-regular",
            // battery
            "battery-1-regular",
            "battery-2-regular",
            "battery-3-regular",
            "battery-4-regular",
            "battery-5-regular",
            "battery-6-regular",
            "battery-7-regular",
            "battery-8-regular",
            "battery-9-regular",
            "battery-10-regular",
            "battery-charge-regular",
            "battery-checkmark-regular",
            // brightness
            "brightness-low-regular",
            "brightness-high-regular",
            // clock
            "clock-regular",
            // weather
            "weather-blowing-snow-regular",
            "weather-cloudy-regular",
            "weather-drizzle-regular",
            "weather-duststorm-regular",
            "weather-fog-regular",
            "weather-hail-day-regular",
            "weather-hail-night-regular",
            "weather-moon-regular",
            "weather-partly-cloudy-day-regular",
            "weather-partly-cloudy-night-regular",
            "weather-rain-regular",
            "weather-rain-showers-day-regular",
            "weather-rain-showers-night-regular",
            "weather-rain-snow-regular",
            "weather-snow-regular",
            "weather-snow-shower-day-regular",
            "weather-snow-shower-night-regular",
            "weather-snowflake-regular",
            "weather-squalls-regular",
            "weather-sunny-regular",
            "weather-thunderstorm-regular",
            // wifi
            "wifi-1-regular",
            "wifi-2-regular",
            "wifi-3-regular",
            "wifi-4-regular",
            // network misc
            "earth-regular",
            "globe-off-regular",
            // bluetooth
            "bluetooth-connected-regular",
            "bluetooth-disabled-regular",
            "bluetooth-regular",
            "bluetooth-searching-regular",
            // media/mpris
            "music-note-1-regular",
            "play-regular",
            "pause-regular",
            "stop-regular",
        ],
    );
}

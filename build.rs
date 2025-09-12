fn main() {
    relm4_icons_build::bundle_icons(
        // name of th6 file that will be generated at `OUT_DIR`
        "icon_names.rs",
        // optional app id
        Some("com.musicaloft.muse-shell"),
        // custom base resource path
        None::<&str>,
        // directory with custom icons (if any)
        None::<&str>,
        // list of icons to include
        [
            // audio
            "speaker-0",
            "speaker-1",
            "speaker-2",
            "speaker-3",
            // battery
            "battery-level-30",
            "battery-level-40",
            "battery-level-50",
            "battery-level-60",
            "battery-level-70",
            "battery-level-80",
            "battery-level-90",
            "battery-level-100",
            "battery-level-0-charging",
            "battery-level-100-charged",
            "battery-missing",
            // brightness
            "display-brightness-low",
            "display-brightness-medium",
            "display-brightness-high",
            // clock
            "clock-alt",
            // weather
            "clouds-outline",
            "few-clouds-outline",
            "fire",
            "fog",
            "moon-clouds-outline",
            "moon-outline",
            "rain-outline",
            "rain-scattered-outline",
            "snow",
            "snow-outline",
            "storm-outline",
            "sun-outline",
            "tornado",
            "windy",
            // wifi
            "radiowaves-1",
            "radiowaves-2",
            "radiowaves-3",
            "radiowaves-4",
        ],
    );
}

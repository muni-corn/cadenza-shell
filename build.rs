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
            "bell",
            // volume
            "speaker-cross",
            "speaker-max",
            "speaker-mid",
            "speaker-min",
            // battery
            "battery-empty",
            "battery-10",
            "battery-20",
            "battery-30",
            "battery-40",
            "battery-50",
            "battery-60",
            "battery-70",
            "battery-80",
            "battery-90",
            "battery-100",
            "battery-0-ch",
            "battery-10-ch",
            "battery-20-ch",
            "battery-30-ch",
            "battery-40-ch",
            "battery-50-ch",
            "battery-60-ch",
            "battery-70-ch",
            "battery-80-ch",
            "battery-90-ch",
            "battery-100-ch",
            // brightness
            "display-brightness",
            // clock
            "clock",
            // weather
            "moon",
            "few-clouds",
            "round-cloud",
            "fog",
            "rain",
            "raindrops",
            "storm",
            "snow",
            "snowflake",
            "moon-cloud",
            // wifi
            "radiowaves-x",
            "radiowaves-1",
            "radiowaves-2",
            "radiowaves-3",
            "radiowaves-4",
            "radiowaves-no",
            "radiowaves-question",
            // network misc
            "lan",
            "lan-question",
            // bluetooth
            "bluetooth",
            "bluetooth-dots",
            "bluetooth-x",
            "bluetooth-no",
            // media/mpris
            "music-note-single",
            "media-playback-pause",
            "media-playback-start",
        ],
    );
}

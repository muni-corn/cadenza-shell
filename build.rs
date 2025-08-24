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
            "clock-alt",
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
        ],
    );
}

use gdk4::Display;
use gtk4::CssProvider;

const DEFAULT_CSS: &str = r#"
/* muse-shell default theme */

/* bar styling */
.bar {
    background-color: rgba(30, 30, 46, 0.9);
    border-radius: 0;
    padding: 4px 12px;
    margin: 0;
    min-height: 32px;
}

/* workspace buttons */
.workspaces {
    margin-right: 16px;
}

.workspace-button {
    background-color: transparent;
    border: 2px solid transparent;
    border-radius: 6px;
    padding: 4px 8px;
    margin: 0 2px;
    min-width: 24px;
    min-height: 24px;
    color: rgba(205, 214, 244, 0.6);
    font-weight: bold;
    transition: all 200ms ease;
}

.workspace-button:hover {
    background-color: rgba(205, 214, 244, 0.1);
    color: rgba(205, 214, 244, 0.8);
}

.workspace-button.active {
    background-color: rgba(137, 180, 250, 0.3);
    border-color: rgba(137, 180, 250, 0.8);
    color: rgba(137, 180, 250, 1.0);
}

.workspace-button.occupied {
    color: rgba(205, 214, 244, 0.9);
    background-color: rgba(205, 214, 244, 0.1);
}

/* Focused client */
.focused-client {
    margin-right: 16px;
}

.client-class {
    color: rgba(249, 226, 175, 1.0);
    font-weight: bold;
    margin-right: 8px;
}

.client-title {
    color: rgba(205, 214, 244, 0.8);
}

/* clock */
.clock {
    margin: 0 16px;
}

.time {
    color: rgba(205, 214, 244, 1.0);
    font-size: 14px;
    font-weight: bold;
}

.date {
    color: rgba(205, 214, 244, 0.7);
    font-size: 11px;
}

/* tiles */
.tile {
    margin: 0 4px;
    padding: 2px 6px;
    border-radius: 4px;
    transition: all 200ms ease;
}

.tile:hover {
    background-color: rgba(205, 214, 244, 0.1);
}

/* icons */
.icon {
    font-size: 14px;
    transition: all 300ms ease;
}

.icon.dim {
    color: rgba(205, 214, 244, 0.6);
}

.icon.bright {
    color: rgba(205, 214, 244, 1.0);
}

/* progress bars */
progressbar {
    min-height: 4px;
    min-width: 16px;
    border-radius: 2px;
    transition: all 300ms ease;
}

progressbar.dim trough {
    background-color: rgba(205, 214, 244, 0.2);
}

progressbar.bright trough {
    background-color: rgba(205, 214, 244, 0.3);
}

progressbar.dim progress {
    background-color: rgba(137, 180, 250, 0.7);
}

progressbar.bright progress {
    background-color: rgba(137, 180, 250, 1.0);
}

/* battery specific styling */
.battery-low .icon {
    color: rgba(250, 179, 135, 1.0);
}

.battery-critical .icon {
    color: rgba(243, 139, 168, 1.0);
}

.battery-charging .icon {
    color: rgba(166, 227, 161, 1.0);
}

.battery-low progressbar progress {
    background-color: rgba(250, 179, 135, 1.0);
}

.battery-critical progressbar progress {
    background-color: rgba(243, 139, 168, 1.0);
}

.battery-charging progressbar progress {
    background-color: rgba(166, 227, 161, 1.0);
}

/* network specific styling */
.network-connected .icon {
    color: rgba(166, 227, 161, 1.0);
}

.network-disconnected .icon {
    color: rgba(243, 139, 168, 1.0);
}

.network-wifi .icon {
    color: rgba(137, 180, 250, 1.0);
}

.network-ethernet .icon {
    color: rgba(166, 227, 161, 1.0);
}

/* Bluetooth specific styling */
.bluetooth-enabled .icon {
    color: rgba(137, 180, 250, 1.0);
}

.bluetooth-disabled .icon {
    color: rgba(205, 214, 244, 0.4);
}

/* Percentage labels */
.percentage {
    font-size: 11px;
    font-weight: bold;
    transition: all 300ms ease;
}

.percentage.dim {
    color: rgba(205, 214, 244, 0.6);
}

.percentage.bright {
    color: rgba(205, 214, 244, 1.0);
}
"#;

pub fn load_css() -> Result<(), Box<dyn std::error::Error>> {
    let provider = CssProvider::new();
    provider.load_from_string(DEFAULT_CSS);

    let display = Display::default().ok_or("Could not connect to a display")?;

    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Ok(())
}

pub fn load_custom_css(css_content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let provider = CssProvider::new();
    provider.load_from_string(css_content);

    let display = Display::default().ok_or("Could not connect to a display")?;

    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );

    Ok(())
}

pub fn load_css_from_file(css_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let css_content = std::fs::read_to_string(css_path)?;
    load_custom_css(&css_content)
}

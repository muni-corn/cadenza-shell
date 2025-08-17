use gtk4::CssProvider;
use gdk4::Display;

const CSS: &str = r#"
.bar {
    background-color: rgba(30, 30, 46, 0.95);
    color: #cdd6f4;
    font-family: "JetBrains Mono", monospace;
    font-size: 14px;
    padding: 0 12px;
}

.bar-label {
    color: #89b4fa;
    font-weight: bold;
}

.tile {
    padding: 4px 8px;
    border-radius: 4px;
}

.tile.dim {
    opacity: 0.5;
    transition: opacity 0.3s ease;
}

.tile.bright {
    opacity: 1.0;
    transition: opacity 0.3s ease;
}

.icon {
    font-family: "Material Design Icons";
}

.workspaces button {
    min-width: 24px;
    min-height: 24px;
    margin: 4px;
    border-radius: 4px;
    background-color: rgba(69, 71, 90, 0.5);
}

.workspaces button.bright {
    background-color: #89b4fa;
    color: #1e1e2e;
}

progressbar {
    min-height: 4px;
}

progressbar trough {
    background-color: rgba(69, 71, 90, 0.5);
    border-radius: 2px;
}

progressbar progress {
    background-color: #89b4fa;
    border-radius: 2px;
}
"#;

pub fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_string(CSS);

    gtk4::style_context_add_provider_for_display(
        &Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
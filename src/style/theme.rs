use gdk4::Display;
use gtk4::CssProvider;

const DEFAULT_CSS: &str = r#"
"#;

pub fn load_css() -> Result<(), &'static str> {
    let provider = CssProvider::new();
    provider.load_from_string(DEFAULT_CSS);

    let display = Display::default().ok_or("could not connect to a display")?;

    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Ok(())
}

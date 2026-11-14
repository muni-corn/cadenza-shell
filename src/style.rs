use rsass::{
    compile_scss,
    output::{Format, Style},
};

pub fn compile_styles() -> Result<String, rsass::Error> {
    compile_scss(include_bytes!("style.scss"), Format {
        style: Style::Expanded,
        ..Default::default()
    })
    .map(|vec| String::from_utf8_lossy(&vec).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_compile() {
        compile_styles().expect("style.scss should compile without errors");
    }

    #[test]
    fn translucent_surfaces_use_theme_colors() {
        let css = compile_styles().unwrap();
        assert!(
            css.contains("alpha(@window_bg_color,0.75)"),
            "bar should be a translucent window_bg_color, got:\n{css}"
        );
    }
}

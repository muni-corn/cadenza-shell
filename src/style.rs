use rsass::{
    compile_scss,
    output::{Format, Style},
};

pub fn compile_styles() -> Result<String, rsass::Error> {
    compile_scss(
        include_bytes!("style.scss"),
        Format {
            style: Style::Expanded,
            ..Default::default()
        },
    )
    .map(|vec| String::from_utf8_lossy(&vec).into_owned())
}

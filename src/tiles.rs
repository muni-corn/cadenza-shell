// Relm4 tile implementations
pub mod battery;
pub mod bluetooth;
pub mod brightness;
pub mod clock;
pub mod mpris;
pub mod network;
pub mod niri;
pub mod notifications;
pub mod pulseaudio;
pub mod weather;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Attention {
    Alarm,
    Warning,
    Normal,
    Dim,
}

impl Attention {
    pub fn css_class(&self) -> &'static str {
        match self {
            Attention::Alarm => "alarm",
            Attention::Warning => "warning",
            Attention::Normal => "",
            Attention::Dim => "dim",
        }
    }
}

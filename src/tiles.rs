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
pub mod tray;
pub mod weather;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Attention {
    Alarm,
    Warning,
    #[default]
    Normal,
    Dim,
}

impl Attention {
    /// The css class for this attention level, if it has one.
    pub fn css_class(self) -> Option<&'static str> {
        match self {
            Attention::Alarm => Some("alarm"),
            Attention::Warning => Some("warning"),
            Attention::Normal => None,
            Attention::Dim => Some("dim"),
        }
    }

    /// Returns `base` followed by this attention level's class, if any.
    pub fn with_classes(self, base: &[&'static str]) -> Vec<&'static str> {
        base.iter().copied().chain(self.css_class()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_attention_adds_no_class() {
        assert_eq!(Attention::Normal.with_classes(&["icon"]), ["icon"]);
    }

    #[test]
    fn other_attention_levels_append_their_class() {
        assert_eq!(Attention::Dim.with_classes(&["icon"]), ["icon", "dim"]);
        assert_eq!(Attention::Alarm.with_classes(&[]), ["alarm"]);
    }
}

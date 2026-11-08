use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::notifications::types::NotificationUrgency;

/// The `hints` dictionary passed with a `Notify` call.
///
/// Clients send hints with all sorts of types (an int instead of a byte for
/// urgency, a string instead of a bool), so each hint is read leniently and
/// falls back to its default instead of rejecting the whole notification.
#[derive(Debug, Default)]
pub struct Hints(HashMap<String, OwnedValue>);

impl From<HashMap<String, OwnedValue>> for Hints {
    fn from(hints: HashMap<String, OwnedValue>) -> Self {
        Self(hints)
    }
}

impl Hints {
    /// The `urgency` hint, defaulting to normal.
    pub fn urgency(&self) -> NotificationUrgency {
        self.integer("urgency")
            .and_then(|level| u8::try_from(level).ok())
            .map(NotificationUrgency::from)
            .unwrap_or_default()
    }

    /// A string hint such as `desktop-entry`.
    pub fn string(&self, key: &str) -> Option<String> {
        match &**self.0.get(key)? {
            Value::Str(text) => Some(text.to_string()),
            _ => None,
        }
    }

    /// An integer hint of any integer type.
    fn integer(&self, key: &str) -> Option<i64> {
        match &**self.0.get(key)? {
            Value::U8(n) => Some(i64::from(*n)),
            Value::I16(n) => Some(i64::from(*n)),
            Value::U16(n) => Some(i64::from(*n)),
            Value::I32(n) => Some(i64::from(*n)),
            Value::U32(n) => Some(i64::from(*n)),
            Value::I64(n) => Some(*n),
            Value::U64(n) => i64::try_from(*n).ok(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(entries: &[(&str, Value<'_>)]) -> Hints {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.try_to_owned().unwrap()))
            .collect::<HashMap<_, _>>()
            .into()
    }

    #[test]
    fn reads_urgency_from_any_integer_type() {
        assert_eq!(
            hints(&[("urgency", Value::U8(2))]).urgency(),
            NotificationUrgency::Critical
        );
        assert_eq!(
            hints(&[("urgency", Value::I32(0))]).urgency(),
            NotificationUrgency::Low
        );
    }

    #[test]
    fn bad_urgency_falls_back_to_normal() {
        assert_eq!(
            hints(&[("urgency", Value::from("high"))]).urgency(),
            NotificationUrgency::Normal
        );
        assert_eq!(Hints::default().urgency(), NotificationUrgency::Normal);
    }

    #[test]
    fn reads_string_hints() {
        let hints = hints(&[("desktop-entry", Value::from("firefox"))]);
        assert_eq!(hints.string("desktop-entry").as_deref(), Some("firefox"));
        assert_eq!(hints.string("missing"), None);
    }
}

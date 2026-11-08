use std::fmt;

/// A wifi password that never shows up in debug output.
///
/// Messages and commands carrying passwords derive `Debug`, so wrapping the
/// password keeps a stray `{msg:?}` log from leaking it.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    /// Wraps a plaintext password.
    pub fn new(password: String) -> Self {
        Self(password)
    }

    /// Returns the plaintext password.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_hides_the_password() {
        let password = Password::new("hunter22".to_string());
        assert!(!format!("{password:?}").contains("hunter22"));
        assert_eq!(password.expose(), "hunter22");
    }
}

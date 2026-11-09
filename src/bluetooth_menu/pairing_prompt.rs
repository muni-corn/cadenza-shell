//! What the pairing prompt says and accepts for each kind of request.

use crate::bluetooth::PairingRequest;

/// How the pairing prompt looks for a request.
#[derive(Debug, PartialEq, Eq)]
pub struct PromptView {
    pub text: String,
    /// Whether the user types something (a PIN or passkey).
    pub show_entry: bool,
    /// Whether there's anything to confirm or cancel; display-only prompts
    /// just show a code until bluez is done with it.
    pub show_buttons: bool,
    /// The cancel and confirm button labels.
    pub button_labels: (&'static str, &'static str),
}

/// Describes the prompt for `request` from the device named `device`.
pub fn prompt_view(request: &PairingRequest, device: &str) -> PromptView {
    let (text, show_entry, show_buttons) = match request {
        PairingRequest::PinCode => (format!("Enter the PIN code for {device}"), true, true),
        PairingRequest::Passkey => (format!("Enter the passkey for {device}"), true, true),
        PairingRequest::ConfirmPasskey(passkey) => (
            format!("Does {device} show the passkey {passkey:06}?"),
            false,
            true,
        ),
        PairingRequest::Authorize => (format!("Allow pairing with {device}?"), false, true),
        PairingRequest::AuthorizeService(profile) => {
            (format!("Allow {device} to use {profile}?"), false, true)
        }
        PairingRequest::DisplayPinCode(pin) => (format!("Enter {pin} on {device}"), false, false),
        PairingRequest::DisplayPasskey { passkey, entered } => {
            let digits = if *entered == 1 { "digit" } else { "digits" };
            (
                format!("Enter {passkey:06} on {device} ({entered} {digits} entered)"),
                false,
                false,
            )
        }
    };

    // permission questions read better as allow or deny
    let button_labels = match request {
        PairingRequest::Authorize | PairingRequest::AuthorizeService(_) => ("Deny", "Allow"),
        _ => ("Cancel", "Confirm"),
    };

    PromptView {
        text,
        show_entry,
        show_buttons,
        button_labels,
    }
}

/// Validates a legacy PIN code, which bluez accepts as 1 to 16 characters.
pub fn parse_pin_code(text: &str) -> Result<String, &'static str> {
    if (1..=16).contains(&text.chars().count()) {
        Ok(text.to_string())
    } else {
        Err("PIN codes are 1 to 16 characters")
    }
}

/// Validates a numeric passkey, which is at most six digits.
pub fn parse_passkey(text: &str) -> Result<u32, &'static str> {
    text.trim()
        .parse::<u32>()
        .ok()
        .filter(|passkey| *passkey <= 999_999)
        .ok_or("Passkeys are numbers from 0 to 999999")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_prompts_name_the_profile() {
        let view = prompt_view(
            &PairingRequest::AuthorizeService("Audio Sink".into()),
            "Headphones",
        );
        assert_eq!(view.text, "Allow Headphones to use Audio Sink?");
        assert!(!view.show_entry && view.show_buttons);
        assert_eq!(view.button_labels, ("Deny", "Allow"));
    }

    #[test]
    fn display_prompts_have_no_buttons() {
        let view = prompt_view(&PairingRequest::DisplayPinCode("0000".into()), "Keyboard");
        assert_eq!(view.text, "Enter 0000 on Keyboard");
        assert!(!view.show_entry && !view.show_buttons);
    }

    #[test]
    fn passkey_prompts_pad_and_pluralize() {
        let view = prompt_view(
            &PairingRequest::DisplayPasskey {
                passkey: 42,
                entered: 1,
            },
            "Keyboard",
        );
        assert_eq!(view.text, "Enter 000042 on Keyboard (1 digit entered)");
    }

    #[test]
    fn pin_codes_are_1_to_16_characters() {
        assert!(parse_pin_code("").is_err());
        assert_eq!(parse_pin_code("0000"), Ok("0000".to_string()));
        assert!(parse_pin_code(&"1".repeat(17)).is_err());
    }

    #[test]
    fn passkeys_are_at_most_six_digits() {
        assert_eq!(parse_passkey(" 123456 "), Ok(123_456));
        assert!(parse_passkey("1000000").is_err());
        assert!(parse_passkey("abc").is_err());
    }
}

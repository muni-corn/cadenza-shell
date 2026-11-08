/// Raw `NM80211ApFlags` bits from an access point's `Flags` property.
mod ap_flags {
    /// Access point requires authentication and encryption (usually WEP).
    pub const PRIVACY: u32 = 0x1;
}

/// Raw `NM80211ApSecurityFlags` bits from an access point's `WpaFlags` and
/// `RsnFlags` properties.
mod ap_security_flags {
    pub const KEY_MGMT_PSK: u32 = 0x100;
    pub const KEY_MGMT_802_1X: u32 = 0x200;
    pub const KEY_MGMT_SAE: u32 = 0x400;
    pub const KEY_MGMT_EAP_SUITE_B_192: u32 = 0x2000;
}

/// The security scheme an access point requires, derived from its `Flags`,
/// `WpaFlags`, and `RsnFlags` properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApSecurity {
    /// No authentication required.
    Open,
    /// Legacy WEP (`Flags` has `PRIVACY` but no WPA/RSN key management).
    Wep,
    /// WPA/WPA2 pre-shared key. Password-only; supported by the inline
    /// connect flow.
    Psk,
    /// WPA3-only Simultaneous Authentication of Equals. Password-only;
    /// supported by the inline connect flow. Transition networks that also
    /// accept PSK are classified as [`ApSecurity::Psk`].
    Sae,
    /// 802.1X or WPA3-Enterprise. Requires more than a single password (a
    /// username, certificate, or RADIUS-backed exchange), which the inline
    /// connect flow does not support.
    Enterprise,
}

impl ApSecurity {
    /// Derives the security scheme from an access point's raw `Flags`,
    /// `WpaFlags`, and `RsnFlags` bitfields.
    pub fn from_flags(flags: u32, wpa_flags: u32, rsn_flags: u32) -> Self {
        use ap_security_flags::{
            KEY_MGMT_802_1X, KEY_MGMT_EAP_SUITE_B_192, KEY_MGMT_PSK, KEY_MGMT_SAE,
        };

        let combined = wpa_flags | rsn_flags;

        // wpa2/wpa3 transition networks advertise both psk and sae; psk works
        // on every card, while forcing sae fails on hardware without it
        if combined & KEY_MGMT_PSK != 0 {
            Self::Psk
        } else if combined & KEY_MGMT_SAE != 0 {
            Self::Sae
        } else if combined & (KEY_MGMT_802_1X | KEY_MGMT_EAP_SUITE_B_192) != 0 {
            Self::Enterprise
        } else if flags & ap_flags::PRIVACY != 0 {
            Self::Wep
        } else {
            Self::Open
        }
    }

    /// Whether this security scheme is unsecured.
    pub fn is_open(self) -> bool {
        self == Self::Open
    }

    /// Whether connecting requires a single password, which the inline
    /// connect flow (no secret agent) can satisfy.
    pub fn requires_password(self) -> bool {
        matches!(self, Self::Wep | Self::Psk | Self::Sae)
    }

    /// Checks that `password` is acceptable for this security scheme before
    /// handing it to NetworkManager, which would otherwise reject it with a
    /// generic error. Returns a user-facing explanation if it isn't.
    pub fn validate_password(self, password: &str) -> Result<(), &'static str> {
        match self {
            Self::Psk | Self::Sae => {
                let is_passphrase = (8..=63).contains(&password.len());
                let is_raw_key =
                    password.len() == 64 && password.chars().all(|c| c.is_ascii_hexdigit());
                if is_passphrase || is_raw_key {
                    Ok(())
                } else {
                    Err("Passwords for this network are 8 to 63 characters")
                }
            }
            Self::Wep if password.is_empty() => Err("Enter the network's password"),
            Self::Wep | Self::Open | Self::Enterprise => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_network_has_no_flags() {
        assert_eq!(ApSecurity::from_flags(0, 0, 0), ApSecurity::Open);
    }

    #[test]
    fn wep_network_has_privacy_flag_only() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, 0),
            ApSecurity::Wep
        );
    }

    #[test]
    fn wpa2_psk_is_detected_from_rsn_flags() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, ap_security_flags::KEY_MGMT_PSK),
            ApSecurity::Psk
        );
    }

    #[test]
    fn wpa1_psk_is_detected_from_wpa_flags() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, ap_security_flags::KEY_MGMT_PSK, 0),
            ApSecurity::Psk
        );
    }

    #[test]
    fn transition_mode_networks_use_psk() {
        assert_eq!(
            ApSecurity::from_flags(
                ap_flags::PRIVACY,
                ap_security_flags::KEY_MGMT_PSK,
                ap_security_flags::KEY_MGMT_SAE
            ),
            ApSecurity::Psk
        );
    }

    #[test]
    fn wpa3_only_networks_use_sae() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, ap_security_flags::KEY_MGMT_SAE),
            ApSecurity::Sae
        );
    }

    #[test]
    fn enterprise_8021x_is_detected() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, ap_security_flags::KEY_MGMT_802_1X),
            ApSecurity::Enterprise
        );
    }

    #[test]
    fn enterprise_suite_b_192_is_detected() {
        assert_eq!(
            ApSecurity::from_flags(
                ap_flags::PRIVACY,
                0,
                ap_security_flags::KEY_MGMT_EAP_SUITE_B_192
            ),
            ApSecurity::Enterprise
        );
    }

    #[test]
    fn wpa_passwords_must_be_8_to_63_characters() {
        assert!(ApSecurity::Psk.validate_password("short").is_err());
        assert!(ApSecurity::Psk.validate_password("long enough").is_ok());
        assert!(ApSecurity::Sae.validate_password(&"x".repeat(64)).is_err());
    }

    #[test]
    fn wpa_accepts_a_64_digit_hex_key() {
        assert!(ApSecurity::Psk.validate_password(&"ab".repeat(32)).is_ok());
    }

    #[test]
    fn wep_needs_a_non_empty_password() {
        assert!(ApSecurity::Wep.validate_password("").is_err());
        assert!(ApSecurity::Wep.validate_password("12345").is_ok());
    }

    #[test]
    fn requires_password_excludes_open_and_enterprise() {
        assert!(!ApSecurity::Open.requires_password());
        assert!(ApSecurity::Wep.requires_password());
        assert!(ApSecurity::Psk.requires_password());
        assert!(ApSecurity::Sae.requires_password());
        assert!(!ApSecurity::Enterprise.requires_password());
    }
}

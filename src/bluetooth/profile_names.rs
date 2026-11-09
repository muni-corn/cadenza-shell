//! Readable names for the profiles and services devices ask to use.

use bluer::{
    Uuid,
    id::{Service, ServiceClass},
};

/// Names the profile or service with this uuid, falling back to the uuid
/// itself for ones bluer doesn't know.
pub(super) fn profile_name(uuid: Uuid) -> String {
    if let Ok(class) = ServiceClass::try_from(uuid) {
        return class.to_string();
    }
    if let Ok(service) = Service::try_from(uuid) {
        return service.to_string();
    }
    uuid.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_profiles_are_named() {
        assert_eq!(profile_name(ServiceClass::AudioSink.into()), "Audio Sink");
    }

    #[test]
    fn unknown_profiles_fall_back_to_the_uuid() {
        let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        assert_eq!(profile_name(uuid), uuid.to_string());
    }
}

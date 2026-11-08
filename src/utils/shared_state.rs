use relm4::SharedState;

/// Writes `value` into `state` only if it differs from the current value.
///
/// relm4 notifies every subscriber whenever a write guard is dropped, even if
/// nothing changed, so skipping no-op writes avoids needless re-renders.
/// Returns whether the state was written.
///
/// # Example
/// ```ignore
/// static VOLUME: SharedState<u32> = SharedState::new();
/// assert!(set_if_changed(&VOLUME, 50));
/// assert!(!set_if_changed(&VOLUME, 50));
/// ```
pub fn set_if_changed<T: PartialEq + Default>(state: &SharedState<T>, value: T) -> bool {
    if *state.read() == value {
        return false;
    }

    *state.write() = value;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_only_when_the_value_changes() {
        static STATE: SharedState<Option<u32>> = SharedState::new();

        assert!(set_if_changed(&STATE, Some(1)));
        assert_eq!(*STATE.read(), Some(1));
        assert!(!set_if_changed(&STATE, Some(1)));
        assert!(set_if_changed(&STATE, None));
        assert_eq!(*STATE.read(), None);
    }
}

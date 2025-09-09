pub mod audio;
pub mod battery;
pub mod brightness;
pub mod hyprland;
pub mod network;
pub mod notifications;

pub trait Service {
    /// The struct that represents the current state of this service.
    type State;

    /// Creates an instance of this service, spawning a thread for its logic.
    fn launch() -> Self;

    /// Adds a callback to this service that will be called upon updates.
    fn with(self, callback: impl FnMut(Self::State) + Send + Sync + 'static) -> Self;
}

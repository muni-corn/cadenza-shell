use std::sync::Arc;

use tokio::sync::RwLock;

pub mod bluetooth;
pub mod brightness;
pub mod hyprland;
pub mod mpris;
pub mod network;
pub mod niri;
pub mod notifications;
pub mod pulseaudio;

/// A callback function that services can call. Takes the service's state as
/// input `I`.
pub trait Callback<I> = FnMut(I) + Send + Sync;

/// A service field that is guarded by an `Arc` and a `RwLock`.
pub type AsyncProp<T> = Arc<RwLock<T>>;

/// A `Vec` of `Callback`s.
pub type CallbackVec<I> = Vec<Box<dyn Callback<I>>>;

/// An `AsyncProp` of a `CallbackVec`.
pub type Callbacks<T> = AsyncProp<CallbackVec<T>>;

pub trait Service {
    /// The type that represents an event when the service is updated.
    type Event;

    /// Creates an instance of this service, spawning threads as needed for its
    /// logic.
    fn launch() -> Self;

    /// Adds a callback to this service that will be called upon updates.
    fn with(self, callback: impl Callback<Self::Event> + 'static) -> Self;
}

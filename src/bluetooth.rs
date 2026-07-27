pub mod agent;
pub mod commands;
pub mod service;
pub mod state;

// not called from the UI yet; wired up in a following commit
#[allow(unused_imports)]
pub use commands::{
    cancel_pairing, pair, pairing_reply, remove, set_trusted, start_discovery, stop_discovery,
};
pub use service::run_bluetooth_service;
// PAIRING_PROMPT/PairingPrompt/PairingRequest: not consumed outside
// bluetooth:: yet; wired up when bluetooth_menu renders the prompt
#[allow(unused_imports)]
pub use state::{
    BLUETOOTH_STATE, BluetoothState, DeviceInfo, PAIRING_PROMPT, PairingPrompt, PairingRequest,
};

pub mod agent;
pub mod commands;
pub mod service;
pub mod state;

pub use agent::PairingReply;
// set_trusted isn't driven by a UI control yet (successful pairing already
// auto-trusts server-side); kept private to bluetooth::commands for now
pub use commands::{cancel_pairing, pair, pairing_reply, remove, start_discovery, stop_discovery};
pub use service::run_bluetooth_service;
pub use state::{
    BLUETOOTH_STATE, BluetoothState, DeviceInfo, PAIRING_PROMPT, PairingPrompt, PairingRequest,
};

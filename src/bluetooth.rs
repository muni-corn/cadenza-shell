mod actions;
mod agent;
mod commands;
mod service;
mod state;

// pairing replies go straight to the agent, which only touches local state,
// rather than queueing behind slower work in the service loop
pub use agent::{PairingReply, cancel as cancel_pairing, respond as pairing_reply};
pub use commands::{
    connect, disconnect, pair, remove, set_powered, start_discovery, stop_discovery,
};
pub use service::run_bluetooth_service;
pub use state::{
    BLUETOOTH_STATE, BluetoothState, DeviceInfo, PAIRING_PROMPT, PairingPrompt, PairingRequest,
    get_icon,
};

pub mod agent;
pub mod service;
pub mod state;

pub use service::run_bluetooth_service;
// PAIRING_PROMPT/PairingPrompt/PairingRequest: not consumed outside
// bluetooth:: yet; wired up when bluetooth_menu renders the prompt
#[allow(unused_imports)]
pub use state::{
    BLUETOOTH_STATE, BluetoothState, DeviceInfo, PAIRING_PROMPT, PairingPrompt, PairingRequest,
};

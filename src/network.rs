pub mod commands;
pub mod dbus;
pub mod events;
pub mod scan;
pub mod service;
pub mod state;
pub mod types;
// not called from the UI yet; wired up in a following commit
#[allow(unused_imports)]
pub use commands::{connect, disconnect, forget, scan, set_wifi_enabled};
// subscribe_events isn't called from the UI yet; wired up in a following
// commit
#[allow(unused_imports)]
pub use events::{ConnectFailureReason, NetworkEvent, subscribe_events};
pub use service::run_network_service;
// WIFI_SCAN_STATE/WifiScanState: not consumed outside network:: yet; the
// network_menu rewrite subscribes to them in a following commit
#[allow(unused_imports)]
pub use state::{
    NETWORK_STATE, NetworkInfo, SpecificNetworkInfo, WIFI_SCAN_STATE, WifiScanState, get_icon,
};

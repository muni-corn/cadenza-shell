use std::{fs, path::PathBuf, sync::OnceLock};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CadenzaShellConfig {
    pub ui: UiConfig,
    pub bar: BarConfig,
    pub notifications: NotificationConfig,
    pub tiles: TileConfig,
    pub network: NetworkConfig,
    pub bluetooth: BluetoothConfig,
}

impl CadenzaShellConfig {
    /// Raises polling intervals to at least one second.
    ///
    /// `tokio::time::interval` panics on a zero period, so a zero in the
    /// config file would otherwise crash the services that poll.
    fn clamp_intervals(&mut self) {
        let intervals = [
            (
                "network.reconcile_interval",
                &mut self.network.reconcile_interval,
            ),
            ("network.scan_interval", &mut self.network.scan_interval),
            (
                "bluetooth.reconcile_interval",
                &mut self.bluetooth.reconcile_interval,
            ),
        ];

        for (name, value) in intervals {
            if *value == 0 {
                tracing::warn!("{name} can't be 0; using 1 second instead");
                *value = 1;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Overall UI scaling factor
    pub scale_factor: f64,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarPosition {
    Top,
    Bottom,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BarConfig {
    /// Bar height in pixels
    pub height: i32,
    /// Bar position (top, bottom)
    pub position: BarPosition,
    /// Spacing between tiles
    pub tile_spacing: i32,
    /// Margin from screen edges
    pub edge_padding: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    /// Maximum number of notifications to show
    pub max_notifications: usize,
    /// Auto-dismiss timeout in seconds (0 = no auto-dismiss)
    pub timeout: u64,
    /// Notification popup width
    pub popup_width: i32,
    /// Notification center width
    pub center_width: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TileConfig {
    /// Default tile icon size
    pub icon_size: i32,
    /// Show tile labels
    pub show_labels: bool,
    /// Maximum text width for tiles
    pub max_text_width: i32,
    /// Analog clock radius
    pub analog_clock_radius: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// How often (in seconds) to reconcile network state from NetworkManager
    /// even without an incoming property change event.
    pub reconcile_interval: u64,
    /// How often (in seconds) the network menu re-requests a wifi scan while
    /// it is open.
    pub scan_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BluetoothConfig {
    /// Whether cadenza-shell should register itself as BlueZ's default
    /// pairing agent. Leave this false to coexist with another agent (e.g.
    /// blueman); cadenza-shell still handles pairing prompts for pairings it
    /// initiates itself either way.
    pub default_agent: bool,
    /// How often (in seconds) to reconcile bluetooth state from BlueZ even
    /// without an incoming property change event.
    pub reconcile_interval: u64,
    /// How long (in seconds) a device discovery session runs before
    /// automatically stopping.
    pub discovery_timeout: u64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { scale_factor: 1.0 }
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            height: 32,
            position: BarPosition::Top,
            tile_spacing: 12,
            edge_padding: 8,
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            max_notifications: 10,
            timeout: 10,
            popup_width: 400,
            center_width: 400,
        }
    }
}

impl Default for TileConfig {
    fn default() -> Self {
        Self {
            icon_size: 16,
            show_labels: true,
            max_text_width: 30,
            analog_clock_radius: 60.0,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            reconcile_interval: 10,
            scan_interval: 15,
        }
    }
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            default_agent: false,
            reconcile_interval: 10,
            discovery_timeout: 30,
        }
    }
}

/// Global configuration, loaded once on first access.
static CONFIG: OnceLock<CadenzaShellConfig> = OnceLock::new();

/// Loads the configuration so later reads don't hit the disk.
pub fn init() {
    get_config();
}

/// Returns the global configuration, loading it on first use.
pub fn get_config() -> &'static CadenzaShellConfig {
    CONFIG.get_or_init(|| {
        let mut config = load().unwrap_or_else(|e| {
            tracing::error!("couldn't load configuration, using defaults: {e:#}");
            CadenzaShellConfig::default()
        });
        config.clamp_intervals();
        config
    })
}

/// Reads the config file, writing a default one first if none exists.
fn load() -> Result<CadenzaShellConfig> {
    let path = config_path();

    if path.exists() {
        let content = fs::read_to_string(&path)?;
        let config = serde_json::from_str(&content)
            .with_context(|| format!("couldn't parse {}", path.display()))?;
        tracing::info!("loaded configuration from: {}", path.display());
        return Ok(config);
    }

    let config = CadenzaShellConfig::default();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(&config)?)?;
    tracing::info!("created default configuration at: {}", path.display());

    Ok(config)
}

/// Returns `$XDG_CONFIG_HOME/cadenza-shell/config.json`.
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("./config"))
        .join("cadenza-shell")
        .join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_intervals_are_raised_to_one_second() {
        let mut config = CadenzaShellConfig::default();
        config.network.reconcile_interval = 0;
        config.network.scan_interval = 0;
        config.bluetooth.reconcile_interval = 0;

        config.clamp_intervals();

        assert_eq!(config.network.reconcile_interval, 1);
        assert_eq!(config.network.scan_interval, 1);
        assert_eq!(config.bluetooth.reconcile_interval, 1);
    }

    #[test]
    fn nonzero_intervals_are_kept() {
        let mut config = CadenzaShellConfig::default();
        config.network.scan_interval = 42;

        config.clamp_intervals();

        assert_eq!(config.network.scan_interval, 42);
    }
}

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MuseShellConfig {
    pub ui: UiConfig,
    pub bar: BarConfig,
    pub notifications: NotificationConfig,
    pub tiles: TileConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for UiConfig {
    fn default() -> Self {
        Self { scale_factor: 1.0 }
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            height: 32,
            position: "top".to_string(),
            spacing: 8,
            margin: 8,
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

#[derive(Debug)]
pub struct ConfigManager {
    config: MuseShellConfig,
    config_path: PathBuf,
}

impl ConfigManager {
    /// Create a new ConfigManager instance
    pub fn new() -> Result<Self> {
        let config_path = Self::get_config_path();
        let config = Self::load_config(&config_path)?;

        Ok(Self {
            config,
            config_path,
        })
    }

    /// load configuration from file, create default if doesn't exist
    pub fn load_config(path: &PathBuf) -> Result<MuseShellConfig> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: MuseShellConfig = serde_json::from_str(&content)?;
            log::info!("loaded configuration from: {}", path.display());
            Ok(config)
        } else {
            let default_config = MuseShellConfig::default();

            // create config directory if it doesn't exist
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            // write default config
            let content = serde_json::to_string_pretty(&default_config)?;
            fs::write(path, content)?;
            log::info!("created default configuration at: {}", path.display());

            Ok(default_config)
        }
    }

    /// Get the configuration file path
    pub fn get_config_path() -> PathBuf {
        // Use XDG config directory or fallback to ~/.config
        let config_dir = if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg_config)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config")
        } else {
            PathBuf::from("./config") // fallback for testing
        };

        config_dir.join("muse-shell").join("config.json")
    }

    /// Get the current configuration
    pub fn config(&self) -> &MuseShellConfig {
        &self.config
    }

    /// Update configuration and save to file
    pub fn update_config(&mut self, config: MuseShellConfig) -> Result<()> {
        self.config = config;
        self.save()?;
        Ok(())
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.config_path, content)?;
        log::info!("saved configuration to: {}", self.config_path.display());
        Ok(())
    }

    /// Reload configuration from file
    pub fn reload(&mut self) -> Result<()> {
        self.config = Self::load_config(&self.config_path)?;
        log::info!("reloaded configuration from file");
        Ok(())
    }

    /// Reset configuration to defaults
    pub fn reset_to_defaults(&mut self) -> Result<()> {
        self.config = MuseShellConfig::default();
        self.save()?;
        log::info!("reset configuration to defaults");
        Ok(())
    }
}

use std::sync::{Mutex, OnceLock};

/// Global configuration instance
static CONFIG: OnceLock<Mutex<MuseShellConfig>> = OnceLock::new();

/// Initialize the global configuration manager
pub fn init() -> Result<()> {
    let config = match ConfigManager::new() {
        Ok(manager) => manager.config,
        Err(e) => {
            log::error!("failed to load configuration: {}", e);
            log::info!("using default configuration");
            MuseShellConfig::default()
        }
    };

    CONFIG
        .set(Mutex::new(config))
        .map_err(|_| anyhow::anyhow!("configuration already initialized"))?;

    Ok(())
}

/// Get a copy of the current configuration
pub fn get_config() -> MuseShellConfig {
    CONFIG
        .get()
        .and_then(|config| config.lock().ok())
        .map(|config| config.clone())
        .unwrap_or_default()
}

/// Update the global configuration
pub fn update_config(new_config: MuseShellConfig) -> Result<()> {
    if let Some(config_mutex) = CONFIG.get() {
        if let Ok(mut config) = config_mutex.lock() {
            *config = new_config.clone();

            // Also save to file
            let config_path = ConfigManager::get_config_path();
            let content = serde_json::to_string_pretty(&new_config)?;
            fs::write(&config_path, content)?;
            log::info!("updated and saved configuration");
        }
    }
    Ok(())
}

/// Reload the global configuration from file
pub fn reload_config() -> Result<()> {
    let config_path = ConfigManager::get_config_path();
    let reloaded_config = ConfigManager::load_config(&config_path)?;

    if let Some(config_mutex) = CONFIG.get() {
        if let Ok(mut config) = config_mutex.lock() {
            *config = reloaded_config;
            log::info!("reloaded configuration from file");
        }
    }
    Ok(())
}

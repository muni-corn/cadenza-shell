use std::process::Command;

use anyhow::Result;
use gtk4::glib;

mod imp {
    use std::{
        cell::{Cell, RefCell},
        process::Command,
    };

    use anyhow::Result;
    use gtk4::{glib, prelude::*, subclass::prelude::*};

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::AudioService)]
    pub struct AudioService {
        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        volume: Cell<f64>,

        #[property(get, set)]
        muted: Cell<bool>,

        #[property(get, set)]
        available: Cell<bool>,

        sink_name: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AudioService {
        type ParentType = glib::Object;
        type Type = super::AudioService;

        const NAME: &'static str = "MuseShellAudioService";
    }

    #[glib::derived_properties]
    impl ObjectImpl for AudioService {
        fn constructed(&self) {
            self.parent_constructed();

            // Initialize audio monitoring
            if let Ok(sink) = self.detect_default_sink() {
                self.sink_name.replace(sink);
                self.available.set(true);

                // Start monitoring
                self.start_monitoring();

                // Initial state update
                if let Ok((volume, muted)) = self.read_current_state() {
                    self.volume.set(volume);
                    self.muted.set(muted);
                }
            } else {
                log::warn!("No audio sink detected, audio service unavailable");
                self.available.set(false);
            }
        }
    }

    impl AudioService {
        fn detect_default_sink(&self) -> Result<String> {
            // Try to get default sink using pactl
            let output = Command::new("pactl").args(["get-default-sink"]).output()?;

            if output.status.success() {
                let sink = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !sink.is_empty() {
                    return Ok(sink);
                }
            }

            // Fallback: try to find any available sink
            let output = Command::new("pactl")
                .args(["list", "short", "sinks"])
                .output()?;

            if output.status.success() {
                let sinks = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = sinks.lines().next()
                    && let Some(sink_name) = first_line.split_whitespace().nth(1)
                {
                    return Ok(sink_name.to_string());
                }
            }

            anyhow::bail!("No audio sink found")
        }

        fn read_current_state(&self) -> Result<(f64, bool)> {
            // Get sink info using pactl
            let output = Command::new("pactl")
                .args(["get-sink-volume", "@DEFAULT_SINK@"])
                .output()?;

            let volume = if output.status.success() {
                let volume_str = String::from_utf8_lossy(&output.stdout);
                // Parse volume percentage from output like "Volume: front-left: 65536 /  100% /
                // 0.00 dB"
                if let Some(percent_pos) = volume_str.find('%') {
                    let before_percent = &volume_str[..percent_pos];
                    if let Some(last_space) = before_percent.rfind(' ') {
                        let percent_str = &before_percent[last_space + 1..];
                        percent_str.parse::<f64>().unwrap_or(0.0) / 100.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            } else {
                0.0
            };

            // Get mute status
            let mute_output = Command::new("pactl")
                .args(["get-sink-mute", "@DEFAULT_SINK@"])
                .output()?;

            let muted = if mute_output.status.success() {
                let mute_str = String::from_utf8_lossy(&mute_output.stdout);
                mute_str.contains("yes")
            } else {
                false
            };

            Ok((volume, muted))
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();

            // Monitor audio changes every 500ms
            glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                if let Ok((volume, muted)) = obj.imp().read_current_state() {
                    // Only update if values changed to avoid unnecessary signals
                    if (obj.volume() - volume).abs() > 0.01 {
                        obj.set_volume(volume);
                    }
                    if obj.muted() != muted {
                        obj.set_muted(muted);
                    }
                }
                glib::ControlFlow::Continue
            });
        }
    }
}

glib::wrapper! {
    pub struct AudioService(ObjectSubclass<imp::AudioService>);
}

impl Default for AudioService {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_volume_value(&self, volume: f64) -> Result<()> {
        let clamped = volume.clamp(0.0, 1.0);
        let percentage = (clamped * 100.0) as u32;

        // Set volume using pactl
        let output = Command::new("pactl")
            .args([
                "set-sink-volume",
                "@DEFAULT_SINK@",
                &format!("{}%", percentage),
            ])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to set volume: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // The property will be updated by the monitoring loop
        Ok(())
    }

    pub fn toggle_mute(&self) -> Result<()> {
        // Toggle mute using pactl
        let output = Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to toggle mute: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // The property will be updated by the monitoring loop
        Ok(())
    }

    pub fn set_mute_state(&self, muted: bool) -> Result<()> {
        let mute_arg = if muted { "1" } else { "0" };

        // Set mute state using pactl
        let output = Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", mute_arg])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to set mute state: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // The property will be updated by the monitoring loop
        Ok(())
    }
}

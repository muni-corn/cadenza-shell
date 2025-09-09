use std::{sync::Arc, time::Duration};

use systemstat::{Platform, System};
use tokio::sync::RwLock;

use crate::services::Service;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BatteryState {
    pub percentage: f64,
    pub charging: bool,
    pub time_remaining: Duration,
}

impl BatteryState {
    pub fn is_low(&self) -> bool {
        let time_remaining_secs = self.time_remaining.as_secs();

        (self.percentage <= 0.2 || time_remaining_secs <= 3600) && !self.charging
    }

    pub fn is_critical(&self) -> bool {
        let time_remaining_secs = self.time_remaining.as_secs();

        (self.percentage <= 0.1 || time_remaining_secs <= 1800) && !self.charging
    }

    pub fn get_readable_time(&self) -> String {
        use chrono::Local;

        if self.charging && self.percentage > 0.99 {
            "Plugged in".to_string()
        } else {
            let time_remaining = self.time_remaining.as_secs();
            if time_remaining < 30 * 60 {
                format!("{} min left", time_remaining / 60)
            } else {
                // calculate actual completion time
                let now = Local::now();
                let completion_time = now + chrono::Duration::seconds(time_remaining as i64);

                // format as "h:mm am/pm"
                let formatted = completion_time.format("%-I:%M %P").to_string();

                if self.charging {
                    format!("Full at {}", formatted)
                } else {
                    format!("Until {}", formatted)
                }
            }
        }
    }
}

type CallbackVec = Vec<Box<dyn FnMut(Option<BatteryState>) + Send + Sync>>;
type CallbackCollection = Arc<RwLock<CallbackVec>>;

pub struct BatteryService {
    state: Arc<RwLock<Option<BatteryState>>>,
    system: Arc<RwLock<System>>,
    callbacks: CallbackCollection,
}

impl BatteryService {
    async fn read_battery_state(system_arc: &Arc<RwLock<System>>) -> Option<BatteryState> {
        let battery_life = system_arc
            .read()
            .await
            .battery_life()
            .map_err(|e| log::error!("error getting battery state: {}", e))
            .ok()?;

        // get percentage (0.0 to 1.0)
        let percentage = battery_life.remaining_capacity as f64;

        // get time remaining
        let time_remaining = battery_life.remaining_time;

        let charging = system_arc.read().await.on_ac_power().ok().unwrap_or(false);

        Some(BatteryState {
            percentage,
            charging,
            time_remaining,
        })
    }
}

impl Service for BatteryService {
    type State = Option<BatteryState>;

    fn launch() -> Self {
        let service = Self {
            state: Arc::new(RwLock::new(None)),
            system: Arc::new(RwLock::new(System::new())),
            callbacks: Arc::new(RwLock::new(Vec::new())),
        };

        // main loop
        let state = Arc::clone(&service.state);
        let system_arc = Arc::clone(&service.system);
        let callbacks_arc = Arc::clone(&service.callbacks);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;

                let new_state = Self::read_battery_state(&system_arc).await;
                let changed = { new_state != *state.read().await };
                if changed {
                    for callback in &mut *callbacks_arc.write().await {
                        callback(new_state)
                    }
                }
            }
        });

        service
    }

    fn with(self, callback: impl FnMut(Self::State) + Send + Sync + 'static) -> Self {
        let callbacks_clone = Arc::clone(&self.callbacks);

        // this is probably extremely grotesque, but heck it we ball
        // (unfortunately, using blocking_write causes a panic)
        tokio::spawn(async move {
            callbacks_clone.write().await.push(Box::new(callback));
        });

        self
    }
}

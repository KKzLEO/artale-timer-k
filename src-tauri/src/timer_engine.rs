use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use uuid::Uuid;

use crate::boss_config::TimerDef;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TimerState {
    Running,
    Warning,
    Expired,
}

#[derive(Debug, Clone, Serialize)]
pub struct Timer {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub duration: f64,
    pub remaining: f64,
    pub state: TimerState,
    pub color: String,
    pub chain_to: Option<String>,
    pub warning_secs: f64,
    pub def_id: String,
    #[serde(skip)]
    pub warning_played: bool,
    #[serde(skip)]
    pub repeat: bool,
    /// "boss" or "buff" - distinguishes timer type for frontend rendering and sound
    #[serde(default)]
    pub timer_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimerUpdate {
    pub timers: Vec<Timer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimerExpiredEvent {
    pub id: String,
    pub name: String,
    pub chain_to: Option<String>,
    pub timer_type: String,
    #[serde(skip)]
    pub def_id: String,
    #[serde(skip)]
    pub repeat: bool,
}

pub struct TimerEngine {
    timers: Arc<Mutex<HashMap<String, Timer>>>,
    timer_defs: Arc<Mutex<HashMap<String, TimerDef>>>,
    buff_defs: Arc<Mutex<HashMap<String, TimerDef>>>,
    muted_defs: Arc<Mutex<HashSet<String>>>,
}

impl TimerEngine {
    pub fn new() -> Self {
        Self {
            timers: Arc::new(Mutex::new(HashMap::new())),
            timer_defs: Arc::new(Mutex::new(HashMap::new())),
            buff_defs: Arc::new(Mutex::new(HashMap::new())),
            muted_defs: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub async fn set_muted_defs(&self, defs: HashSet<String>) {
        let mut muted = self.muted_defs.lock().await;
        *muted = defs;
    }

    pub async fn is_muted(&self, def_id: &str) -> bool {
        let muted = self.muted_defs.lock().await;
        muted.contains(def_id)
    }

    pub async fn load_timer_defs(&self, defs: Vec<TimerDef>) {
        let mut timer_defs = self.timer_defs.lock().await;
        timer_defs.clear();
        for def in defs {
            timer_defs.insert(def.id.clone(), def);
        }
    }

    pub async fn load_buff_defs(&self, defs: Vec<TimerDef>) {
        let mut buff_defs = self.buff_defs.lock().await;
        buff_defs.clear();
        for def in defs {
            buff_defs.insert(def.id.clone(), def);
        }
    }

    pub async fn start_timer(&self, timer_def_id: &str) -> Result<String, String> {
        let defs = self.timer_defs.lock().await;
        let buff_defs = self.buff_defs.lock().await;
        let def = defs
            .get(timer_def_id)
            .or_else(|| buff_defs.get(timer_def_id))
            .ok_or_else(|| format!("Timer definition '{}' not found", timer_def_id))?
            .clone();
        drop(defs);
        drop(buff_defs);

        let id = Uuid::new_v4().to_string();
        let timer = Timer {
            id: id.clone(),
            name: def.name.clone(),
            icon: def.icon.clone(),
            duration: def.duration_secs,
            remaining: def.duration_secs,
            state: TimerState::Running,
            color: def.color.clone(),
            chain_to: def.chain_to.clone(),
            warning_secs: def.warning_secs,
            def_id: timer_def_id.to_string(),
            warning_played: false,
            repeat: def.repeat,
            timer_type: def.timer_type.clone().unwrap_or_else(|| "boss".to_string()),
        };

        let mut timers = self.timers.lock().await;
        timers.insert(id.clone(), timer);
        Ok(id)
    }

    pub async fn stop_timer(&self, timer_id: &str) -> bool {
        let mut timers = self.timers.lock().await;
        timers.remove(timer_id).is_some()
    }

    pub async fn stop_all(&self) {
        let mut timers = self.timers.lock().await;
        timers.clear();
    }

    pub async fn stop_by_def_id(&self, def_id: &str) {
        let mut timers = self.timers.lock().await;
        let defs = self.timer_defs.lock().await;
        let buff_defs = self.buff_defs.lock().await;
        let def = defs.get(def_id).or_else(|| buff_defs.get(def_id));
        if let Some(def) = def {
            let name = &def.name;
            timers.retain(|_, t| &t.name != name);
        }
    }

    /// Remove expired timers matching a specific def_id (used for buff timer reset).
    pub async fn stop_expired_by_def_id(&self, def_id: &str) {
        let mut timers = self.timers.lock().await;
        timers.retain(|_, t| !(t.def_id == def_id && t.state == TimerState::Expired));
    }

    /// Tick all timers by delta_secs. Returns timer update, expired events, and def_ids that triggered warnings.
    pub async fn tick(&self, delta_secs: f64) -> (TimerUpdate, Vec<TimerExpiredEvent>, Vec<String>) {
        let mut timers = self.timers.lock().await;
        let mut expired_events = Vec::new();
        let mut warning_def_ids = Vec::new();

        for timer in timers.values_mut() {
            if timer.state == TimerState::Expired {
                continue;
            }
            timer.remaining -= delta_secs;
            if timer.remaining <= 0.0 {
                timer.remaining = 0.0;
                timer.state = TimerState::Expired;
                expired_events.push(TimerExpiredEvent {
                    id: timer.id.clone(),
                    name: timer.name.clone(),
                    chain_to: timer.chain_to.clone(),
                    timer_type: timer.timer_type.clone(),
                    def_id: timer.def_id.clone(),
                    repeat: timer.repeat,
                });
            } else if timer.remaining <= timer.warning_secs {
                if timer.state != TimerState::Warning {
                    warning_def_ids.push(timer.def_id.clone());
                }
                timer.state = TimerState::Warning;
                if !timer.warning_played {
                    timer.warning_played = true;
                    if !warning_def_ids.contains(&timer.def_id) {
                        warning_def_ids.push(timer.def_id.clone());
                    }
                }
            }
        }

        let update = TimerUpdate {
            timers: timers.values().cloned().collect(),
        };

        (update, expired_events, warning_def_ids)
    }

    /// Remove expired timers that have been expired for longer than linger_secs.
    /// Buff timers are kept in expired state (not cleaned up) until user resets them.
    pub async fn cleanup_expired(&self, linger_secs: f64) {
        let mut timers = self.timers.lock().await;
        timers.retain(|_, t| {
            if t.state == TimerState::Expired {
                // Buff timers stay in expired state indefinitely
                if t.timer_type == "buff" {
                    return true;
                }
                // remaining is negative after continued ticking past 0
                // we use the absolute remaining as time since expiry
                t.remaining > -linger_secs
            } else {
                true
            }
        });
    }

    pub async fn get_timers(&self) -> Vec<Timer> {
        let timers = self.timers.lock().await;
        timers.values().cloned().collect()
    }

    pub async fn has_running_timers_for_def(&self, def_id: &str) -> bool {
        let timers = self.timers.lock().await;
        let defs = self.timer_defs.lock().await;
        let buff_defs = self.buff_defs.lock().await;
        let def = defs.get(def_id).or_else(|| buff_defs.get(def_id));
        if let Some(def) = def {
            timers
                .values()
                .any(|t| t.name == def.name && t.state != TimerState::Expired)
        } else {
            false
        }
    }
}

/// Load buff defs synchronously (used during app setup before async runtime).
pub fn load_buff_defs_sync(
    engine: &Arc<TimerEngine>,
    defs: &[TimerDef],
    app_handle: &tauri::AppHandle,
) {
    let engine = engine.clone();
    let defs: Vec<TimerDef> = defs.to_vec();
    let _ = app_handle;
    tauri::async_runtime::block_on(async {
        engine.load_buff_defs(defs).await;
    });
}

pub fn start_tick_loop(
    engine: Arc<TimerEngine>,
    app_handle: tauri::AppHandle,
) {
    let tick_interval_ms = 100;
    let delta = tick_interval_ms as f64 / 1000.0;
    let expired_linger_secs = 3.0;

    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(Duration::from_millis(tick_interval_ms));
        loop {
            ticker.tick().await;

            let (update, expired_events, warning_def_ids) = engine.tick(delta).await;

            // Emit timer update to frontend
            let _ = app_handle.emit("timer-update", &update);

            // Play warning sound for non-muted timers
            let mut boss_warning_played = false;
            let mut buff_warning_played = false;
            for def_id in &warning_def_ids {
                if !engine.is_muted(def_id).await {
                    let is_buff = def_id.starts_with("buff_");
                    if is_buff && !buff_warning_played {
                        crate::sound::play_buff_warning_beep();
                        buff_warning_played = true;
                    } else if !is_buff && !boss_warning_played {
                        crate::sound::play_warning_beep();
                        boss_warning_played = true;
                    }
                }
            }

            // Emit expired events and play sound for non-muted timers
            for event in &expired_events {
                let _ = app_handle.emit("timer-expired", event);
                if !engine.is_muted(&event.def_id).await {
                    if event.timer_type == "buff" {
                        crate::sound::play_buff_expired_beep();
                    } else {
                        crate::sound::play_expired_beep();
                    }
                }
            }

            // Handle chain triggers and auto-repeat
            for event in expired_events {
                if event.chain_to.is_some() || event.repeat {
                    // Remove the expired timer immediately before restarting
                    engine.stop_timer(&event.id).await;
                }
                if let Some(chain_to) = event.chain_to {
                    let _ = engine.start_timer(&chain_to).await;
                } else if event.repeat {
                    let _ = engine.start_timer(&event.def_id).await;
                }
            }

            // Cleanup expired timers after linger period
            engine.cleanup_expired(expired_linger_secs).await;
        }
    });
}

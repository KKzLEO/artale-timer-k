use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_back_hotkey")]
    pub back_hotkey: String,

    #[serde(default = "default_stop_all_hotkey")]
    pub stop_all_hotkey: String,

    /// Per-boss hotkey overrides: boss_id -> (timer_id -> hotkey)
    #[serde(default)]
    pub hotkeys: HashMap<String, HashMap<String, String>>,

    /// Per-boss hidden timer IDs: boss_id -> list of timer IDs
    #[serde(default)]
    pub hidden_timers: HashMap<String, Vec<String>>,

    /// Per-boss muted timer IDs: boss_id -> list of timer IDs
    #[serde(default)]
    pub muted_timers: HashMap<String, Vec<String>>,

    /// Global mini mode toggle
    #[serde(default)]
    pub mini_mode: bool,

    #[serde(default = "default_font_scale")]
    pub font_scale: f64,

    #[serde(default = "default_icon_scale")]
    pub icon_scale: f64,

    #[serde(default = "default_bg_opacity")]
    pub bg_opacity: f64,

    #[serde(default = "default_pause_hotkey")]
    pub pause_hotkey: String,
}

fn default_back_hotkey() -> String {
    "Alt+Home".to_string()
}

fn default_stop_all_hotkey() -> String {
    "Ctrl+0".to_string()
}

fn default_font_scale() -> f64 {
    1.0
}

fn default_icon_scale() -> f64 {
    1.25
}

fn default_bg_opacity() -> f64 {
    1.0
}

fn default_pause_hotkey() -> String {
    "Ctrl+Backquote".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            back_hotkey: default_back_hotkey(),
            stop_all_hotkey: default_stop_all_hotkey(),
            hotkeys: HashMap::new(),
            hidden_timers: HashMap::new(),
            muted_timers: HashMap::new(),
            mini_mode: false,
            font_scale: default_font_scale(),
            icon_scale: default_icon_scale(),
            bg_opacity: default_bg_opacity(),
            pause_hotkey: default_pause_hotkey(),
        }
    }
}

pub fn load_settings(path: &Path) -> AppSettings {
    if path.exists() {
        match fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => AppSettings::default(),
        }
    } else {
        AppSettings::default()
    }
}

pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let content = toml::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    fs::write(path, content)
        .map_err(|e| format!("Failed to write settings: {}", e))?;
    Ok(())
}

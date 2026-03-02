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
}

fn default_back_hotkey() -> String {
    "Alt+Home".to_string()
}

fn default_stop_all_hotkey() -> String {
    "Ctrl+0".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            back_hotkey: default_back_hotkey(),
            stop_all_hotkey: default_stop_all_hotkey(),
            hotkeys: HashMap::new(),
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

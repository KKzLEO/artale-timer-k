use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossConfig {
    pub boss: BossInfo,
    #[serde(default)]
    pub timers: Vec<TimerDef>,
    #[serde(default)]
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BossInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDef {
    pub id: String,
    pub name: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    pub duration_secs: f64,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default)]
    pub chain_to: Option<String>,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_warning_secs")]
    pub warning_secs: f64,
    #[serde(default)]
    pub repeat: bool,
}

fn default_icon() -> String {
    "⏱".to_string()
}

fn default_color() -> String {
    "#00FF00".to_string()
}

fn default_warning_secs() -> f64 {
    3.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_opacity")]
    pub opacity: f64,
}

fn default_opacity() -> f64 {
    0.85
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            opacity: default_opacity(),
        }
    }
}

pub fn load_boss_config(path: &Path) -> Result<BossConfig, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let config: BossConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &BossConfig) -> Result<(), String> {
    if config.boss.name.is_empty() {
        return Err("Boss name cannot be empty".to_string());
    }

    let timer_ids: HashMap<&str, usize> = config
        .timers
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();

    for timer in &config.timers {
        if timer.duration_secs <= 0.0 {
            return Err(format!(
                "Timer '{}' has invalid duration: {}",
                timer.id, timer.duration_secs
            ));
        }
        if let Some(ref chain) = timer.chain_to {
            if !timer_ids.contains_key(chain.as_str()) {
                return Err(format!(
                    "Timer '{}' chains to unknown timer '{}'",
                    timer.id, chain
                ));
            }
        }
    }
    Ok(())
}

pub fn load_all_bosses(dir: &Path) -> Vec<(String, BossConfig)> {
    let mut bosses = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "toml") {
                match load_boss_config(&path) {
                    Ok(config) => {
                        let file_stem = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        bosses.push((file_stem, config));
                    }
                    Err(e) => {
                        eprintln!("Warning: {}", e);
                    }
                }
            }
        }
    }
    bosses.sort_by(|a, b| a.0.cmp(&b.0));
    bosses
}

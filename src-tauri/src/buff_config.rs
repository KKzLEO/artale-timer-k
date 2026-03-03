use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuffItem {
    pub id: String,
    pub name: String,
    pub duration_secs: u32,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuffConfig {
    #[serde(default)]
    pub buffs: Vec<BuffItem>,
}

pub fn load_buffs(path: &Path) -> BuffConfig {
    if path.exists() {
        match fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<BuffConfig>(&content) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Warning: Failed to parse buffs.toml: {}", e);
                    BuffConfig::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: Failed to read buffs.toml: {}", e);
                BuffConfig::default()
            }
        }
    } else {
        // Create empty buffs.toml
        let config = BuffConfig::default();
        let _ = save_buffs(path, &config);
        config
    }
}

pub fn save_buffs(path: &Path, config: &BuffConfig) -> Result<(), String> {
    let content = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize buffs: {}", e))?;
    fs::write(path, content)
        .map_err(|e| format!("Failed to write buffs.toml: {}", e))?;
    Ok(())
}

mod boss_config;
mod buff_config;
pub mod key_listener;
pub mod settings;
pub mod shortcuts;
pub mod sound;
mod timer_engine;
mod tray;

use boss_config::{load_all_bosses, BossConfig};
use buff_config::{BuffConfig, BuffItem};
use settings::AppSettings;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use timer_engine::{Timer, TimerEngine};
use tokio::sync::Mutex;

pub struct AppState {
    pub timer_engine: Arc<TimerEngine>,
    pub bosses: Arc<Mutex<Vec<(String, BossConfig)>>>,
    pub active_boss: Arc<Mutex<Option<String>>>,
    pub bosses_dir: PathBuf,
    pub settings: Arc<Mutex<AppSettings>>,
    pub settings_path: PathBuf,
    pub buffs: Arc<Mutex<BuffConfig>>,
    pub buffs_path: PathBuf,
    pub key_listener: key_listener::KeyListener,
    pub shortcuts_paused: Arc<Mutex<bool>>,
}

fn get_bosses_dir(app: &AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");
    data_dir.join("bosses")
}

fn get_settings_path(app: &AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");
    data_dir.join("settings.toml")
}

fn get_buffs_path(app: &AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");
    data_dir.join("buffs.toml")
}

fn ensure_default_bosses(bosses_dir: &PathBuf) {
    fs::create_dir_all(bosses_dir).ok();

    // Remove legacy boss configs
    for old in &["zakum.toml", "horntail.toml"] {
        let p = bosses_dir.join(old);
        if p.exists() {
            fs::remove_file(&p).ok();
        }
    }

    // Always overwrite built-in boss configs with latest defaults
    let ccq = include_str!("../resources/bosses/chaos_crimson_queen.toml");
    fs::write(bosses_dir.join("chaos_crimson_queen.toml"), ccq).ok();

    let lotus = include_str!("../resources/bosses/hard_lotus.toml");
    fs::write(bosses_dir.join("hard_lotus.toml"), lotus).ok();
}

/// Re-register shortcuts for the currently active boss, filtering hidden timers.
/// Also re-registers buff shortcuts (unless monitoring is active).
pub async fn re_register_shortcuts_for_active_boss(
    app: &AppHandle,
    boss_id: &str,
    config: &BossConfig,
    settings: &AppSettings,
) {
    let state = app.state::<AppState>();

    // If monitoring is active, only register non-buff shortcuts
    if state.key_listener.is_running() {
        shortcuts::register_shortcuts_without_buffs(
            app,
            Some(boss_id),
            Some(config),
            settings,
        );
    } else {
        let buffs = state.buffs.lock().await.clone();

        shortcuts::register_all_shortcuts(
            app,
            Some(boss_id),
            Some(config),
            settings,
            &buffs.buffs,
        );
    }
}

#[tauri::command]
async fn list_bosses(state: State<'_, AppState>) -> Result<Vec<BossListItem>, String> {
    let bosses = state.bosses.lock().await;
    Ok(bosses
        .iter()
        .map(|(id, config)| BossListItem {
            id: id.clone(),
            name: config.boss.name.clone(),
            description: config.boss.description.clone(),
            timer_count: config.timers.len(),
        })
        .collect())
}

#[derive(serde::Serialize)]
struct BossListItem {
    id: String,
    name: String,
    description: String,
    timer_count: usize,
}

#[derive(serde::Serialize)]
struct SelectBossResponse {
    config: BossConfig,
    hidden_timers: Vec<String>,
    muted_timers: Vec<String>,
    mini_mode: bool,
    timer_order: Vec<String>,
}

#[tauri::command]
async fn select_boss(
    boss_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SelectBossResponse, String> {
    let bosses = state.bosses.lock().await;
    let (_, config) = bosses
        .iter()
        .find(|(id, _)| id == &boss_id)
        .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;
    let config = config.clone();
    drop(bosses);

    // Stop all running timers
    state.timer_engine.stop_all().await;

    // Get settings
    let settings = state.settings.lock().await;
    let hidden = settings
        .hidden_timers
        .get(&boss_id)
        .cloned()
        .unwrap_or_default();
    let muted = settings
        .muted_timers
        .get(&boss_id)
        .cloned()
        .unwrap_or_default();
    let mini_mode = settings.mini_mode;
    let timer_order = settings
        .timer_orders
        .get(&boss_id)
        .cloned()
        .unwrap_or_default();

    // Load only non-hidden timer defs
    let visible_timers: Vec<_> = config
        .timers
        .iter()
        .filter(|t| !hidden.contains(&t.id))
        .cloned()
        .collect();

    state
        .timer_engine
        .load_timer_defs(visible_timers)
        .await;

    // Load muted defs into engine
    state
        .timer_engine
        .set_muted_defs(muted.iter().cloned().collect::<HashSet<_>>())
        .await;

    // Register shortcuts
    re_register_shortcuts_for_active_boss(&app, &boss_id, &config, &settings).await;
    drop(settings);

    // Set active boss
    let mut active = state.active_boss.lock().await;
    *active = Some(boss_id);

    Ok(SelectBossResponse {
        config,
        hidden_timers: hidden,
        muted_timers: muted,
        mini_mode,
        timer_order,
    })
}

#[tauri::command]
async fn hide_timer(
    boss_id: String,
    timer_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().await;

    let hidden = settings.hidden_timers.entry(boss_id.clone()).or_default();
    if !hidden.contains(&timer_id) {
        hidden.push(timer_id);
    }

    settings::save_settings(&state.settings_path, &settings)?;

    // Re-register shortcuts if this is the active boss
    let active = state.active_boss.lock().await;
    if active.as_deref() == Some(&boss_id) {
        let bosses = state.bosses.lock().await;
        if let Some((_, config)) = bosses.iter().find(|(id, _)| id == &boss_id) {
            let visible_timers: Vec<_> = config
                .timers
                .iter()
                .filter(|t| {
                    !settings
                        .hidden_timers
                        .get(&boss_id)
                        .map_or(false, |h| h.contains(&t.id))
                })
                .cloned()
                .collect();

            // Reload visible timer defs into engine
            state.timer_engine.load_timer_defs(visible_timers).await;

            re_register_shortcuts_for_active_boss(&app, &boss_id, config, &settings).await;
        }
    }

    Ok(())
}

#[tauri::command]
async fn reset_hidden_timers(
    boss_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().await;

    settings.hidden_timers.remove(&boss_id);

    settings::save_settings(&state.settings_path, &settings)?;

    // Re-register shortcuts if this is the active boss
    let active = state.active_boss.lock().await;
    if active.as_deref() == Some(&boss_id) {
        let bosses = state.bosses.lock().await;
        if let Some((_, config)) = bosses.iter().find(|(id, _)| id == &boss_id) {
            // All timers are now visible
            state
                .timer_engine
                .load_timer_defs(config.timers.clone())
                .await;

            re_register_shortcuts_for_active_boss(&app, &boss_id, config, &settings).await;
        }
    }

    Ok(())
}

#[tauri::command]
async fn save_timer_order(
    boss_id: String,
    order: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().await;
    settings.timer_orders.insert(boss_id, order);
    settings::save_settings(&state.settings_path, &settings)?;
    Ok(())
}

#[tauri::command]
async fn set_mini_mode(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut settings = state.settings.lock().await;
    settings.mini_mode = enabled;
    settings::save_settings(&state.settings_path, &settings)?;
    Ok(())
}

#[tauri::command]
async fn toggle_mute_timer(
    boss_id: String,
    timer_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut settings = state.settings.lock().await;

    let muted_list = settings.muted_timers.entry(boss_id).or_default();
    let is_muted = if let Some(pos) = muted_list.iter().position(|id| id == &timer_id) {
        muted_list.remove(pos);
        false
    } else {
        muted_list.push(timer_id);
        true
    };

    settings::save_settings(&state.settings_path, &settings)?;

    // Update engine muted set
    let active = state.active_boss.lock().await;
    if let Some(active_id) = active.as_ref() {
        let muted_set: HashSet<String> = settings
            .muted_timers
            .get(active_id)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        state.timer_engine.set_muted_defs(muted_set).await;
    }

    Ok(is_muted)
}

#[tauri::command]
async fn trigger_timer(
    timer_def_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // If timer for this def is already running, reset it
    if state
        .timer_engine
        .has_running_timers_for_def(&timer_def_id)
        .await
    {
        state.timer_engine.stop_by_def_id(&timer_def_id).await;
    }
    state.timer_engine.start_timer(&timer_def_id).await
}

#[tauri::command]
async fn stop_all_timers(state: State<'_, AppState>) -> Result<(), String> {
    state.timer_engine.stop_all().await;
    Ok(())
}

#[tauri::command]
async fn get_timers(state: State<'_, AppState>) -> Result<Vec<Timer>, String> {
    Ok(state.timer_engine.get_timers().await)
}

#[tauri::command]
async fn reload_bosses(state: State<'_, AppState>) -> Result<Vec<BossListItem>, String> {
    let new_bosses = load_all_bosses(&state.bosses_dir);
    let mut bosses = state.bosses.lock().await;
    *bosses = new_bosses;
    Ok(bosses
        .iter()
        .map(|(id, config)| BossListItem {
            id: id.clone(),
            name: config.boss.name.clone(),
            description: config.boss.description.clone(),
            timer_count: config.timers.len(),
        })
        .collect())
}

#[tauri::command]
async fn get_active_boss(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let active = state.active_boss.lock().await;
    Ok(active.clone())
}

#[tauri::command]
async fn set_cursor_passthrough(app: AppHandle, ignore: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        window
            .set_ignore_cursor_events(ignore)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = state.settings.lock().await;
    Ok(settings.clone())
}

#[derive(serde::Deserialize)]
struct SaveSettingsPayload {
    settings: AppSettings,
}

#[tauri::command]
async fn save_settings(
    payload: SaveSettingsPayload,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let new_settings = payload.settings;

    // Save to file
    settings::save_settings(&state.settings_path, &new_settings)?;

    // Update in-memory settings
    let mut settings = state.settings.lock().await;
    *settings = new_settings.clone();
    drop(settings);

    // Re-register shortcuts if a boss is active
    let active_boss = state.active_boss.lock().await;
    if let Some(boss_id) = active_boss.clone() {
        drop(active_boss);
        let bosses = state.bosses.lock().await;
        if let Some((_, config)) = bosses.iter().find(|(id, _)| id == &boss_id) {
            re_register_shortcuts_for_active_boss(&app, &boss_id, config, &new_settings).await;
        }
    }

    Ok(())
}

#[derive(serde::Serialize)]
struct BossHotkeyInfo {
    timer_id: String,
    timer_name: String,
    default_hotkey: Option<String>,
    effective_hotkey: Option<String>,
}

#[tauri::command]
async fn get_boss_hotkeys(
    boss_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<BossHotkeyInfo>, String> {
    let bosses = state.bosses.lock().await;
    let (_, config) = bosses
        .iter()
        .find(|(id, _)| id == &boss_id)
        .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;

    let settings = state.settings.lock().await;
    let overrides = settings.hotkeys.get(&boss_id);

    let hotkeys = config
        .timers
        .iter()
        .map(|t| {
            let override_hotkey = overrides.and_then(|o| o.get(&t.id));
            BossHotkeyInfo {
                timer_id: t.id.clone(),
                timer_name: t.name.clone(),
                default_hotkey: t.hotkey.clone(),
                effective_hotkey: override_hotkey
                    .cloned()
                    .or_else(|| t.hotkey.clone()),
            }
        })
        .collect();

    Ok(hotkeys)
}

#[tauri::command]
async fn list_buffs(state: State<'_, AppState>) -> Result<Vec<BuffItem>, String> {
    let config = state.buffs.lock().await;
    Ok(config.buffs.clone())
}

#[derive(serde::Deserialize)]
struct AddBuffPayload {
    name: String,
    duration_secs: u32,
    hotkey: Option<String>,
}

#[tauri::command]
async fn add_buff(
    payload: AddBuffPayload,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BuffItem, String> {
    if payload.name.trim().is_empty() {
        return Err("Buff name cannot be empty".to_string());
    }
    if payload.duration_secs == 0 {
        return Err("Duration must be a positive integer".to_string());
    }

    let buff = BuffItem {
        id: uuid::Uuid::new_v4().to_string(),
        name: payload.name,
        duration_secs: payload.duration_secs,
        hotkey: payload.hotkey,
        enabled: true,
    };

    let mut config = state.buffs.lock().await;
    config.buffs.push(buff.clone());
    buff_config::save_buffs(&state.buffs_path, &config)?;
    drop(config);

    // Re-register buff shortcuts
    re_register_buff_shortcuts(&app, &state).await;

    Ok(buff)
}

#[derive(serde::Deserialize)]
struct UpdateBuffPayload {
    id: String,
    name: Option<String>,
    duration_secs: Option<u32>,
    hotkey: Option<Option<String>>,
    enabled: Option<bool>,
}

#[tauri::command]
async fn update_buff(
    payload: UpdateBuffPayload,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BuffItem, String> {
    let mut config = state.buffs.lock().await;
    let buff = config
        .buffs
        .iter_mut()
        .find(|b| b.id == payload.id)
        .ok_or_else(|| format!("Buff '{}' not found", payload.id))?;

    if let Some(name) = &payload.name {
        if name.trim().is_empty() {
            return Err("Buff name cannot be empty".to_string());
        }
        buff.name = name.clone();
    }
    if let Some(secs) = payload.duration_secs {
        if secs == 0 {
            return Err("Duration must be a positive integer".to_string());
        }
        buff.duration_secs = secs;
    }
    if let Some(hotkey) = payload.hotkey {
        buff.hotkey = hotkey;
    }
    if let Some(enabled) = payload.enabled {
        buff.enabled = enabled;
        // If disabling, stop any running buff timer for this buff
        if !enabled {
            let buff_def_id = format!("buff_{}", buff.id);
            state.timer_engine.stop_by_def_id(&buff_def_id).await;
        }
    }

    let updated = buff.clone();
    buff_config::save_buffs(&state.buffs_path, &config)?;
    drop(config);

    // Re-register buff shortcuts
    re_register_buff_shortcuts(&app, &state).await;

    Ok(updated)
}

#[tauri::command]
async fn delete_buff(
    buff_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut config = state.buffs.lock().await;
    let original_len = config.buffs.len();
    config.buffs.retain(|b| b.id != buff_id);
    if config.buffs.len() == original_len {
        return Err(format!("Buff '{}' not found", buff_id));
    }

    // Stop any running timer for this buff
    let buff_def_id = format!("buff_{}", buff_id);
    state.timer_engine.stop_by_def_id(&buff_def_id).await;

    buff_config::save_buffs(&state.buffs_path, &config)?;
    drop(config);

    // Re-register buff shortcuts
    re_register_buff_shortcuts(&app, &state).await;

    Ok(())
}

async fn re_register_buff_shortcuts(app: &AppHandle, state: &AppState) {
    let config = state.buffs.lock().await;
    let settings = state.settings.lock().await;
    let active_boss = state.active_boss.lock().await;

    // Reload buff timer defs in engine
    let buff_timer_defs: Vec<boss_config::TimerDef> = config
        .buffs
        .iter()
        .filter(|b| b.enabled)
        .map(|b| boss_config::TimerDef {
            id: format!("buff_{}", b.id),
            name: b.name.clone(),
            icon: "💠".to_string(),
            duration_secs: b.duration_secs as f64,
            hotkey: None,
            chain_to: None,
            color: "#4ECDC4".to_string(),
            warning_secs: 5.0,
            repeat: false,
            timer_type: Some("buff".to_string()),
            description: None,
        })
        .collect();
    state.timer_engine.load_buff_defs(buff_timer_defs).await;

    let boss_config = if let Some(boss_id) = active_boss.as_ref() {
        let bosses = state.bosses.lock().await;
        bosses.iter().find(|(id, _)| id == boss_id).map(|(id, c)| (id.clone(), c.clone()))
    } else {
        None
    };

    // If monitoring is active, update KeyListener's hotkey map instead of
    // re-registering buff shortcuts via global_shortcut
    if state.key_listener.is_running() {
        let map = build_buff_hotkey_map(&config.buffs);
        state.key_listener.update_hotkeys(map);

        // Only register non-buff shortcuts
        shortcuts::register_shortcuts_without_buffs(
            app,
            boss_config.as_ref().map(|(id, _)| id.as_str()),
            boss_config.as_ref().map(|(_, c)| c),
            &settings,
        );
    } else {
        shortcuts::register_all_shortcuts(
            app,
            boss_config.as_ref().map(|(id, _)| id.as_str()),
            boss_config.as_ref().map(|(_, c)| c),
            &settings,
            &config.buffs,
        );
    }
}

#[tauri::command]
async fn trigger_buff_timer(
    buff_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let def_id = format!("buff_{}", buff_id);

    // If already running, reset
    if state
        .timer_engine
        .has_running_timers_for_def(&def_id)
        .await
    {
        state.timer_engine.stop_by_def_id(&def_id).await;
    }

    state.timer_engine.start_timer(&def_id).await
}

/// Build a hotkey→buff_id map from current buff config.
fn build_buff_hotkey_map(buffs: &[BuffItem]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for b in buffs {
        if b.enabled {
            if let Some(ref hk) = b.hotkey {
                if !hk.is_empty() {
                    map.insert(hk.clone(), b.id.clone());
                }
            }
        }
    }
    map
}

#[tauri::command]
async fn start_buff_monitoring(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state.key_listener.is_running() {
        return Ok(());
    }

    // Build hotkey map
    let buffs = state.buffs.lock().await;
    let map = build_buff_hotkey_map(&buffs.buffs);
    state.key_listener.update_hotkeys(map);
    drop(buffs);

    // Unregister buff shortcuts from global_shortcut (so they don't consume keys)
    // Re-register only non-buff shortcuts
    {
        let settings = state.settings.lock().await;
        let active_boss = state.active_boss.lock().await;
        let bosses = state.bosses.lock().await;

        let boss_config = if let Some(boss_id) = active_boss.as_ref() {
            bosses.iter().find(|(id, _)| id == boss_id).map(|(id, c)| (id.clone(), c.clone()))
        } else {
            None
        };

        // Register only non-buff shortcuts (global + boss)
        shortcuts::register_shortcuts_without_buffs(
            &app,
            boss_config.as_ref().map(|(id, _)| id.as_str()),
            boss_config.as_ref().map(|(_, c)| c),
            &settings,
        );
    }

    // Start CGEventTap
    let app_clone = app.clone();
    let callback: key_listener::Callback = Arc::new(move |buff_id: String| {
        let app = app_clone.clone();
        let def_id = format!("buff_{}", buff_id);
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            if state.timer_engine.has_running_timers_for_def(&def_id).await {
                state.timer_engine.stop_by_def_id(&def_id).await;
            }
            state.timer_engine.stop_expired_by_def_id(&def_id).await;
            let _ = state.timer_engine.start_timer(&def_id).await;
        });
    });

    state.key_listener.start(callback)
}

#[tauri::command]
async fn stop_buff_monitoring(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.key_listener.stop();

    // Re-register all shortcuts including buff shortcuts via global_shortcut
    re_register_buff_shortcuts(&app, &state).await;

    Ok(())
}

#[tauri::command]
async fn get_monitoring_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.key_listener.is_running())
}

#[tauri::command]
async fn check_accessibility_permission() -> Result<bool, String> {
    Ok(key_listener::KeyListener::check_accessibility())
}

#[tauri::command]
async fn request_accessibility_permission() -> Result<(), String> {
    key_listener::KeyListener::request_accessibility();
    Ok(())
}

#[tauri::command]
async fn disable_shortcuts(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().await;
    let pause_hotkey = settings.pause_hotkey.clone();
    drop(settings);
    // Keep only the pause-toggle shortcut alive
    shortcuts::register_pause_toggle_only(&app, &pause_hotkey);
    Ok(())
}

#[tauri::command]
async fn enable_shortcuts(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Don't re-enable if shortcuts are globally paused
    let paused = *state.shortcuts_paused.lock().await;
    if paused {
        return Ok(());
    }

    let settings = state.settings.lock().await.clone();
    let active_boss = state.active_boss.lock().await.clone();
    let buffs = state.buffs.lock().await.clone();

    if let Some(boss_id) = active_boss.as_ref() {
        let bosses = state.bosses.lock().await;
        let config = bosses
            .iter()
            .find(|(id, _)| id == boss_id)
            .map(|(_, c)| c.clone());
        drop(bosses);

        if let Some(config) = config {
            if state.key_listener.is_running() {
                shortcuts::register_shortcuts_without_buffs(
                    &app,
                    Some(boss_id.as_str()),
                    Some(&config),
                    &settings,
                );
            } else {
                shortcuts::register_all_shortcuts(
                    &app,
                    Some(boss_id.as_str()),
                    Some(&config),
                    &settings,
                    &buffs.buffs,
                );
            }
        }
    } else {
        shortcuts::register_all_shortcuts(&app, None, None, &settings, &buffs.buffs);
    }

    // Always keep pause toggle registered
    shortcuts::register_pause_toggle(&app, &settings.pause_hotkey);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let bosses_dir = get_bosses_dir(&handle);
            let settings_path = get_settings_path(&handle);

            // Ensure data dir exists
            if let Some(parent) = settings_path.parent() {
                fs::create_dir_all(parent).ok();
            }

            // Copy default boss configs if missing
            ensure_default_bosses(&bosses_dir);

            // Load all boss configs
            let bosses = load_all_bosses(&bosses_dir);

            // Load settings
            let app_settings = settings::load_settings(&settings_path);

            // Load buff config
            let buffs_path = get_buffs_path(&handle);
            let buff_cfg = buff_config::load_buffs(&buffs_path);

            // Build boss list for tray menu
            let boss_list: Vec<(String, String)> = bosses
                .iter()
                .map(|(id, c)| (id.clone(), c.boss.name.clone()))
                .collect();

            let timer_engine = Arc::new(TimerEngine::new());

            // Start the tick loop
            timer_engine::start_tick_loop(timer_engine.clone(), handle.clone());

            // Load buff timer defs into engine
            let buff_timer_defs: Vec<boss_config::TimerDef> = buff_cfg
                .buffs
                .iter()
                .filter(|b| b.enabled)
                .map(|b| boss_config::TimerDef {
                    id: format!("buff_{}", b.id),
                    name: b.name.clone(),
                    icon: "💠".to_string(),
                    duration_secs: b.duration_secs as f64,
                    hotkey: None,
                    chain_to: None,
                    color: "#4ECDC4".to_string(),
                    warning_secs: 5.0,
                    repeat: false,
                    timer_type: Some("buff".to_string()),
                    description: None,
                })
                .collect();
            timer_engine::load_buff_defs_sync(&timer_engine, &buff_timer_defs, &handle);

            app.manage(AppState {
                timer_engine,
                bosses: Arc::new(Mutex::new(bosses)),
                active_boss: Arc::new(Mutex::new(None)),
                bosses_dir,
                settings: Arc::new(Mutex::new(app_settings.clone())),
                settings_path,
                buffs: Arc::new(Mutex::new(buff_cfg.clone())),
                buffs_path: buffs_path.clone(),
                key_listener: key_listener::KeyListener::new(),
                shortcuts_paused: Arc::new(Mutex::new(false)),
            });

            // Register buff shortcuts at startup
            shortcuts::register_buff_shortcuts_only(
                &handle,
                &buff_cfg.buffs,
            );

            // Register pause-toggle shortcut
            shortcuts::register_pause_toggle(&handle, &app_settings.pause_hotkey);

            // Setup system tray (after AppState is managed)
            tray::setup_tray(&handle, &boss_list)?;

            // Exit the entire app when the main overlay window is closed
            if let Some(overlay) = app.get_webview_window("overlay") {
                let app_handle = handle.clone();
                overlay.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        app_handle.exit(0);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_bosses,
            select_boss,
            trigger_timer,
            stop_all_timers,
            get_timers,
            reload_bosses,
            get_active_boss,
            set_cursor_passthrough,
            get_settings,
            save_settings,
            get_boss_hotkeys,
            hide_timer,
            reset_hidden_timers,
            set_mini_mode,
            save_timer_order,
            toggle_mute_timer,
            list_buffs,
            add_buff,
            update_buff,
            delete_buff,
            trigger_buff_timer,
            start_buff_monitoring,
            stop_buff_monitoring,
            get_monitoring_status,
            check_accessibility_permission,
            request_accessibility_permission,
            disable_shortcuts,
            enable_shortcuts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

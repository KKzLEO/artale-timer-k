mod boss_config;
pub mod settings;
pub mod shortcuts;
pub mod sound;
mod timer_engine;
mod tray;

use boss_config::{load_all_bosses, BossConfig};
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

fn ensure_default_bosses(bosses_dir: &PathBuf) {
    fs::create_dir_all(bosses_dir).ok();

    // Remove legacy boss configs
    for old in &["zakum.toml", "horntail.toml"] {
        let p = bosses_dir.join(old);
        if p.exists() {
            fs::remove_file(&p).ok();
        }
    }

    let ccq_path = bosses_dir.join("chaos_crimson_queen.toml");
    if !ccq_path.exists() {
        let ccq = include_str!("../resources/bosses/chaos_crimson_queen.toml");
        fs::write(&ccq_path, ccq).ok();
    }

    let lotus_path = bosses_dir.join("hard_lotus.toml");
    if !lotus_path.exists() {
        let lotus = include_str!("../resources/bosses/hard_lotus.toml");
        fs::write(&lotus_path, lotus).ok();
    }
}

/// Re-register shortcuts for the currently active boss, filtering hidden timers.
pub fn re_register_shortcuts_for_active_boss(
    app: &AppHandle,
    boss_id: &str,
    config: &BossConfig,
    settings: &AppSettings,
) {
    let hidden = settings
        .hidden_timers
        .get(boss_id)
        .cloned()
        .unwrap_or_default();

    let visible_timers: Vec<_> = config
        .timers
        .iter()
        .filter(|t| !hidden.contains(&t.id))
        .cloned()
        .collect();

    let hotkey_overrides = settings
        .hotkeys
        .get(boss_id)
        .cloned()
        .unwrap_or_default();

    shortcuts::register_boss_shortcuts(
        app,
        boss_id,
        &visible_timers,
        &hotkey_overrides,
        &settings.stop_all_hotkey,
        &settings.back_hotkey,
    );
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
    re_register_shortcuts_for_active_boss(&app, &boss_id, &config, &settings);
    drop(settings);

    // Set active boss
    let mut active = state.active_boss.lock().await;
    *active = Some(boss_id);

    Ok(SelectBossResponse {
        config,
        hidden_timers: hidden,
        muted_timers: muted,
        mini_mode,
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

            re_register_shortcuts_for_active_boss(&app, &boss_id, config, &settings);
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

            re_register_shortcuts_for_active_boss(&app, &boss_id, config, &settings);
        }
    }

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
            re_register_shortcuts_for_active_boss(&app, &boss_id, config, &new_settings);
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

            // Build boss list for tray menu
            let boss_list: Vec<(String, String)> = bosses
                .iter()
                .map(|(id, c)| (id.clone(), c.boss.name.clone()))
                .collect();

            let timer_engine = Arc::new(TimerEngine::new());

            // Start the tick loop
            timer_engine::start_tick_loop(timer_engine.clone(), handle.clone());

            app.manage(AppState {
                timer_engine,
                bosses: Arc::new(Mutex::new(bosses)),
                active_boss: Arc::new(Mutex::new(None)),
                bosses_dir,
                settings: Arc::new(Mutex::new(app_settings)),
                settings_path,
            });

            // Setup system tray (after AppState is managed)
            tray::setup_tray(&handle, &boss_list)?;

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
            toggle_mute_timer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

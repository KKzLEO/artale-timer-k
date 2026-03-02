mod boss_config;
pub mod settings;
pub mod shortcuts;
pub mod sound;
mod timer_engine;
mod tray;

use boss_config::{load_all_bosses, BossConfig};
use settings::AppSettings;
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
    if bosses_dir.exists() && fs::read_dir(bosses_dir).map_or(false, |mut d| d.next().is_some()) {
        return;
    }
    fs::create_dir_all(bosses_dir).ok();

    let zakum = include_str!("../resources/bosses/zakum.toml");
    let horntail = include_str!("../resources/bosses/horntail.toml");

    fs::write(bosses_dir.join("zakum.toml"), zakum).ok();
    fs::write(bosses_dir.join("horntail.toml"), horntail).ok();
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

#[tauri::command]
async fn select_boss(
    boss_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BossConfig, String> {
    let bosses = state.bosses.lock().await;
    let (_, config) = bosses
        .iter()
        .find(|(id, _)| id == &boss_id)
        .ok_or_else(|| format!("Boss '{}' not found", boss_id))?;
    let config = config.clone();
    drop(bosses);

    // Stop all running timers
    state.timer_engine.stop_all().await;

    // Load timer defs
    state
        .timer_engine
        .load_timer_defs(config.timers.clone())
        .await;

    // Get hotkey overrides from settings
    let settings = state.settings.lock().await;
    let hotkey_overrides = settings
        .hotkeys
        .get(&boss_id)
        .cloned()
        .unwrap_or_default();
    let stop_all_hotkey = settings.stop_all_hotkey.clone();
    let back_hotkey = settings.back_hotkey.clone();
    drop(settings);

    // Register global shortcuts for this boss
    shortcuts::register_boss_shortcuts(
        &app,
        &boss_id,
        &config.timers,
        &hotkey_overrides,
        &stop_all_hotkey,
        &back_hotkey,
    );

    // Set active boss
    let mut active = state.active_boss.lock().await;
    *active = Some(boss_id);

    Ok(config)
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
            let timers = config.timers.clone();
            drop(bosses);

            let hotkey_overrides = new_settings
                .hotkeys
                .get(&boss_id)
                .cloned()
                .unwrap_or_default();

            shortcuts::register_boss_shortcuts(
                &app,
                &boss_id,
                &timers,
                &hotkey_overrides,
                &new_settings.stop_all_hotkey,
                &new_settings.back_hotkey,
            );
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

            // Copy default boss configs on first run
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

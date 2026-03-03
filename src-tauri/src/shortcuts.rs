use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::boss_config::TimerDef;
use crate::buff_config::BuffItem;
use crate::settings::AppSettings;
use crate::AppState;

/// Register all shortcuts: global keys, boss timer keys, and buff keys.
/// Called when boss is selected or buff config changes.
pub fn register_all_shortcuts(
    app: &AppHandle,
    boss_id: Option<&str>,
    boss_config: Option<&crate::boss_config::BossConfig>,
    settings: &AppSettings,
    buffs: &[BuffItem],
) {
    // Unregister all existing shortcuts first
    let _ = app.global_shortcut().unregister_all();

    let mut registered_keys: HashSet<String> = HashSet::new();

    // Register "stop all" shortcut
    register_stop_all(app, &settings.stop_all_hotkey);
    registered_keys.insert(settings.stop_all_hotkey.clone());

    // Register "back to main" shortcut
    register_back_to_main(app, &settings.back_hotkey);
    registered_keys.insert(settings.back_hotkey.clone());

    // Register boss timer shortcuts
    if let (Some(boss_id), Some(config)) = (boss_id, boss_config) {
        let hidden = settings
            .hidden_timers
            .get(boss_id)
            .cloned()
            .unwrap_or_default();
        let hotkey_overrides = settings
            .hotkeys
            .get(boss_id)
            .cloned()
            .unwrap_or_default();

        let visible_timers: Vec<_> = config
            .timers
            .iter()
            .filter(|t| !hidden.contains(&t.id))
            .collect();

        for timer in &visible_timers {
            let hotkey = hotkey_overrides
                .get(&timer.id)
                .or(timer.hotkey.as_ref());

            if let Some(hotkey) = hotkey {
                if registered_keys.contains(hotkey) {
                    continue; // skip duplicate
                }
                register_boss_timer_shortcut(app, &timer.id, hotkey);
                registered_keys.insert(hotkey.clone());
            }
        }
    }

    // Register buff shortcuts
    for buff in buffs {
        if !buff.enabled {
            continue;
        }
        if let Some(ref hotkey) = buff.hotkey {
            if hotkey.is_empty() || registered_keys.contains(hotkey) {
                continue; // skip empty or conflicting
            }
            register_buff_shortcut(app, &buff.id, hotkey);
            registered_keys.insert(hotkey.clone());
        }
    }
}

/// Register boss shortcuts in the legacy pattern (for re_register_shortcuts_for_active_boss).
pub fn register_boss_shortcuts(
    app: &AppHandle,
    _boss_id: &str,
    timers: &[TimerDef],
    hotkey_overrides: &HashMap<String, String>,
    stop_all_hotkey: &str,
    back_hotkey: &str,
) {
    // Unregister all existing shortcuts first
    let _ = app.global_shortcut().unregister_all();

    register_stop_all(app, stop_all_hotkey);
    register_back_to_main(app, back_hotkey);

    for timer in timers {
        let hotkey = hotkey_overrides
            .get(&timer.id)
            .or(timer.hotkey.as_ref());

        if let Some(hotkey) = hotkey {
            register_boss_timer_shortcut(app, &timer.id, hotkey);
        }
    }
}

/// Register only buff shortcuts (used at startup before any boss is selected).
pub fn register_buff_shortcuts_only(
    app: &AppHandle,
    buffs: &[BuffItem],
) {
    // Don't unregister_all here — called right after manage() so no shortcuts exist yet
    // Register stop_all and back_to_main from defaults
    // We need settings, so read from state
    let state = app.state::<AppState>();
    let settings = tauri::async_runtime::block_on(async {
        state.settings.lock().await.clone()
    });

    let _ = app.global_shortcut().unregister_all();

    let mut registered_keys: HashSet<String> = HashSet::new();

    register_stop_all(app, &settings.stop_all_hotkey);
    registered_keys.insert(settings.stop_all_hotkey.clone());

    register_back_to_main(app, &settings.back_hotkey);
    registered_keys.insert(settings.back_hotkey.clone());

    for buff in buffs {
        if !buff.enabled {
            continue;
        }
        if let Some(ref hotkey) = buff.hotkey {
            if hotkey.is_empty() || registered_keys.contains(hotkey) {
                continue;
            }
            register_buff_shortcut(app, &buff.id, hotkey);
            registered_keys.insert(hotkey.clone());
        }
    }
}

/// Check for hotkey conflicts between boss timers and buffs.
/// Returns list of conflicting hotkeys.
pub fn check_hotkey_conflicts(
    boss_config: Option<&crate::boss_config::BossConfig>,
    boss_hotkey_overrides: &HashMap<String, String>,
    buffs: &[BuffItem],
    settings: &AppSettings,
) -> Vec<String> {
    let mut used_keys: HashSet<String> = HashSet::new();
    let mut conflicts = Vec::new();

    // Global keys
    used_keys.insert(settings.stop_all_hotkey.clone());
    used_keys.insert(settings.back_hotkey.clone());

    // Boss timer keys
    if let Some(config) = boss_config {
        for timer in &config.timers {
            let hotkey = boss_hotkey_overrides
                .get(&timer.id)
                .or(timer.hotkey.as_ref());
            if let Some(hk) = hotkey {
                used_keys.insert(hk.clone());
            }
        }
    }

    // Check buff keys against used keys
    for buff in buffs {
        if !buff.enabled {
            continue;
        }
        if let Some(ref hotkey) = buff.hotkey {
            if !hotkey.is_empty() && used_keys.contains(hotkey) {
                conflicts.push(hotkey.clone());
            }
        }
    }

    conflicts
}

fn register_stop_all(app: &AppHandle, hotkey: &str) {
    let app_clone = app.clone();
    let _ = app
        .global_shortcut()
        .on_shortcut(hotkey, move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let app = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    state.timer_engine.stop_all().await;
                });
            }
        });
}

fn register_back_to_main(app: &AppHandle, hotkey: &str) {
    let app_clone = app.clone();
    let _ = app
        .global_shortcut()
        .on_shortcut(hotkey, move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let app = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    state.timer_engine.stop_all().await;
                    let mut active = state.active_boss.lock().await;
                    *active = None;
                    drop(active);
                    if let Some(window) = app.get_webview_window("overlay") {
                        let _ = window.set_ignore_cursor_events(false);
                    }
                    let _ = app.emit("back-to-main", ());
                });
            }
        });
}

fn register_boss_timer_shortcut(app: &AppHandle, timer_id: &str, hotkey: &str) {
    let app_clone = app.clone();
    let timer_id = timer_id.to_string();
    let _ = app.global_shortcut().on_shortcut(
        hotkey,
        move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let app = app_clone.clone();
                let tid = timer_id.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    if state.timer_engine.has_running_timers_for_def(&tid).await {
                        state.timer_engine.stop_by_def_id(&tid).await;
                    }
                    let _ = state.timer_engine.start_timer(&tid).await;
                });
            }
        },
    );
}

fn register_buff_shortcut(app: &AppHandle, buff_id: &str, hotkey: &str) {
    let app_clone = app.clone();
    let def_id = format!("buff_{}", buff_id);
    let _ = app.global_shortcut().on_shortcut(
        hotkey,
        move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let app = app_clone.clone();
                let did = def_id.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    // If running or expired, reset
                    if state.timer_engine.has_running_timers_for_def(&did).await {
                        state.timer_engine.stop_by_def_id(&did).await;
                    }
                    // Also clear expired buff timers by def_id directly
                    state.timer_engine.stop_expired_by_def_id(&did).await;
                    let _ = state.timer_engine.start_timer(&did).await;
                });
            }
        },
    );
}

/// Register only global + boss shortcuts (no buff shortcuts).
/// Used when buff monitoring is active (CGEventTap handles buff keys).
pub fn register_shortcuts_without_buffs(
    app: &AppHandle,
    boss_id: Option<&str>,
    boss_config: Option<&crate::boss_config::BossConfig>,
    settings: &AppSettings,
) {
    let _ = app.global_shortcut().unregister_all();

    let mut registered_keys: HashSet<String> = HashSet::new();

    register_stop_all(app, &settings.stop_all_hotkey);
    registered_keys.insert(settings.stop_all_hotkey.clone());

    register_back_to_main(app, &settings.back_hotkey);
    registered_keys.insert(settings.back_hotkey.clone());

    if let (Some(boss_id), Some(config)) = (boss_id, boss_config) {
        let hidden = settings
            .hidden_timers
            .get(boss_id)
            .cloned()
            .unwrap_or_default();
        let hotkey_overrides = settings
            .hotkeys
            .get(boss_id)
            .cloned()
            .unwrap_or_default();

        let visible_timers: Vec<_> = config
            .timers
            .iter()
            .filter(|t| !hidden.contains(&t.id))
            .collect();

        for timer in &visible_timers {
            let hotkey = hotkey_overrides
                .get(&timer.id)
                .or(timer.hotkey.as_ref());

            if let Some(hotkey) = hotkey {
                if registered_keys.contains(hotkey) {
                    continue;
                }
                register_boss_timer_shortcut(app, &timer.id, hotkey);
                registered_keys.insert(hotkey.clone());
            }
        }
    }
}

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

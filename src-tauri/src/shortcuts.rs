use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::boss_config::TimerDef;
use crate::AppState;

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

    // Register "stop all" shortcut
    let app_clone = app.clone();
    let _ = app
        .global_shortcut()
        .on_shortcut(stop_all_hotkey, move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let app = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    state.timer_engine.stop_all().await;
                });
            }
        });

    // Register "back to main" shortcut
    let app_clone = app.clone();
    let _ = app
        .global_shortcut()
        .on_shortcut(back_hotkey, move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let app = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    // Stop all timers
                    state.timer_engine.stop_all().await;
                    // Reset active boss
                    let mut active = state.active_boss.lock().await;
                    *active = None;
                    drop(active);
                    // Disable cursor passthrough so picker is clickable
                    if let Some(window) = app.get_webview_window("overlay") {
                        let _ = window.set_ignore_cursor_events(false);
                    }
                    // Emit event to frontend
                    let _ = app.emit("back-to-main", ());
                });
            }
        });

    // Register shortcuts for each timer that has a hotkey
    for timer in timers {
        let hotkey = hotkey_overrides
            .get(&timer.id)
            .or(timer.hotkey.as_ref());

        if let Some(hotkey) = hotkey {
            let app_clone = app.clone();
            let timer_id = timer.id.clone();
            let _ = app.global_shortcut().on_shortcut(
                hotkey.as_str(),
                move |_app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let app = app_clone.clone();
                        let tid = timer_id.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<AppState>();
                            // If running, reset; otherwise start
                            if state.timer_engine.has_running_timers_for_def(&tid).await {
                                state.timer_engine.stop_by_def_id(&tid).await;
                            }
                            let _ = state.timer_engine.start_timer(&tid).await;
                        });
                    }
                },
            );
        }
    }
}

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

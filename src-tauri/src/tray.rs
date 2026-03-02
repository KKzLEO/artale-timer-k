use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

use crate::AppState;

pub fn setup_tray(app: &AppHandle, boss_list: &[(String, String)]) -> tauri::Result<()> {
    let stop_all =
        MenuItem::with_id(app, "stop_all", "停止所有計時器 / Stop All", true, None::<&str>)?;
    let reload =
        MenuItem::with_id(app, "reload", "重新載入設定 / Reload Config", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "結束 / Quit", true, None::<&str>)?;

    // Build boss submenu
    let mut boss_items: Vec<MenuItem<tauri::Wry>> = Vec::new();
    for (id, name) in boss_list {
        let item = MenuItem::with_id(
            app,
            &format!("boss_{}", id),
            name,
            true,
            None::<&str>,
        )?;
        boss_items.push(item);
    }

    let boss_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        boss_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let boss_submenu = Submenu::with_items(app, "選擇 Boss / Select Boss", true, &boss_refs)?;

    let menu = Menu::with_items(
        app,
        &[
            &boss_submenu,
            &separator,
            &stop_all,
            &reload,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("Artale Timer")
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "quit" => {
                    app.exit(0);
                }
                "stop_all" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        state.timer_engine.stop_all().await;
                    });
                }
                "reload" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        let new_bosses =
                            crate::boss_config::load_all_bosses(&state.bosses_dir);
                        let mut bosses = state.bosses.lock().await;
                        *bosses = new_bosses;
                    });
                }
                other => {
                    if let Some(boss_id) = other.strip_prefix("boss_") {
                        let app = app.clone();
                        let boss_id = boss_id.to_string();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<AppState>();
                            let bosses = state.bosses.lock().await;
                            if let Some((_, config)) =
                                bosses.iter().find(|(id, _)| id == &boss_id)
                            {
                                let config = config.clone();
                                drop(bosses);

                                state.timer_engine.stop_all().await;

                                let settings = state.settings.lock().await;
                                let hidden = settings
                                    .hidden_timers
                                    .get(&boss_id)
                                    .cloned()
                                    .unwrap_or_default();

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

                                let mut active = state.active_boss.lock().await;
                                *active = Some(boss_id.clone());
                                drop(active);

                                crate::re_register_shortcuts_for_active_boss(
                                    &app,
                                    &boss_id,
                                    &config,
                                    &settings,
                                );
                            }
                        });
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}

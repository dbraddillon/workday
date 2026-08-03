//! Shell / Desktop host layer.
//!
//! Wires the whole app together: a menu bar (tray) icon that toggles a
//! popover-style window near the icon, a global shortcut to focus it, a
//! background polling loop, and the SQLite-backed state. UI, connectors, and
//! standup logic live in their own modules.

mod commands;
mod config;
mod connector;
mod db;
mod delivery;
mod model;
mod standup;
mod sync;

use db::{settings_repo, Db};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

/// Small shared app state beyond the DB.
pub struct AppState {
    ready: AtomicBool,
}

impl AppState {
    fn new() -> Self {
        AppState { ready: AtomicBool::new(false) }
    }
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::Relaxed);
    }
}

/// Toggle popover visibility, positioning it near the tray icon.
fn toggle_window(app: &tauri::AppHandle, tray_rect: Option<tauri::Rect>) {
    let Some(window) = app.get_webview_window("main") else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    // Position under the tray icon when we know where it is; otherwise top-right.
    if let Some(rect) = tray_rect {
        if let (tauri::Position::Physical(pos), tauri::Size::Physical(size)) =
            (rect.position, rect.size)
        {
            let win_size = window.outer_size().ok();
            let win_w = win_size.map(|s| s.width as i32).unwrap_or(400);
            let x = pos.x + (size.width as i32 / 2) - (win_w / 2);
            let y = pos.y + size.height as i32 + 4;
            let _ = window.set_position(tauri::PhysicalPosition::new(x.max(0), y));
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    // Register the global-shortcut plugin exactly once, with its handler. The
    // ⌘⇧J toggle is matched inside the handler; we call `.register()` in setup.
    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
        let toggle = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyJ);
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, sc, event| {
                    if sc == &toggle && event.state() == ShortcutState::Pressed {
                        toggle_window(app, None);
                    }
                })
                .build(),
        );
    }

    builder
        .setup(|app| {
            // On macOS, run as a menu bar accessory (no Dock icon).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // ---- Data layer: open SQLite in the app data dir. ----
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir).ok();
            let db = Db::open(&data_dir.join("workday.db")).expect("open db");
            app.manage(db);
            app.manage(AppState::new());

            // ---- Tray icon + menu. ----
            let show_item = MenuItem::with_id(app, "show", "Open Workday", true, None::<&str>)?;
            let refresh_item =
                MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &refresh_item, &quit_item])?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true) // render monochrome to match the menu bar
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => toggle_window(app, None),
                    "refresh" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let db = app.state::<Db>();
                            let settings = {
                                let conn = db.0.lock().unwrap();
                                settings_repo::load(&conn)
                            };
                            let _ = sync::run_sync(&db, &settings).await;
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle(), Some(rect));
                    }
                })
                .build(app)?;

            // ---- Register the ⌘⇧J toggle shortcut (plugin + handler already
            // set up on the builder). Best-effort. ----
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let _ = app.global_shortcut().register(Shortcut::new(
                    Some(Modifiers::SUPER | Modifiers::SHIFT),
                    Code::KeyJ,
                ));
            }

            // ---- Hide the window when it loses focus (popover behavior). ----
            if let Some(win) = app.get_webview_window("main") {
                let w = win.clone();
                win.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let _ = w.hide();
                    }
                });
            }

            // ---- Background polling loop. ----
            // Reads the interval live each pass. On repeated failures it applies
            // a simple linear backoff (capped) so a misconfigured Jira doesn't
            // get hammered every interval; a success resets the backoff.
            let poll_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Kick an initial sync shortly after launch.
                tokio::time::sleep(Duration::from_secs(2)).await;
                let mut consecutive_failures: u32 = 0;
                loop {
                    let db = poll_handle.state::<Db>();
                    let settings = {
                        let conn = db.0.lock().unwrap();
                        settings_repo::load(&conn)
                    };
                    match sync::run_sync(&db, &settings).await {
                        Ok(_) => consecutive_failures = 0,
                        Err(_) => consecutive_failures = consecutive_failures.saturating_add(1),
                    }
                    let base = settings.refresh_interval_secs.max(30);
                    // Back off up to ~5 min extra after repeated failures.
                    let backoff = (consecutive_failures.min(10) as u64) * 30;
                    tokio::time::sleep(Duration::from_secs(base + backoff)).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::set_jira_token,
            commands::get_in_progress,
            commands::get_recent,
            commands::get_sync_status,
            commands::refresh_now,
            commands::build_standup_model,
            commands::generate_standup,
            commands::record_delivery,
            commands::hide_window,
            commands::ui_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

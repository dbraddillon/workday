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
use std::sync::RwLock;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

/// Small shared app state beyond the DB.
pub struct AppState {
    ready: AtomicBool,
    /// Cached Jira API token, read from the Keychain **once** at startup.
    ///
    /// Reading the Keychain triggers a macOS ACL prompt whenever the binary's
    /// signature doesn't match the one that stored the secret (which is every
    /// rebuild for an unsigned dev binary). We used to read it on every settings
    /// load and every sync pass — dozens of prompts. Caching it here means at
    /// most one prompt per launch, and "Always Allow" silences even that until
    /// the next rebuild. `None` = no token stored.
    jira_token: RwLock<Option<String>>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            ready: AtomicBool::new(false),
            jira_token: RwLock::new(config::get_jira_token()),
        }
    }
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::Relaxed);
    }
    /// Cached token (no Keychain hit).
    pub fn jira_token(&self) -> Option<String> {
        self.jira_token.read().ok().and_then(|g| g.clone())
    }
    /// Whether a token is present, without touching the Keychain.
    pub fn has_jira_token(&self) -> bool {
        self.jira_token.read().map(|g| g.is_some()).unwrap_or(false)
    }
    /// Update the cache after a Keychain write/clear.
    pub fn set_jira_token(&self, token: Option<String>) {
        if let Ok(mut g) = self.jira_token.write() {
            *g = token;
        }
    }
}

/// Toggle popover visibility, positioning it near the tray icon.
///
/// Positioning is done entirely in **physical** pixels, clamped to the monitor
/// the tray icon is on. This matters on multi-monitor + Retina setups: mixing
/// the window's physical `outer_size` with logical math (or assuming one scale
/// factor) placed the popover off every display — visible=true but nowhere seen.
fn toggle_window(app: &tauri::AppHandle, tray_rect: Option<tauri::Rect>) {
    let Some(window) = app.get_webview_window("main") else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    if let Some(rect) = tray_rect {
        if let (tauri::Position::Physical(pos), tauri::Size::Physical(size)) =
            (rect.position, rect.size)
        {
            // Window size in physical px (already scaled for its monitor).
            let win = window.outer_size().ok();
            let win_w = win.map(|s| s.width as i32).unwrap_or(400);

            // Center under the tray icon, place just below the menu bar.
            let mut x = pos.x + (size.width as i32 / 2) - (win_w / 2);
            let mut y = pos.y + size.height as i32 + 4;

            // Clamp to the monitor under the tray icon so we never land in a
            // multi-display dead zone. Fall back to primary if lookup fails.
            let monitor = window
                .monitor_from_point(pos.x as f64, pos.y as f64)
                .ok()
                .flatten()
                .or_else(|| window.primary_monitor().ok().flatten());
            if let Some(m) = monitor {
                let mp = m.position();
                let ms = m.size();
                let win_w = win.map(|s| s.width as i32).unwrap_or(400);
                let win_h = win.map(|s| s.height as i32).unwrap_or(560);
                let min_x = mp.x;
                let max_x = mp.x + ms.width as i32 - win_w;
                let min_y = mp.y;
                let max_y = mp.y + ms.height as i32 - win_h;
                x = x.clamp(min_x.min(max_x), max_x.max(min_x));
                y = y.clamp(min_y.min(max_y), max_y.max(min_y));
            }

            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
    } else {
        // No tray rect (menu item / shortcut): center on the primary monitor.
        if let Ok(Some(m)) = window.primary_monitor() {
            let mp = m.position();
            let ms = m.size();
            let win = window.outer_size().ok();
            let win_w = win.map(|s| s.width as i32).unwrap_or(400);
            let win_h = win.map(|s| s.height as i32).unwrap_or(560);
            let x = mp.x + (ms.width as i32 - win_w) / 2;
            let y = mp.y + (ms.height as i32 - win_h) / 2;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
    }

    // Re-assert float-above behavior on each show. As an LSUIElement/Accessory
    // app, the popover otherwise slips behind other apps' windows (you'd have to
    // minimize them to find it). always_on_top + visible-on-all-workspaces makes
    // it behave like a real menu bar popover.
    let _ = window.set_always_on_top(true);
    #[cfg(target_os = "macos")]
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    // Launch-at-login. Uses the OS-native mechanism (macOS Login Items). The
    // toggle in Settings enables/disables it at runtime.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    }

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

            // Dedicated monochrome template icon (black-on-transparent). macOS
            // recolors template icons to match the menu bar in light/dark. The
            // @2x asset is embedded; macOS scales it for the ~22pt bar.
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!(
                "../icons/tray-icon@2x.png"
            ))
            .expect("tray icon");

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .icon_as_template(true) // render monochrome to match the menu bar
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => toggle_window(app, None),
                    "refresh" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let db = app.state::<Db>();
                            let token = app.state::<AppState>().jira_token();
                            let settings = {
                                let conn = db.0.lock().unwrap();
                                settings_repo::load(&conn)
                            };
                            let _ = sync::run_sync(&db, &settings, token).await;
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
                    let token = poll_handle.state::<AppState>().jira_token();
                    let settings = {
                        let conn = db.0.lock().unwrap();
                        settings_repo::load(&conn)
                    };
                    match sync::run_sync(&db, &settings, token).await {
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
            commands::get_autostart,
            commands::set_autostart,
            commands::hide_window,
            commands::ui_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

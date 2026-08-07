//! Popover window class: show/hide/pin for the tray-anchored panel.
//!
//! ## Why this module exists
//!
//! A plain `NSWindow` owned by an `Accessory` (LSUIElement) app cannot come
//! forward over a fullscreen or maximized app — an accessory app never truly
//! activates, so the window has nothing to rise above with. The previous
//! workaround promoted the whole app to `ActivationPolicy::Regular` on show and
//! back to `Accessory` on hide, which worked but flashed a Dock icon for as long
//! as the popover was open.
//!
//! The native answer — and what menu bar apps like JetBrains Toolbox actually
//! use — is an [`NSPanel`] with three specific AppKit attributes:
//!
//! - `styleMask` containing `.nonactivatingPanel` — the panel can take keyboard
//!   focus *without activating the app*, so there is nothing to promote and no
//!   Dock icon to hide.
//! - `collectionBehavior` of `.fullScreenAuxiliary | .canJoinAllSpaces` — grants
//!   permission to draw into whichever Space or fullscreen app is current.
//! - a floating window `level`, so it sits above ordinary windows.
//!
//! Tauri's window API can't express `NSPanel`, hence the `tauri-nspanel`
//! dependency. It swizzles the existing window's class in place, so the window
//! stays the same object: Tauri positioning, the webview, and IPC all still work.
//!
//! ## Two things to not "clean up"
//!
//! 1. **We never call the plugin's `set_event_handler`.** It *replaces* the
//!    window's `NSWindowDelegate`, which on a Tauri window is Tauri's own — and
//!    that delegate is what raises `WindowEvent::Focused(false)`, i.e. our entire
//!    hide-on-click-away behavior. Installing a panel event handler would trade
//!    the popover's dismissal for notifications we don't need.
//! 2. **Don't call Tauri's `set_always_on_top` or `set_visible_on_all_workspaces`
//!    on the panel.** Both write the same underlying AppKit properties we set
//!    here, and `set_visible_on_all_workspaces` sets `collectionBehavior` to
//!    `canJoinAllSpaces` *alone* — dropping `fullScreenAuxiliary` and silently
//!    reintroducing the exact bug this module fixes.
//!
//! ## Pinning
//!
//! Auto-hide-on-blur is what makes this feel like a menu bar popover, but it
//! fights you when you're typing a standup into it and tab away to check Jira.
//! Pinning suppresses only that blur-hide — the tray icon, Escape, and ⌘⇧J all
//! still dismiss it. Pin state is deliberately per-session (not in `AppSettings`):
//! it's a transient "keep this open while I work" gesture, not a preference.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the popover should survive losing focus. Session-only; see module docs.
static PINNED: AtomicBool = AtomicBool::new(false);

pub fn is_pinned() -> bool {
    PINNED.load(Ordering::Relaxed)
}

pub fn set_pinned(pinned: bool) {
    PINNED.store(pinned, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
mod imp {
    use tauri::{AppHandle, Manager};
    use tauri_nspanel::{
        tauri_panel, CollectionBehavior, ManagerExt, PanelLevel, StyleMask, WebviewWindowExt,
    };

    tauri_panel! {
        panel!(WorkdayPopover {
            config: {
                // The popover has real text inputs (Jira URL, standup date range,
                // the draft textarea), so it must be able to take key focus.
                // Nonactivating + key-capable is a supported combination — it's
                // how Spotlight behaves.
                can_become_key_window: true,
                can_become_main_window: false,
                is_floating_panel: true,
                // We hide on blur ourselves (and skip it when pinned); letting
                // AppKit also hide on app deactivation would ignore the pin.
                hides_on_deactivate: false
            }
        })
    }

    /// Convert the `main` window into a nonactivating, fullscreen-capable panel.
    ///
    /// Best-effort: on failure the window stays an ordinary `NSWindow`, which
    /// still shows normally — it just won't float over fullscreen apps. That's a
    /// worse popover, not a broken app, so it isn't worth aborting launch over.
    pub fn init(app: &AppHandle) {
        let Some(window) = app.get_webview_window("main") else { return };
        let Ok(panel) = window.to_panel::<WorkdayPopover>() else { return };

        panel.set_level(PanelLevel::Floating.value());

        // The load-bearing line: without `.nonactivatingPanel` the panel can't
        // take focus over a fullscreen app without activating the app first.
        // `empty()` is also `NSWindowStyleMask::Borderless` (0), which matches
        // the window's `decorations: false`.
        panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());

        panel.set_collection_behavior(
            CollectionBehavior::new()
                .full_screen_auxiliary()
                .can_join_all_spaces()
                .into(),
        );

        panel.set_hides_on_deactivate(false);
    }

    pub fn show(app: &AppHandle) {
        if let Ok(panel) = app.get_webview_panel("main") {
            // Orders front *and* makes key, so typing works immediately without
            // the app activating.
            panel.show_and_make_key();
        } else if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    pub fn hide(app: &AppHandle) {
        if let Ok(panel) = app.get_webview_panel("main") {
            panel.hide();
        } else if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }

    pub fn is_visible(app: &AppHandle) -> bool {
        if let Ok(panel) = app.get_webview_panel("main") {
            return panel.is_visible();
        }
        app.get_webview_window("main")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false)
    }
}

/// Non-macOS fallback: plain Tauri window calls, so the crate still builds
/// (and behaves sanely) off-platform. There is no panel concept to set up.
#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::{AppHandle, Manager};

    pub fn init(_app: &AppHandle) {}

    pub fn show(app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    pub fn hide(app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }

    pub fn is_visible(app: &AppHandle) -> bool {
        app.get_webview_window("main")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false)
    }
}

pub use imp::{hide, init, is_visible, show};

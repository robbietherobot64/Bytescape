//! Browser tab commands — creates native webviews within the main window.
//!
//! Each browser tab gets its own native webview (backed by WebKitGTK on Linux,
//! WebView2 on Windows, WKWebView on macOS). These webviews are positioned
//! within the main window's content area and managed by the React tab system.
//!
//! ## Security model
//!
//! External webviews are **sandboxed** from the Tauri IPC bridge:
//! - An initialization script removes `__TAURI__` and `__TAURI_INTERNALS__`
//!   so external pages cannot invoke Tauri commands.
//! - Navigation restricted to `http:` / `https:` schemes.
//! - `window.open()` intercepted and routed back to the React tab system.
//! - Each browser webview uses an isolated data directory.
//!
//! ## Lifecycle
//!
//! ```text
//! browser_create  → webview appears at (x,y,w,h), show()+focus()
//! browser_hide    → webview.hide() — invisible, preserves page state
//! browser_show    → webview.show()+focus() — restore visibility
//! browser_show_at → show()+position+size in one IPC round-trip
//! browser_close   → webview destroyed permanently
//! ```
//!
//! ## Related Files
//!
//! - `src/components/BrowserView.tsx` — React component that calls these commands
//! - `src/components/Omnibar.tsx` — URL bar driving `browser_navigate` / `browser_eval`
//! - `commands/mod.rs` — command registration in the Tauri invoke handler

use tauri::{AppHandle, Manager, WebviewUrl, LogicalPosition, LogicalSize, Emitter};

// ═══════════════════════════════════════════════════════════════════════════════
// Linux GTK Overlay — platform-specific widget tree surgery
// ═══════════════════════════════════════════════════════════════════════════════

// ─────────────────────────────────────────────────────────────────────────────
// Linux GTK Overlay — fixes the split-screen bug
//
// On Linux, Tauri's `window.add_child()` packs webviews into a vertical
// GtkBox with `expand=true`. Two webviews in the same box split the window
// 50/50, and set_position/set_size are no-ops on GtkBox children.
//
// Fix: restructure the GTK widget tree so the main webview is wrapped in a
// GtkOverlay, with a GtkFixed as the overlay layer. Browser webviews are
// reparented from the GtkBox into the GtkFixed for absolute positioning.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
mod gtk_overlay {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::{AppHandle, Manager};

    static SETUP_DONE: AtomicBool = AtomicBool::new(false);

    /// Restructure the GTK widget tree to support browser webview overlays.
    ///
    /// Before: `GtkBox [ MainWebView ]`
    /// After:  `GtkBox [ GtkOverlay [ MainWebView + GtkFixed("browser-fixed") ] ]`
    ///
    /// The GtkFixed has `pass_through=true` so mouse events on areas not covered
    /// by a browser webview pass through to the React main webview below.
    /// Browser webviews receive events via their own GDK/X11 windows.
    ///
    /// # Cross-System: GTK Interop
    /// Uses `Webview::with_webview()` to access the native WebKitGTK widget, then
    /// walks the GTK widget tree to restructure it. An `mpsc::channel` synchronizes
    /// the GTK main-thread closure back to the async Tauri command.
    pub fn ensure_overlay(app: &AppHandle) -> Result<(), String> {
        if SETUP_DONE.load(Ordering::Acquire) {
            return Ok(());
        }

        let main_wv = app
            .get_webview("main")
            .ok_or_else(|| "main webview not found".to_string())?;

        let (tx, rx) = std::sync::mpsc::channel();

        main_wv
            .with_webview(move |pv| {
                use gtk::prelude::*;
                let wv = pv.inner();

                // Walk up the widget tree to find the default GtkBox (vbox)
                let mut cursor: Option<gtk::Widget> = wv.parent();
                while let Some(ref widget) = cursor {
                    if widget.type_().name() == "GtkBox" {
                        let vbox: gtk::Box = widget.clone().downcast().unwrap();

                        // Already restructured from a previous session?
                        if vbox
                            .children()
                            .iter()
                            .any(|c: &gtk::Widget| c.type_().name() == "GtkOverlay")
                        {
                            SETUP_DONE.store(true, Ordering::Release);
                            let _ = tx.send(Ok(()));
                            return;
                        }

                        // Remove the main webview from the vbox
                        vbox.remove(&wv);

                        // Build: overlay(main=wv, overlay=GtkFixed)
                        let overlay = gtk::Overlay::new();
                        overlay.add(&wv);

                        let fixed = gtk::Fixed::new();
                        fixed.set_widget_name("browser-fixed");
                        overlay.add_overlay(&fixed);
                        // pass_through lets clicks on empty areas reach the React UI
                        overlay.set_overlay_pass_through(&fixed, true);

                        vbox.pack_start(&overlay, true, true, 0);
                        overlay.show_all();

                        SETUP_DONE.store(true, Ordering::Release);
                        let _ = tx.send(Ok(()));
                        return;
                    }
                    cursor = widget.parent();
                }

                let _ = tx.send(Err("GtkBox ancestor not found".to_string()));
            })
            .map_err(|e| format!("with_webview: {}", e))?;

        rx.recv()
            .map_err(|_| "GTK overlay setup: channel closed".to_string())?
    }

    /// After `add_child` puts a browser webview into the GtkBox, move it
    /// into the GtkFixed inside the overlay for absolute positioning.
    ///
    /// # Cross-System: GTK Interop
    /// Accesses the native GtkBox → GtkOverlay → GtkFixed widget chain via
    /// `with_webview()`. Blocks on an `mpsc::channel` until the GTK reparent completes.
    pub fn reparent_to_fixed(
        webview: &tauri::Webview<tauri::Wry>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();

        webview
            .with_webview(move |pv| {
                use gtk::prelude::*;
                let wv = pv.inner();

                // Current parent is the GtkBox (vbox) — remove from it
                let Some(parent) = wv.parent() else {
                    let _ = tx.send(Ok(()));
                    return;
                };
                let Ok(vbox) = parent.downcast::<gtk::Box>() else {
                    // Already in the right place or unexpected parent
                    let _ = tx.send(Ok(()));
                    return;
                };
                vbox.remove(&wv);

                // Find the GtkFixed inside the GtkOverlay
                for child in vbox.children() {
                    if child.type_().name() != "GtkOverlay" {
                        continue;
                    }
                    let overlay: gtk::Overlay = child.downcast().unwrap();
                    for oc in overlay.children() {
                        if oc.widget_name().as_str() == "browser-fixed" {
                            let fixed: gtk::Fixed = oc.downcast().unwrap();
                            wv.set_size_request(width, height);
                            fixed.put(&wv, x, y);
                            wv.show_all();
                            let _ = tx.send(Ok(()));
                            return;
                        }
                    }
                }

                let _ = tx.send(Err("GtkFixed 'browser-fixed' not found".to_string()));
            })
            .map_err(|e| format!("with_webview: {}", e))?;

        rx.recv()
            .map_err(|_| "reparent: channel closed".to_string())?
    }

    /// Reposition + resize a browser webview within the GtkFixed.
    /// Fire-and-forget (no blocking wait).
    ///
    /// # Cross-System: GTK Interop
    /// Uses `with_webview()` to call `GtkFixed::move_()` and `set_size_request()`
    /// on the native widget. Unlike `ensure_overlay`/`reparent_to_fixed`, this is
    /// fire-and-forget — errors are silently ignored for performance.
    pub fn set_bounds(
        webview: &tauri::Webview<tauri::Wry>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let _ = webview.with_webview(move |pv| {
            use gtk::prelude::*;
            let wv = pv.inner();
            if let Some(parent) = wv.parent() {
                if parent.type_().name() == "GtkFixed" {
                    let fixed: gtk::Fixed = parent.downcast().unwrap();
                    fixed.move_(&wv, x, y);
                    wv.set_size_request(width, height);
                }
            }
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Security & Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// JavaScript injected into every external webview before any page scripts run.
///
/// Removes `__TAURI__` and `__TAURI_INTERNALS__` from the global scope so that
/// external pages loaded in browser tabs cannot invoke Tauri IPC commands.
/// This is the primary sandbox boundary for untrusted web content.
const EXTERNAL_WEBVIEW_INIT_SCRIPT: &str = r#"
(function() {
    if (window.location.protocol !== 'http:' || window.location.hostname !== 'tauri.localhost') {
        delete window.__TAURI__;
        delete window.__TAURI_INTERNALS__;
        Object.defineProperty(window, '__TAURI__', { get() { return undefined; }, configurable: false });
        Object.defineProperty(window, '__TAURI_INTERNALS__', { get() { return undefined; }, configurable: false });
    }
})();
"#;

/// Check whether a URL is safe for browser webview navigation.
///
/// Only `http:` and `https:` schemes are allowed — this blocks `file:`,
/// `javascript:`, `data:`, and custom Tauri schemes from being loaded
/// in external webviews.
fn is_allowed_navigation(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

/// Returns the isolated data directory for browser webview storage.
///
/// Each browser session stores cookies, cache, and local storage in
/// `{app_data_dir}/browser_data/`, separate from the main Tauri webview.
fn browser_data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|p| p.join("browser_data"))
}

/// Resolve the main application window, with a fallback to any available window.
///
/// The primary lookup uses the `"main"` label. If that fails (can happen during
/// window recreation), falls back to the first window in the manager's list.
fn get_main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window("main")
        .or_else(|| {
            eprintln!("[browser] WARN: get_window(\"main\") returned None, trying fallback");
            app.windows().into_values().next()
        })
        .ok_or_else(|| {
            let available: Vec<String> = app.windows().keys().cloned().collect();
            format!("No window found (available: {:?})", available)
        })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tauri IPC Commands — browser tab lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a native browser webview within the main window.
///
/// The webview is positioned at (x, y) with the given dimensions,
/// overlaying the React content area for that tab. If a webview with the
/// same label already exists (e.g. React StrictMode double-mount), it is
/// repositioned and shown instead of creating a duplicate.
///
/// # Cross-System: Tauri Webview + GTK (Linux)
/// On Linux, this command triggers GTK widget tree surgery: `ensure_overlay()`
/// restructures the GtkBox, then `reparent_to_fixed()` moves the new webview
/// into a GtkFixed for absolute positioning. On other platforms, Tauri's
/// built-in `set_position`/`set_size` is used directly.
///
/// # IPC
/// Exposed as Tauri command `browser_create` — called from `BrowserView.tsx`.
#[tauri::command]
pub async fn browser_create(
    app: AppHandle,
    label: String,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // ── Duplicate guard ──
    // React StrictMode (dev) double-mounts components, so two concurrent
    // browser_create calls may arrive with the same label.
    if let Some(existing) = app.get_webview(&label) {
        #[cfg(target_os = "linux")]
        {
            gtk_overlay::set_bounds(&existing, x as i32, y as i32, width as i32, height as i32);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = existing.set_position(LogicalPosition::new(x, y));
            let _ = existing.set_size(LogicalSize::new(width, height));
        }
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let parsed_url: url::Url = url.parse()
        .map_err(|e: url::ParseError| format!("Invalid URL: {}", e))?;

    if !is_allowed_navigation(&parsed_url) {
        return Err(format!("Blocked: only http/https allowed, got {}", parsed_url.scheme()));
    }

    let window = get_main_window(&app)?;

    let app_for_title = app.clone();
    let label_for_title = label.clone();
    let app_for_nav = app.clone();
    let label_for_nav = label.clone();
    let app_for_new = app.clone();

    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        &label,
        WebviewUrl::External(parsed_url),
    )
    .on_navigation(|url| is_allowed_navigation(url))
    .initialization_script(EXTERNAL_WEBVIEW_INIT_SCRIPT)
    .on_document_title_changed(move |_wv, title| {
        let _ = app_for_title.emit(
            &format!("browser-title-{}", label_for_title),
            title,
        );
    })
    .on_page_load(move |_wv, payload| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
            let _ = app_for_nav.emit(
                &format!("browser-url-{}", label_for_nav),
                payload.url().to_string(),
            );
        }
    })
    .on_new_window(move |url, _features| {
        let _ = app_for_new.emit("browser-new-tab", url.to_string());
        tauri::webview::NewWindowResponse::Deny
    })
    .zoom_hotkeys_enabled(true)
    .enable_clipboard_access();

    if let Some(data_dir) = browser_data_dir(&app) {
        webview_builder = webview_builder.data_directory(data_dir);
    }

    // ── Linux: set up GTK overlay before add_child ──
    #[cfg(target_os = "linux")]
    gtk_overlay::ensure_overlay(&app)?;

    let webview = window.add_child(
        webview_builder,
        LogicalPosition::new(x, y),
        LogicalSize::new(width, height),
    ).map_err(|e| format!("add_child failed for '{}': {}", label, e))?;

    // ── Linux: reparent from GtkBox → GtkFixed overlay ──
    #[cfg(target_os = "linux")]
    {
        // Hide immediately to prevent the 50/50 split flash while still in GtkBox
        let _ = webview.hide();
        gtk_overlay::reparent_to_fixed(
            &webview,
            x as i32, y as i32, width as i32, height as i32,
        )?;
        let _ = webview.set_focus();
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = webview.show();
        let _ = webview.set_focus();
    }

    Ok(())
}

/// Destroy a browser webview (called when the tab is closed).
#[tauri::command]
pub async fn browser_close(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.close();
    }
    Ok(())
}

/// Update the position and size of a browser webview.
#[tauri::command]
pub async fn browser_set_bounds(
    app: AppHandle,
    label: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    #[cfg(target_os = "linux")]
    {
        gtk_overlay::set_bounds(&webview, x as i32, y as i32, width as i32, height as i32);
    }

    #[cfg(not(target_os = "linux"))]
    {
        webview.set_position(LogicalPosition::new(x, y))
            .map_err(|e| format!("set_position: {}", e))?;
        webview.set_size(LogicalSize::new(width, height))
            .map_err(|e| format!("set_size: {}", e))?;
    }

    Ok(())
}

/// Hide a browser webview.
///
/// On Linux (GtkFixed overlay), `hide()` is sufficient.
/// On other platforms, we also move off-screen as belt-and-suspenders.
#[tauri::command]
pub async fn browser_hide(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.hide();
        #[cfg(not(target_os = "linux"))]
        {
            let _ = webview.set_position(LogicalPosition::new(-10000.0_f64, -10000.0_f64));
        }
    }
    Ok(())
}

/// Show a hidden browser webview and give it focus.
///
/// Restores a webview that was previously hidden via `browser_hide`.
/// The frontend should follow up with `browser_set_bounds` or use
/// `browser_show_at` to also set the position in one call.
#[tauri::command]
pub async fn browser_show(app: AppHandle, label: String) -> Result<(), String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;
    webview.show().map_err(|e| format!("show: {}", e))?;
    webview.set_focus().map_err(|e| format!("set_focus: {}", e))?;
    Ok(())
}

/// Show a browser webview AND set its bounds in a single IPC call.
///
/// Combines positioning + show to avoid visible flicker.
/// On Linux, uses GTK-level positioning (GtkFixed).
/// On other platforms, uses Tauri's set_position/set_size.
#[tauri::command]
pub async fn browser_show_at(
    app: AppHandle,
    label: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    #[cfg(target_os = "linux")]
    {
        gtk_overlay::set_bounds(&webview, x as i32, y as i32, width as i32, height as i32);
    }

    #[cfg(not(target_os = "linux"))]
    {
        webview.set_position(LogicalPosition::new(x, y))
            .map_err(|e| format!("set_position: {}", e))?;
        webview.set_size(LogicalSize::new(width, height))
            .map_err(|e| format!("set_size: {}", e))?;
    }

    webview.show().map_err(|e| format!("show: {}", e))?;
    webview.set_focus().map_err(|e| format!("set_focus: {}", e))?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tauri IPC Commands — browser navigation & utilities
// ═══════════════════════════════════════════════════════════════════════════════

/// Navigate an existing browser webview to a new URL.
///
/// Validates the URL scheme before navigating — only `http:` and `https:` are
/// allowed. Returns an error if the webview doesn't exist or the URL is blocked.
///
/// # IPC
/// Exposed as Tauri command `browser_navigate` — called from `Omnibar.tsx`.
#[tauri::command]
pub async fn browser_navigate(
    app: AppHandle,
    label: String,
    url: String,
) -> Result<(), String> {
    let parsed: url::Url = url.parse()
        .map_err(|e: url::ParseError| format!("Invalid URL: {}", e))?;

    if !is_allowed_navigation(&parsed) {
        return Err(format!("Blocked: only http/https URLs are allowed, got {}", parsed.scheme()));
    }

    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    webview.navigate(parsed)
        .map_err(|e| format!("navigate: {}", e))?;
    Ok(())
}

/// Execute JavaScript in a browser webview.
///
/// Powers back/forward/reload actions from the Omnibar.
#[tauri::command]
pub async fn browser_eval(
    app: AppHandle,
    label: String,
    js: String,
) -> Result<(), String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    webview.eval(&js)
        .map_err(|e| format!("eval: {}", e))?;
    Ok(())
}

/// Get the current URL of a browser webview.
#[tauri::command]
pub async fn browser_get_url(
    app: AppHandle,
    label: String,
) -> Result<String, String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;

    webview.url()
        .map(|u| u.to_string())
        .map_err(|e| format!("url: {}", e))
}

/// Clear all browser data (cookies, cache, storage).
#[tauri::command]
pub async fn browser_clear_data(app: AppHandle) -> Result<(), String> {
    if let Some(data_dir) = browser_data_dir(&app) {
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir)
                .map_err(|e| format!("Failed to clear browser data: {}", e))?;
        }
    }
    Ok(())
}

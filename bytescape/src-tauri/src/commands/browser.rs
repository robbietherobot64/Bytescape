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

use tauri::{AppHandle, Manager, WebviewUrl, LogicalPosition, LogicalSize, Emitter};

/// JavaScript injected into every external webview before any page scripts run.
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

fn is_allowed_navigation(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn browser_data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|p| p.join("browser_data"))
}

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

/// Create a native browser webview within the main window.
///
/// The webview is positioned at (x, y) with the given dimensions,
/// overlaying the React content area for that tab. If a webview with the
/// same label already exists (e.g. React StrictMode double-mount), it is
/// repositioned and shown instead of creating a duplicate.
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
        let _ = existing.set_position(LogicalPosition::new(x, y));
        let _ = existing.set_size(LogicalSize::new(width, height));
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

    let webview = window.add_child(
        webview_builder,
        LogicalPosition::new(x, y),
        LogicalSize::new(width, height),
    ).map_err(|e| format!("add_child failed for '{}': {}", label, e))?;

    let _ = webview.show();
    let _ = webview.set_focus();
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
    webview.set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("set_position: {}", e))?;
    webview.set_size(LogicalSize::new(width, height))
        .map_err(|e| format!("set_size: {}", e))?;
    Ok(())
}

/// Hide a browser webview.
///
/// Uses the real `hide()` API so the webview is truly invisible and
/// does not consume paint resources. Falls back to off-screen positioning
/// if `hide()` fails on a given platform.
#[tauri::command]
pub async fn browser_hide(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        if webview.hide().is_err() {
            // Fallback: move off-screen if hide() not supported
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
/// Combines `browser_set_bounds` + `browser_show` to avoid a visible
/// flicker where the webview appears at its old position before being
/// repositioned. The sequence is: position → size → show → focus.
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
    webview.set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("set_position: {}", e))?;
    webview.set_size(LogicalSize::new(width, height))
        .map_err(|e| format!("set_size: {}", e))?;
    webview.show().map_err(|e| format!("show: {}", e))?;
    webview.set_focus().map_err(|e| format!("set_focus: {}", e))?;
    Ok(())
}

/// Navigate an existing browser webview to a new URL.
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

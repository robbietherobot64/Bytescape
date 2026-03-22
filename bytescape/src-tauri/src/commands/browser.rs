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
//! ## Threading model
//!
//! `browser_create` is an **async** command, so Tauri spawns it on Tokio.
//! This means the GTK main thread is free to process messages. Internally,
//! `window.add_child()` sends a message to the GTK event loop and blocks
//! until the widget is created. Since we're on a Tokio worker (not the main
//! thread), this should not deadlock.

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
            eprintln!("[browser] get_window(\"main\") returned None, trying fallback");
            app.windows().into_values().next()
        })
        .ok_or_else(|| {
            let available: Vec<String> = app.windows().keys().cloned().collect();
            format!("No window found (available: {:?})", available)
        })
}

/// Create a native browser webview within the main window.
///
/// This is an **async** command — Tauri spawns it on Tokio, freeing the GTK
/// main thread to process the `add_child` message internally.
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
    eprintln!("[browser_create] ENTER label={} url={} thread={:?}",
        label, url, std::thread::current().id());

    // ── Validate URL ──
    let parsed_url: url::Url = url.parse()
        .map_err(|e: url::ParseError| {
            eprintln!("[browser_create] URL parse error: {}", e);
            format!("Invalid URL: {}", e)
        })?;

    if !is_allowed_navigation(&parsed_url) {
        let msg = format!("Blocked: only http/https allowed, got {}", parsed_url.scheme());
        eprintln!("[browser_create] {}", msg);
        return Err(msg);
    }
    eprintln!("[browser_create] URL validated OK");

    // ── Get main window ──
    let window = get_main_window(&app)?;
    eprintln!("[browser_create] main window found: {:?}", window.label());

    // ── Build the WebviewBuilder ──
    eprintln!("[browser_create] building WebviewBuilder...");

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
        eprintln!("[browser_create] using data_dir: {:?}", data_dir);
        webview_builder = webview_builder.data_directory(data_dir);
    }

    // ── Create the webview ──
    eprintln!("[browser_create] >>> calling window.add_child()...");
    let webview = window.add_child(
        webview_builder,
        LogicalPosition::new(x, y),
        LogicalSize::new(width, height),
    ).map_err(|e| {
        let msg = format!("add_child failed: {}", e);
        eprintln!("[browser_create] <<< {}", msg);
        msg
    })?;
    eprintln!("[browser_create] <<< add_child returned OK");

    // ── Show and focus ──
    eprintln!("[browser_create] calling show()...");
    if let Err(e) = webview.show() {
        eprintln!("[browser_create] show() error (non-fatal): {}", e);
    }

    eprintln!("[browser_create] calling set_focus()...");
    if let Err(e) = webview.set_focus() {
        eprintln!("[browser_create] set_focus() error (non-fatal): {}", e);
    }

    eprintln!("[browser_create] DONE — webview '{}' created successfully", label);
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
///
/// Called when the tab becomes active or the window resizes,
/// to keep the webview aligned with the React content area.
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

/// Hide a browser webview by moving it off-screen.
///
/// Called when the user switches away from a browser tab.
/// The webview stays alive (preserving page state) but is invisible.
#[tauri::command]
pub async fn browser_hide(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.set_position(LogicalPosition::new(-10000.0_f64, -10000.0_f64));
    }
    Ok(())
}

/// Navigate an existing browser webview to a new URL.
///
/// Used by the Omnibar when the user types a URL while a webview tab is
/// active, or when following an in-page redirect.
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

/// Execute JavaScript in a browser webview and return the result.
///
/// Powers back/forward/reload actions from the Omnibar and
/// any other frontend-initiated DOM queries.
///
/// **Safety:** This only runs JS in an already-sandboxed external webview
/// (no Tauri IPC bridge), so the injected script cannot escalate privileges.
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
///
/// Returns the live URL (which may differ from the originally-opened URL
/// after in-page navigation).
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
///
/// Called on account disconnect or explicit user action to ensure no
/// session data leaks between accounts.
#[tauri::command]
pub async fn browser_clear_data(app: AppHandle) -> Result<(), String> {
    if let Some(data_dir) = browser_data_dir(&app) {
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir)
                .map_err(|e| format!("Failed to clear browser data: {}", e))?;
            eprintln!("browser_clear_data: cleared {}", data_dir.display());
        }
    }
    Ok(())
}

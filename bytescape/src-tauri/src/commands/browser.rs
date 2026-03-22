//! Browser tab commands — creates native webviews within the main window.
//!
//! Each browser tab gets its own native webview (backed by WebKitGTK on Linux,
//! WebView2 on Windows, WKWebView on macOS). These webviews are positioned
//! within the main window's content area and managed by the React tab system.
//!
//! This bypasses iframe X-Frame-Options restrictions because the webviews are
//! full native browser instances, not sandboxed iframes.
//!
//! ## Security model
//!
//! External webviews are **sandboxed** from the Tauri IPC bridge:
//! - An initialization script removes `__TAURI__` and `__TAURI_INTERNALS__`
//!   so external pages cannot invoke Tauri commands.
//! - Navigation is restricted to `http:` / `https:` schemes — `file://`,
//!   `javascript:`, `data:`, and `blob:` navigations are blocked.
//! - `window.open()` / `target="_blank"` are intercepted and routed back to
//!   the React tab system via events instead of spawning OS popups.
//! - Each browser webview uses an isolated data directory so cookies and
//!   storage from external sites cannot access the main app's session data.

use tauri::{AppHandle, Manager, WebviewUrl, LogicalPosition, LogicalSize, Emitter};

/// JavaScript injected into every external webview before any page scripts run.
///
/// Strips the Tauri IPC bridge so external pages can never call `invoke()` or
/// listen for events. This is the primary defence against a compromised or
/// malicious page reaching into the host app.
const EXTERNAL_WEBVIEW_INIT_SCRIPT: &str = r#"
(function() {
    // Strip Tauri IPC from external contexts
    if (window.location.protocol !== 'http:' || window.location.hostname !== 'tauri.localhost') {
        delete window.__TAURI__;
        delete window.__TAURI_INTERNALS__;
        Object.defineProperty(window, '__TAURI__', { get() { return undefined; }, configurable: false });
        Object.defineProperty(window, '__TAURI_INTERNALS__', { get() { return undefined; }, configurable: false });
    }
})();
"#;

/// Schemes that browser webviews are allowed to navigate to.
///
/// Everything else (`file://`, `javascript:`, `data:`, `blob:`) is blocked.
fn is_allowed_navigation(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

/// Get (or create) the isolated data directory for browser webviews.
///
/// All external webviews share one data directory that is separate from the
/// main app's webview data, so cookies / localStorage / IndexedDB from
/// visited sites cannot access the Bytescape session data.
fn browser_data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|p| p.join("browser_data"))
}

/// Resolve the main Tauri window.
///
/// Tries `"main"` first (the default label from tauri.conf.json), then falls
/// back to the first available window. Returns an error string if no window
/// exists.
fn get_main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window("main")
        .or_else(|| {
            eprintln!("browser: get_window(\"main\") returned None, trying fallback");
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
/// overlaying the React content area for that tab.
///
/// ## Async design
///
/// On Linux, `window.add_child()` dispatches to the GTK main thread and
/// blocks until it completes. If called directly inside a Tauri async command
/// handler, this deadlocks — the GTK event loop can't process the request
/// because it's still handling the IPC message from this very command.
///
/// To break the deadlock, this command **returns `Ok(())` immediately** after
/// validating the URL. The actual webview creation is spawned in a separate
/// async task. When the webview is ready, it emits `browser-ready-{label}`.
/// If creation fails, it emits `browser-error-{label}` with the error message.
///
/// ## Security
/// - Navigation restricted to `http:`/`https:` only.
/// - Tauri IPC bridge stripped via initialization script.
/// - Isolated data directory for cookies/storage.
/// - `window.open()` intercepted → emits `browser-new-tab` event.
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
    // Validate URL synchronously — this can return an error to the caller.
    let parsed_url: url::Url = url.parse()
        .map_err(|e: url::ParseError| format!("Invalid URL: {}", e))?;

    if !is_allowed_navigation(&parsed_url) {
        return Err(format!("Blocked: only http/https URLs are allowed, got {}", parsed_url.scheme()));
    }

    // Verify the main window exists before spawning.
    let _ = get_main_window(&app)?;

    eprintln!("browser_create: accepted url={}, spawning async creation for label={}", url, label);

    // Spawn the actual webview creation in a separate task.
    // This returns Ok(()) to the frontend immediately, freeing the GTK
    // event loop so window.add_child() can complete without deadlocking.
    let app_handle = app.clone();
    let label_clone = label.clone();
    tauri::async_runtime::spawn(async move {
        let label = label_clone;
        let app = app_handle;

        let window = match get_main_window(&app) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("browser_create [async]: window error: {}", e);
                let _ = app.emit(&format!("browser-error-{}", label), e);
                return;
            }
        };

        // Clone app handles for the event callbacks
        let app_for_title = app.clone();
        let label_for_title = label.clone();
        let app_for_nav = app.clone();
        let label_for_nav = label.clone();
        let app_for_new = app.clone();
        let app_for_ready = app.clone();
        let label_for_ready = label.clone();

        let mut webview_builder = tauri::webview::WebviewBuilder::new(
            &label,
            WebviewUrl::External(parsed_url),
        )
        .on_navigation(|url| is_allowed_navigation(url))
        .initialization_script(EXTERNAL_WEBVIEW_INIT_SCRIPT)
        .on_document_title_changed(move |_webview, title| {
            let _ = app_for_title.emit(
                &format!("browser-title-{}", label_for_title),
                title,
            );
        })
        .on_page_load(move |_webview, payload| {
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

        match window.add_child(
            webview_builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width, height),
        ) {
            Ok(webview) => {
                // show() and set_focus() are synchronous GTK calls — safe here
                // because we're in a spawned task, not blocking the IPC handler.
                let _ = webview.show();
                let _ = webview.set_focus();
                eprintln!("browser_create [async]: webview '{}' created, shown, focused", label);
                let _ = app_for_ready.emit(&format!("browser-ready-{}", label_for_ready), "ok");
            }
            Err(e) => {
                let msg = format!("Failed to create webview '{}': {}", label, e);
                eprintln!("browser_create [async]: {}", msg);
                let _ = app.emit(&format!("browser-error-{}", label), msg);
            }
        }
    });

    // Return immediately — the frontend will listen for the ready/error events.
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

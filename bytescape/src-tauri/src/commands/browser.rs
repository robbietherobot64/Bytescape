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
//!
//! ## Threading model
//!
//! On Linux, WebKitGTK widgets **must** be created on the GTK main thread.
//! Tauri runs command handlers on a thread pool (async commands on Tokio,
//! sync commands on `spawn_blocking`). Attempting to call `window.add_child()`
//! from either type of worker thread will panic or deadlock.
//!
//! To solve this, `browser_create` is a **synchronous** command that uses
//! `app.run_on_main_thread()` to dispatch the actual GTK work to the main
//! thread, then receives the result via a `std::sync::mpsc` channel. The
//! calling (blocking) thread waits for the result without blocking the GTK
//! event loop.

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
/// This is a **synchronous** command. Tauri dispatches it onto a blocking
/// worker thread (via `spawn_blocking`), which is perfectly fine for our
/// purposes — we're going to block it anyway waiting for the GTK main thread.
///
/// ## Flow
///
/// 1. **Validate URL** on the worker thread (fast, no GTK needed).
/// 2. **Dispatch to GTK main thread** via `app.run_on_main_thread()`.
///    Inside the closure we build the `WebviewBuilder`, call
///    `window.add_child()`, `show()`, and `set_focus()`.
/// 3. **Send result** through a `mpsc` channel back to the worker thread.
/// 4. **Return** `Ok(())` or `Err(...)` to the frontend.
///
/// This avoids every previous failure mode:
/// - No `tauri::async_runtime::spawn` (panics on GTK ops were swallowed).
/// - No direct `add_child` from a Tokio thread (deadlock or panic).
/// - The GTK main thread is free because Tauri's IPC dispatch is non-blocking.
///
/// ## Security
/// - Navigation restricted to `http:`/`https:` only.
/// - Tauri IPC bridge stripped via initialization script.
/// - Isolated data directory for cookies/storage.
/// - `window.open()` intercepted → emits `browser-new-tab` event.
#[tauri::command]
pub fn browser_create(
    app: AppHandle,
    label: String,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // ── Step 1: Validate URL (runs on blocking worker thread) ──
    let parsed_url: url::Url = url.parse()
        .map_err(|e: url::ParseError| format!("Invalid URL: {}", e))?;

    if !is_allowed_navigation(&parsed_url) {
        return Err(format!(
            "Blocked: only http/https URLs are allowed, got {}",
            parsed_url.scheme()
        ));
    }

    // Quick sanity check that a window exists.
    let _ = get_main_window(&app)?;

    eprintln!(
        "browser_create: url={}, label={}, dispatching to main thread",
        url, label
    );

    // ── Step 2: Channel for result from GTK main thread ──
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();

    // We need two AppHandle clones:
    // - `app_dispatch` calls run_on_main_thread (consumed by the call)
    // - `app_inner` is moved into the closure for GTK work + event callbacks
    let app_dispatch = app.clone();
    let app_inner = app.clone();

    // ── Step 3: Dispatch to GTK main thread ──
    app_dispatch.run_on_main_thread(move || {
        // Everything in this closure runs on the GTK main thread.
        // Wrap in catch_unwind so a panic doesn't kill the main thread.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            create_webview_on_main_thread(
                &app_inner, &label, parsed_url, x, y, width, height,
            )
        }));

        let send_result = match result {
            Ok(inner) => inner,
            Err(panic_info) => {
                let msg = format!("Panic during webview creation: {:?}", panic_info);
                eprintln!("browser_create [main thread]: {}", msg);
                Err(msg)
            }
        };

        // Send result to the waiting worker thread.
        let _ = tx.send(send_result);
    }).map_err(|e| format!("Failed to dispatch to main thread: {}", e))?;

    // ── Step 4: Wait for the result ──
    // This blocks the worker thread (which is designed for blocking) until
    // the GTK main thread finishes. The GTK main thread is free because
    // Tauri's IPC dispatch is non-blocking — it already returned to the
    // event loop after spawning this worker.
    match rx.recv() {
        Ok(result) => result,
        Err(_) => Err("Main thread closure dropped without sending result".to_string()),
    }
}

/// Actually build and attach the webview. Runs on the GTK main thread.
fn create_webview_on_main_thread(
    app: &AppHandle,
    label: &str,
    parsed_url: url::Url,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = get_main_window(app)?;

    // Clone app handles for the event callbacks (called later from GTK thread
    // during page load events — AppHandle is Send+Sync so this is safe).
    let app_for_title = app.clone();
    let label_for_title = label.to_string();
    let app_for_nav = app.clone();
    let label_for_nav = label.to_string();
    let app_for_new = app.clone();

    let mut webview_builder = tauri::webview::WebviewBuilder::new(
        label,
        WebviewUrl::External(parsed_url),
    )
    // Block non-http(s) navigations — prevents file:// / javascript:// exploits
    .on_navigation(|url| is_allowed_navigation(url))
    // Strip Tauri IPC so external pages cannot call invoke()
    .initialization_script(EXTERNAL_WEBVIEW_INIT_SCRIPT)
    // Emit title changes so the React tab bar can update
    .on_document_title_changed(move |_webview, title| {
        let _ = app_for_title.emit(
            &format!("browser-title-{}", label_for_title),
            title,
        );
    })
    // Emit URL changes so the Omnibar can track navigation
    .on_page_load(move |_webview, payload| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
            let _ = app_for_nav.emit(
                &format!("browser-url-{}", label_for_nav),
                payload.url().to_string(),
            );
        }
    })
    // Intercept window.open / target="_blank" links → emit event for React
    // to open them as new in-app tabs instead of spawning OS windows
    .on_new_window(move |url, _features| {
        let _ = app_for_new.emit("browser-new-tab", url.to_string());
        tauri::webview::NewWindowResponse::Deny
    })
    .zoom_hotkeys_enabled(true)
    .enable_clipboard_access();

    // Isolate browser cookies/storage from the main app webview
    if let Some(data_dir) = browser_data_dir(app) {
        webview_builder = webview_builder.data_directory(data_dir);
    }

    let webview = window.add_child(
        webview_builder,
        LogicalPosition::new(x, y),
        LogicalSize::new(width, height),
    ).map_err(|e| {
        let msg = format!("add_child failed for '{}': {}", label, e);
        eprintln!("browser_create [main thread]: {}", msg);
        msg
    })?;

    // show() and set_focus() are synchronous GTK calls — we're already on
    // the main thread so they execute immediately.
    if let Err(e) = webview.show() {
        eprintln!("browser_create: show() failed (non-fatal): {}", e);
    }
    if let Err(e) = webview.set_focus() {
        eprintln!("browser_create: set_focus() failed (non-fatal): {}", e);
    }

    eprintln!(
        "browser_create [main thread]: webview '{}' created, shown, focused",
        label
    );
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

//! Browser tab commands — creates native webviews within the main window.
//!
//! Each browser tab gets its own native webview (backed by WebKitGTK on Linux,
//! WebView2 on Windows, WKWebView on macOS). These webviews are positioned
//! within the main window's content area and managed by the React tab system.
//!
//! This bypasses iframe X-Frame-Options restrictions because the webviews are
//! full native browser instances, not sandboxed iframes.

use tauri::{AppHandle, Manager, WebviewUrl, LogicalPosition, LogicalSize, Emitter};

/// Create a native browser webview within the main window.
///
/// The webview is positioned at (x, y) with the given dimensions,
/// overlaying the React content area for that tab.
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
    let parsed_url: url::Url = url.parse()
        .map_err(|e: url::ParseError| format!("Invalid URL: {}", e))?;

    // Debug: log available windows to help diagnose label mismatches
    let available_windows: Vec<String> = app.windows().keys().cloned().collect();
    eprintln!("browser_create: available windows: {:?}", available_windows);

    // Find the main window — label defaults to "main" from tauri.conf.json
    let window = app.get_window("main")
        .or_else(|| {
            eprintln!("browser_create: get_window(\"main\") returned None, trying fallback");
            // Fallback: use the first available window
            app.windows().into_values().next()
        })
        .ok_or_else(|| {
            let msg = format!("No window found (available: {:?})", available_windows);
            eprintln!("browser_create: {}", msg);
            msg
        })?;

    eprintln!("browser_create: using window label={:?}, creating webview label={:?} url={}", window.label(), label, url);

    // Clone app handles for the event callbacks
    let app_for_title = app.clone();
    let label_for_title = label.clone();
    let app_for_nav = app.clone();
    let label_for_nav = label.clone();
    let app_for_new = app.clone();

    let webview_builder = tauri::webview::WebviewBuilder::new(
        &label,
        WebviewUrl::External(parsed_url),
    )
    // Allow all navigation — this is a browser, not a locked-down webview
    .on_navigation(|_url| true)
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

    window.add_child(
        webview_builder,
        LogicalPosition::new(x, y),
        LogicalSize::new(width, height),
    ).map_err(|e| {
        let msg = format!("Failed to create webview '{}': {}", label, e);
        eprintln!("browser_create: {}", msg);
        msg
    })?;

    Ok(())
}

/// Destroy a browser webview (called when the tab is closed).
#[tauri::command]
pub async fn browser_close(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        // Ignore errors — the webview may already be closing
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
        // Move far off-screen to hide without destroying
        let _ = webview.set_position(LogicalPosition::new(-10000.0_f64, -10000.0_f64));
    }
    Ok(())
}

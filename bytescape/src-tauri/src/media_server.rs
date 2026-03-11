use warp::Filter;
use std::path::PathBuf;

/// Starts a dedicated Warp static file server bound to 127.0.0.1 on a random port.
/// Returns the port number it successfully bound to.
pub async fn start_warp_media_server(app_data_dir: PathBuf) -> u16 {
    let cache_dir = app_data_dir.join("blob_cache");

    // Ensure the cache directory exists before serving it
    if !cache_dir.exists() {
        let _ = std::fs::create_dir_all(&cache_dir);
    }

    // Serve files natively out of the blob cache, injecting CORS headers automatically.
    // CRIT-5: Restrict CORS to Tauri origins only — prevents arbitrary browser
    // tabs/extensions from reading cached media via the localhost server.
    let cors = warp::cors()
        .allow_origins(vec![
            "tauri://localhost",         // Desktop (Linux/macOS/Windows)
            "https://tauri.localhost",    // Android WebView
            "http://localhost:1420",      // Dev mode (Vite HMR)
        ])
        .allow_methods(vec!["GET", "OPTIONS", "HEAD"])
        .allow_headers(vec!["range", "content-type", "accept"])
        .max_age(std::time::Duration::from_secs(86400)); // 24 hours

    let route = warp::fs::dir(cache_dir.clone())
        .with(cors);

    // Reserve a random port by binding a TcpListener, then immediately start
    // the warp server on that port. The TOCTOU window is negligible since this
    // is localhost-only and the listener is dropped right before warp binds.
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind a random port for media server");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    println!("[Warp Media Server] Starting on http://127.0.0.1:{} serving {:?}", port, cache_dir);

    // Run the server completely independently in the Tokio background runtime
    tokio::spawn(warp::serve(route).run(([127, 0, 0, 1], port)));

    port
}

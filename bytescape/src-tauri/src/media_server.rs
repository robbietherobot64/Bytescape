//! # media_server.rs — Local Media Server for WebView
//!
//! Starts a localhost HTTP server (using Warp) that serves cached media files
//! (images, videos, audio) to the frontend WebView. This is necessary because
//! Tauri's WebView cannot directly access files on the filesystem — it needs
//! an HTTP URL to load media.
//!
//! ## How It Works
//!
//! 1. A Warp server binds to `127.0.0.1` on a random available port
//! 2. It serves files from the `blob_cache/` directory inside the app data folder
//! 3. The frontend loads media via `http://127.0.0.1:{port}/filename`
//! 4. CORS headers restrict access to Tauri origins only (CRIT-5 security)
//!
//! ## Cache Strategy
//! - **Videos**: `no-store` — prevents GStreamer double-buffering (each pipeline +
//!   HTTP cache = 2× memory). Re-request cost is zero since it's localhost.
//! - **Images**: `max-age=300, immutable` — WebKitGTK caches the decoded bitmap
//!   across virtualizer remount cycles, preventing multi-GB RSS spikes from
//!   re-decoding full-res images during scroll.
//!
//! ## Security (CRIT-5)
//! CORS is locked to Tauri origins (`tauri://localhost`, `https://tauri.localhost`,
//! `http://localhost:1420` for dev). This prevents arbitrary browser tabs or
//! extensions from reading cached media via the localhost server.
//!
//! ## Related Files
//! - `sync.rs` — The blob store that populates the `blob_cache/` directory
//! - `lib.rs` — Starts this server during app initialization, stores the port in AppState
//! - `blob_hash.rs` — Parses blob hash strings used as filenames in the cache

use warp::Filter;
use warp::Reply;
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
        .map(|file: warp::fs::File| {
            // MEMORY-CRITICAL: no-store for ALL media types.
            // Videos: prevents GStreamer double-buffering (pipeline + HTTP cache = 2×).
            // Images: prevents WebKitGTK from caching decoded bitmaps (48 MB each
            //         full-res image). With hundreds of images in a feed, the old
            //         max-age=300 policy caused multi-GB bitmap cache growth.
            // Re-request cost is zero since we serve from localhost disk cache.
            let mut resp = file.into_response();

            // MEMORY-CRITICAL: ALL media types use no-store.
            // Previously images used max-age=300,immutable which caused
            // WebKitGTK to accumulate decoded bitmaps (48 MB each for
            // full-res images). Over hundreds of images in a feed, this
            // grew to multi-GB. The blob_cache/ on disk prevents redundant
            // Iroh fetches, so HTTP caching is unnecessary.
            let cache_val = "no-store, no-cache, must-revalidate";

            resp.headers_mut().insert(
                "cache-control",
                cache_val.parse().unwrap(),
            );
            resp.headers_mut().insert(
                "connection",
                "close".parse().unwrap(),
            );
            resp
        })
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

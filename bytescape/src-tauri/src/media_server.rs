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
//! - **Videos**: `max-age=3600, immutable` — blob cache files are content-addressed
//!   (hash-named) so immutable is always correct. Caching prevents GStreamer from
//!   re-initializing its demuxer pipeline on every seek or rebuffer, which was the
//!   #1 cause of video stutter. Localhost serving means zero security risk.
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
            let mut resp = file.into_response();
            // All blob_cache files are content-addressed (hash-named) so
            // immutable is always correct. Videos get a longer max-age because
            // re-fetching on seek/rebuffer forces GStreamer to re-init its
            // demuxer pipeline, which is the #1 cause of video stutter.
            let is_video = resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|ct| ct.starts_with("video/"))
                .unwrap_or(false);

            let cache_val = if is_video {
                // Allow WebKitGTK to cache the video in memory. Seeking and
                // rebuffering reuse the cached response instead of triggering
                // a new HTTP round-trip + demuxer init.
                "public, max-age=3600, immutable"
            } else {
                "public, max-age=300, immutable"
            };

            resp.headers_mut().insert(
                "cache-control",
                cache_val.parse().unwrap(),
            );
            // Accept-Ranges is already supported by warp::fs natively, but
            // setting the header explicitly ensures WebKitGTK's GStreamer
            // pipeline knows it can do byte-range seeks without re-downloading.
            resp.headers_mut().insert(
                "accept-ranges",
                "bytes".parse().unwrap(),
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

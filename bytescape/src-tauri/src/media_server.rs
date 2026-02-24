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

    // Serve files natively out of the blob cache, injecting CORS headers automatically
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "OPTIONS", "HEAD"])
        .allow_headers(vec!["range", "content-type", "accept"])
        .max_age(std::time::Duration::from_secs(86400)); // 24 hours

    let route = warp::fs::dir(cache_dir.clone())
        .with(cors);

    // Find a free port using standard library
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Free it so warp can bind it

    println!("[Warp Media Server] Successfully bound to http://127.0.0.1:{} serving {:?}", port, cache_dir);

    // Run the server completely independently in the Tokio background runtime
    tokio::spawn(async move {
        warp::serve(route).run(([127, 0, 0, 1], port)).await;
    });

    port
}

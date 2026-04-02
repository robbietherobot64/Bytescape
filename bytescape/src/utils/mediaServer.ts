import { invoke } from '@tauri-apps/api/core';

/**
 * PURPOSE
 * Resolves local media-server URLs for blob cache files in the Tauri app.
 *
 * HOW IT WORKS
 * Fetches the backend-provided localhost port once, caches it in module state,
 * then maps absolute blob-cache file paths to `http://127.0.0.1:{port}/{name}`.
 *
 * RELATED FILES
 * - src-tauri/src/lib.rs (command: `get_media_server_port`)
 * - src/components/media renderers that require browser-loadable URLs
 */
/**
 * Cached media server port.
 *
 * The Warp media server runs on localhost to serve blob_cache files via plain
 * HTTP. This bypasses WebKitGTK's broken `asset://` protocol on Linux.
 */
let globalMediaPort: number | null = null;

/**
 * Returns the port of the local Warp media server.
 * Caches the result after the first invocation.
 *
 * Caveat:
 * Cache lifetime is process/module lifetime. If the backend port changes during
 * runtime, callers must reload process state to observe a new value.
 */
export async function getMediaPort(): Promise<number> {
    if (globalMediaPort) return globalMediaPort;
    globalMediaPort = await invoke<number>('get_media_server_port');
    return globalMediaPort;
}

/**
 * Converts a blob cache file path to an HTTP URL served by the local Warp
 * media server. Works around the broken `asset://` protocol on Linux/WebKitGTK.
 *
 * @param filePath - Absolute path to a file in blob_cache/
 * @returns HTTP URL like `http://127.0.0.1:{port}/{filename}`
 *
 * Caveat:
 * URL identity is based on basename only; different directories with the same
 * filename are indistinguishable to this URL shape.
 */
export async function blobCacheUrl(filePath: string): Promise<string> {
    const filename = filePath.split(/[/\\]/).pop();
    if (!filename) throw new Error(`Cannot extract filename from: ${filePath}`);
    const port = await getMediaPort();
    return `http://127.0.0.1:${port}/${filename}`;
}

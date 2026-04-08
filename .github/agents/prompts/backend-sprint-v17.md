# Backend Sprint v17 — Cross-Platform Repost Architecture & Feed Density

**Sprint**: v17  
**Track**: A (Backend — Rust)  
**Date**: 2026-04-08  
**Previous sprint**: v16 (video stability — delivered, user confirmed "mostly smooth")  
**HEAD at prompt time**: `d70c04f`

---

## Sprint Context

Sprint v16 delivered:
- ✅ Video playback stability (HLS variant selection, FFmpeg strategy reorder, resolve lock guard, cache key versioning)
- ✅ Memory leak eradication (streaming blob exports, 500MB LRU cache eviction, 120s FFmpeg timeouts)
- ✅ CPU spike on launch (async `seed_default_topics`)
- ✅ Instagram video import (header-only MIME detection)

User feedback on 2026-04-08 confirms video is "mostly smooth" and removed memory/CPU issues from critical priorities. New critical items have emerged.

---

## P0: Cross-Platform Repost — AT-URI Validation & Quote-Post Command

### Problem

When user reposts a Mastodon post to Bluesky, the frontend sends a Mastodon HTTPS URL (e.g., `https://indieweb.social/statuses/11636119590771…`) as the `uri` parameter to `repost_post`. Bluesky's API rejects this with:

```
400 Bad Request: Invalid app.bsky.feed.repost record: Invalid at-uri
```

**Root cause**: `repost_post` (in `src-tauri/src/commands/interactions.rs`, approx line 193-221) does zero validation on the `uri` parameter — it passes whatever it receives directly to `com.atproto.repo.createRecord` as `subject.uri`.

### Requirements

1. **Add URI validation to `repost_post`**: If `uri` does not start with `at://`, return an error immediately with a clear message like `"Cannot natively repost non-Bluesky content. Use quote-post instead."` This prevents confusing API errors.

2. **Add a new command: `quote_post_to_bluesky`**: This is the creative solution for cross-platform repost. Instead of a native Bluesky repost (which requires an AT-URI), create a new Bluesky post that embeds the original post's content:

   ```rust
   #[tauri::command]
   pub async fn quote_post_to_bluesky(
       text: String,                    // User's commentary (can be empty)
       original_url: String,            // The Mastodon/other platform URL
       original_author: String,         // Display name of original author
       original_content: String,        // Text content of the original post
       state: State<'_, AppState>,
   ) -> Result<String, String>
   ```

   The command should:
   - Create a Bluesky post with `text` + a link card embed pointing to `original_url`
   - Use `app.bsky.embed.external` with the original URL, title = `original_author`, and description = truncated `original_content`
   - This way, Bluesky users see a "shared post" with a link back to the original on its native platform
   - Return the AT-URI of the new Bluesky post

3. **Add a Mastodon repost command: `boost_post_on_mastodon`**: For Mastodon-native reposts:

   ```rust
   #[tauri::command]
   pub async fn boost_post_on_mastodon(
       status_id: String,              // Mastodon status ID
       state: State<'_, AppState>,
   ) -> Result<String, String>
   ```

   - Uses the Mastodon API `POST /api/v1/statuses/:id/reblog`
   - Only works for Mastodon-native posts (where `status_id` is available)

4. **Register new commands** in `lib.rs` invoke handler.

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/commands/interactions.rs` | Add URI validation to `repost_post`, add `quote_post_to_bluesky`, add `boost_post_on_mastodon` |
| `src-tauri/src/lib.rs` | Register new commands in invoke handler |

### Track boundaries

- Do NOT modify any files in `src/` (frontend — Track B)
- The frontend will call these new commands from RepostModal

---

## P1: Feed Population — Server-Side Source Filtering

### Problem

Feed page size is already 50 (`DEFAULT_PAGE_SIZE` in `useUnifiedFeed.ts`), but filtering is **client-side only**. When user selects a source filter (e.g., "Bluesky"), the frontend filters the cached 50 items. If only 2 of 50 are from that source, the user sees only 2 posts.

### Requirements

1. **Add optional `source_filter` parameter to `get_unified_feed`**:

   ```rust
   pub async fn get_unified_feed(
       state: State<'_, AppState>,
       tags: Vec<String>,
       feed_uris: Vec<String>,
       limit: u16,
       cursor: Option<String>,
       source_filter: Option<String>,    // NEW — "bluesky", "mastodon", "instagram", etc.
   ) -> Result<(Vec<BlueskyImportedPost>, Option<String>), String>
   ```

   When `source_filter` is set:
   - If `"bluesky"` → only fetch from Bluesky feeds (skip Mastodon)
   - If `"mastodon"` → only fetch from Mastodon (skip Bluesky)
   - If matches a vault source (e.g., `"instagram"`, `"facebook"`) → skip network calls, only query the local vault/redb for items with that source

2. **Add optional `source_filter` to `fetch_all_sources`** in `feed_aggregation.rs` (approx line 494):

   The `fetch_all_sources` command aggregates from multiple pipelines (Bluesky timeline, Mastodon timeline, vault items). When `source_filter` is provided, only run the matching pipeline.

3. **Invalidate feed cache when source filter changes**: The cache key in `get_unified_feed` already includes tags and feed URIs. Add `source_filter` to the cache key so different filters don't collide:

   ```rust
   if let Some(ref sf) = source_filter {
       key_parts.push(format!("source:{}", sf));
   }
   ```

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/commands/import.rs` | Add `source_filter` param to `get_unified_feed`, conditional pipeline execution |
| `src-tauri/src/commands/feed_aggregation.rs` | Add `source_filter` param to `fetch_all_sources`, skip irrelevant pipelines |
| `src-tauri/src/lib.rs` | Update command registration if function signatures change |

### Track boundaries

- Frontend (Track B) will pass `source_filter` from the dropdown — do NOT modify frontend files

---

## P2: Video Playback — Final Polish for Rock-Solid Smoothness

### Problem

User says video is "mostly smooth" but wants "rock solid" since video is "a large part of how the platform is built."

### Requirements

1. **Validate re-encode quality**: The current FFmpeg strategy prioritizes full re-encode first (Strategy A) with `veryfast` preset, `crf 26`, `fps=30`, `yuv420p`. If user reports specific stutter scenarios, investigate:
   - Whether `veryfast` is too fast for some content (try `fast` for problematic clips)
   - Whether `fps=30` creates judder for 24fps or 60fps source content (consider passthrough)
   - Whether the 1.2 Mbps variant selection target is too low for some HLS sources

2. **Add GStreamer pipeline reuse hint**: In the media server (`media_server.rs`), add `Cache-Control: immutable` to video responses (may already be done — verify). Ensure `ETag` headers are set so WebKitGTK doesn't re-request the same video file.

3. **Monitor**: This is more of a "keep an eye on it" item. If no new stutter reports come in, consider this resolved.

### Files to modify

| File | Changes |
|------|---------|
| `src-tauri/src/media_server.rs` | Verify cache headers are optimal |
| `src-tauri/src/commands/media.rs` | Only if specific stutter scenarios are identified |

---

## Verification

Before committing any changes:

```bash
cd bytescape
cargo check --manifest-path src-tauri/Cargo.toml  # 0 errors
npx tsc --noEmit                                   # 0 errors
pnpm test -- --run                                 # all tests pass
```

Document all changes in `progress.md`.

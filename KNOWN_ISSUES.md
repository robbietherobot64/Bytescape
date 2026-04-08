# Known Issues

Tracked bugs and feature gaps that are acknowledged but not yet resolved.

---

## ISSUE-001: Bluesky rich link embeds not rendering

**Priority:** Low  
**Status:** Open  
**Track:** Frontend (B) / Backend (A) — cross-track  
**Reported:** 2026-03-16  

### Description

When sharing a post to Bluesky via the RepostModal "share as link" flow, URLs in the post body (e.g. YouTube videos) appear as bare text links instead of rich preview cards with title, description, and thumbnail.

Mastodon handles link previews server-side (auto-unfurl), but Bluesky requires the **client** to provide the full embed at post-creation time using `app.bsky.embed.external` with `uri`, `title`, `description`, and an uploaded `thumb` blob.

### What's been tried (3 attempts)

1. **Attempt 1** — Added `buildLinkCard()` in `RepostModal.tsx` that calls `unfurl_link` and passes the result as `linkCard` to `publish_post_v2`. Problem: it unfurled the Mastodon permalink URL instead of the embedded YouTube URL.

2. **Attempt 2** — Rewrote `buildLinkCard()` to extract external URLs from the post's HTML content via `extractExternalUrls()`, filter out same-platform permalinks, and unfurl the first external URL (e.g. YouTube link). Still produced bare links.

3. **Attempt 3** — Verified the extraction + unfurl pipeline connects to `publish_post_v2`'s `link_card: Option<LinkCardInput>` parameter and the backend's `app.bsky.embed.external` builder (lines 372-390 of `posting.rs`). Still no rich embed on Bluesky.

### Suspected root causes (not yet verified)

- **Tauri IPC serialization**: The frontend sends `linkCard` (camelCase) but Tauri may or may not auto-convert to `link_card` (snake_case) for the Rust command parameter. If the field name doesn't match, the `Option<LinkCardInput>` silently deserializes as `None`.
- **`unfurl_link` may fail silently on YouTube**: YouTube may block or rate-limit the unfurl request, returning no OG metadata, causing `buildLinkCard()` to return `null`.
- **Embed record structure**: The `app.bsky.embed.external` record built in `posting.rs` may not exactly match Bluesky's lexicon expectations (e.g. missing `$type` field, wrong nesting).
- **Thumbnail upload**: `download_and_upload_thumb()` may fail silently, and Bluesky may reject embeds without a valid `thumb` blob ref.

### Debugging plan

1. Add `console.log` in `RepostModal.tsx` to confirm `linkCard` value before `invoke('publish_post_v2', ...)`.
2. Add `println!` in `posting.rs` `publish_post_v2` to confirm `link_card` is `Some(...)` on the Rust side.
3. If `link_card` is `None`, the issue is Tauri IPC field naming — fix by using `#[serde(rename = "linkCard")]` on the Rust parameter or renaming the field.
4. If `link_card` is `Some(...)`, inspect the exact JSON body sent to `com.atproto.repo.createRecord` and compare against a working Bluesky embed.
5. Test `unfurl_link` directly with a YouTube URL via the Tauri dev console.

### Files involved

- `src/components/RepostModal.tsx` — `buildLinkCard()`, `extractExternalUrls()`
- `src-tauri/src/commands/posting.rs` — `publish_post_v2`, `LinkCardInput`, embed builder (lines 372-390)
- `src-tauri/src/commands/unfurl.rs` — `unfurl_link` command

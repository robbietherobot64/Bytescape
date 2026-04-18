# Backend Sprint v22 — Media Regression Root Cause & Blog Feed HTML Support

**Sprint**: v22 (Bug Fix + Blog Backend Support)
**Track**: A (Backend — Rust)
**Date**: 2026-04-17
**Previous sprint**: v21 delivered Mastodon media backfill, media provenance, and archive author identity
**HEAD at prompt time**: `e5d07c2`

---

## Sprint Context

Sprint v21 successfully delivered Mastodon media download parity, the media provenance command, archive author identity fixes, and supporting backend work for the profile sign-off. Build is clean: cargo check 0 errors, tsc 0 errors, 273/273 tests pass.

However, a **critical regression** was introduced and one **feature gap** needs backend support:

1. **CRITICAL**: The vault/media gallery no longer shows imported media (Facebook/Instagram archives, Bluesky/Mastodon). Only uploaded media appears. The `vault_cleanup_repost_media` command added in commit `2c4987f` is the primary suspect.
2. **Lower priority**: Blog modules need richer content — the current `blog_feed.rs` strips all HTML from RSS content, losing formatting and images. The frontend wants a `body_html` field for iframe/rich rendering.

### Track boundary note

`src-tauri/src/commands/blog_feed.rs` was created by the Frontend agent in v21 — a **track violation** per guardrails §3. Track A now owns this file. Review it for correctness and maintain it going forward.

---

## P0: Fix vault_cleanup_repost_media Regression (CRITICAL)

### Problem

The user reports: "Media section and vault now fail to show media uploaded from social media archives like facebook/instagram, or from linked accounts like bluesky/mastodon. The only images showing in the vault/media section are uploaded media."

### Root cause analysis

The `vault_cleanup_repost_media` command in `commands/vault.rs` (added in `2c4987f`):

1. Lists ALL vault files via `sync.vault_list_files()`
2. For each file with an `item_id`, looks up the item via `db.get_item_by_id()`
3. If `item.reposted_by_did.is_some()` → **deletes the vault file** (tombstones it)

The frontend calls this command **on every media tab load** in `ProfileScreen.tsx`. This is destructive and runs unconditionally.

### Investigation steps

1. **Add diagnostic logging** to `vault_cleanup_repost_media` BEFORE any deletions:
   - Log the total number of files scanned
   - Log the number of files with `item_id`
   - Log the number of items where `reposted_by_did.is_some()`
   - Log each file being deleted (file.id, file.name, item_id, source)

2. **Check for false positives**: Are archive-imported items (Facebook/Instagram) getting `reposted_by_did` set incorrectly somewhere? The import path in `commit_memories` sets `reposted_by_did: None`, but verify no other code path is overwriting this.

3. **Check for over-deletion**: Is the `get_item_by_id` lookup returning the wrong item due to ID collision or stale data?

4. **Check the import paths**:
   - `import_mastodon_posts` — does it set `reposted_by_did` on items that are NOT reposts?
   - `save_posts` (Bluesky) — does it correctly gate repost detection?
   - Facebook/Instagram archive import — are items getting incorrectly flagged?

### Required fix

Based on findings:

- **If the cleanup logic is correct but overbroad**: Tighten the condition. Only delete files where the item is genuinely a boost/repost (not original content with media) AND the media has no independent value.

- **If the cleanup logic is deleting the wrong files**: Fix the condition.

- **Regardless**: The cleanup should NOT run on every media tab load. It should:
  1. Run once at app startup (background, non-blocking)
  2. Run after a new import completes
  3. NOT block the `vault_list_files` query path

Also fix the **N+1 query pattern**: The current implementation makes one `get_item_by_id` call per file. If the file list is large, this is expensive. Consider batching.

### Files to investigate and fix

| File | Work |
|------|------|
| `src-tauri/src/commands/vault.rs` | Fix `vault_cleanup_repost_media` logic and query pattern |
| `src-tauri/src/commands/import.rs` | Verify import paths are not incorrectly flagging items as reposts |
| `src-tauri/src/db/vault.rs` | Add batch lookup helper if needed for performance |

---

## P1: Add body_html Field to Blog Feed Response

### Problem

The `fetch_blog_feed` command in `blog_feed.rs` strips all HTML from RSS feed content via `strip_html_tags()`, returning only plain text in the `body` field. This loses all formatting, images, and links from blog posts.

The frontend wants to offer an iframe-based rich preview of blog posts, which works best with the original HTML content preserved.

### Required changes

1. **Add `body_html: Option<String>` to `FeedBlogEntry`** — the raw HTML content from the RSS `<content:encoded>` or `<description>` field.

2. **Keep `body` as plain text** — the existing stripped version for summary/fallback display.

3. **Basic HTML sanitization**: Before returning `body_html`, strip `<script>`, `<style>`, and event handler attributes (`onclick`, `onerror`, etc.) to prevent injection. The frontend will render this in a sandboxed iframe, but defense-in-depth is required.

4. **Size limit**: Cap `body_html` at 500KB per entry to prevent memory bloat from pathological RSS feeds.

### Review the existing blog_feed.rs

This file was created by the Frontend agent (track violation). Review it for:
- Error handling quality
- Security of the URL validation (`validate_public_url`)
- Correctness of feed parsing
- Any hardcoded values that should be configurable

### Files to modify

| File | Work |
|------|------|
| `src-tauri/src/commands/blog_feed.rs` | Add `body_html` field, review and fix any issues |

---

## P2: Acknowledge and Own blog_feed.rs

The Frontend agent created this file in commit `cae6d00`. Per guardrails section 3, Rust files in `src-tauri/src/` belong to Track A. This is now your file to maintain.

Review it for:
- The SSRF protection in `validate_public_url()` — is it sufficient?
- The `strip_html_tags()` function — is it robust enough?
- Feed parsing error handling
- The 5MB response limit and 10s timeout — are these appropriate?

If issues are found, fix them as part of this sprint.

---

## Guardrails

### Do

- Fix the media regression as absolute first priority
- Add diagnostic logging before making destructive changes
- Test with actual imported Facebook/Instagram/Bluesky/Mastodon data
- Review and take ownership of `blog_feed.rs`

### Do not

- Do not start hosted identity/publish server work
- Do not modify React frontend files
- Do not remove the media cleanup capability entirely — just fix the logic and timing

---

## Verification

```bash
cd bytescape/src-tauri && cargo check    # 0 errors
cd bytescape && npx tsc --noEmit         # 0 errors
cd bytescape && pnpm test -- --run       # all tests pass
```

Behavioral verification:
1. Imported Facebook/Instagram media appears in vault/media gallery
2. Linked Bluesky/Mastodon media appears in vault/media gallery
3. Uploaded media still appears
4. Actual reposts do NOT pollute the media gallery with duplicate thumbnails

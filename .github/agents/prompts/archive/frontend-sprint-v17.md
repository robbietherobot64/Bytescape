# Frontend Sprint v17 — Repost Modal Fix, Feed Density, Profile Images, UX Polish

**Sprint**: v17  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-08  
**Previous sprint**: v16 (media rebuild — delivered: FeedMedia, VideoPlayer rewrite, containment removal)  
**HEAD at prompt time**: `d70c04f`

---

## Sprint Context

Sprint v16 delivered:
- ✅ FeedMedia component rebuilt (plain `<img>`, removed gallery overlay, removed expand buttons)
- ✅ VideoPlayer rewrite (no click-to-play, debounced visibility, fullscreen protection, autoPlay/controls/muted)
- ✅ Feed card containment removal (no inline `contentVisibility`/`containIntrinsicSize`)
- ✅ Source filter re-fetch trigger
- ✅ RepostModal created (but broken — layout + cross-platform logic)

User feedback on 2026-04-08: video is "mostly smooth", memory/CPU issues resolved. New critical items: repost broken, feed sparse, profile images broken.

---

## P0: Fix RepostModal — Layout, Cross-Platform Logic, Bluesky Repost

### Problem (3 sub-issues)

1. **Layout**: Text is "misaligned and out of the pop up window." Root cause: `RepostModal` wraps content in `Win98Box` but does not pass `variant="window"`, so the title bar doesn't render and the modal looks unstyled. No `overflow: hidden` on the body.

2. **Cross-platform repost**: `resolveAtUri()` in `src/components/feed/helpers.ts` (line 106-117) returns a Mastodon HTTPS URL for non-Bluesky posts. This gets passed to `repost_post` which sends it to Bluesky's API, causing the "Invalid at-uri" error.

3. **Bluesky repost doesn't work at all**: Even for Bluesky-native posts, the repost flow may be broken. The `cid` parameter extraction needs verification.

### Requirements

1. **Fix RepostModal layout** (`src/components/RepostModal.tsx`):
   - Pass `variant="window"` and `title="Share Post"` to the `Win98Box` wrapper
   - Add `overflow: hidden` to the modal body
   - Ensure modal padding is sufficient (currently `padding: 12px` may be too tight)
   - Test that the close button renders in the title bar
   - Ensure modal is centered and properly sized (not clipping content)

2. **Fix `resolveAtUri` for cross-platform awareness** (`src/components/feed/helpers.ts`):
   ```typescript
   export function resolveAtUri(entry: FeedEntry): string {
       // Only return AT-URIs for Bluesky-native posts
       if (entry.id.startsWith('at://')) return entry.id;
       // Check syndicated URI (for vault items that were posted to Bluesky)
       if (entry.syndicated_uri?.startsWith('at://')) return entry.syndicated_uri;
       // Check share history for Bluesky cross-posts
       const bskyEvent = entry.share_history?.find(
           e => e.platform === 'bluesky' && e.active
       );
       if (bskyEvent?.uri?.startsWith('at://')) return bskyEvent.uri;
       // No valid AT-URI — return empty string to signal "not repostable on Bluesky"
       return '';
   }
   ```

3. **Update RepostModal to handle cross-platform repost** (`src/components/RepostModal.tsx`):
   - Detect whether the post has a valid AT-URI (using updated `resolveAtUri`)
   - If AT-URI exists: show "Repost on Bluesky" option (native repost)
   - If no AT-URI but post has `original_url`: show "Share on Bluesky" option that calls the new backend `quote_post_to_bluesky` command (created in backend sprint v17)
   - Show "Boost on Mastodon" option if post is Mastodon-native (has a numeric status ID or Mastodon URL)
   - Disable platform options that aren't applicable (e.g., can't boost a Bluesky post on Mastodon)
   - Show clear user feedback: "This post will be shared as a link on Bluesky" vs "This post will be natively reposted"

4. **Also fix the duplicate `resolveAtUri`** in `src/components/RepostModal.tsx` (approx line 73-79) — it has a copy of the same broken function. Remove it and import from `helpers.ts` instead.

### Files to modify

| File | Changes |
|------|---------|
| `src/components/RepostModal.tsx` | Fix Win98Box variant, fix layout, add cross-platform logic, remove duplicate `resolveAtUri` |
| `src/components/feed/helpers.ts` | Fix `resolveAtUri` to only return valid AT-URIs |

### Acceptance criteria

- RepostModal renders as a proper Win98-style window with title bar
- Text content fits within the modal bounds
- Reposting a Bluesky post on Bluesky works (no "Invalid at-uri" error)
- Reposting a Mastodon post shows "Share as link on Bluesky" option
- Clear feedback about what kind of share is happening

---

## P1: Feed Density — Source Filter to Backend & Filter-Triggered Reload

### Problem

Page size is already 50 (`DEFAULT_PAGE_SIZE`), but source filtering is client-side only. When user selects e.g. "Mastodon" from the source dropdown, the frontend filters the cached 50 items. If only 2/50 are Mastodon posts, user sees 2 posts.

### Requirements

1. **Pass `sourceFilter` to backend** (`src/hooks/useUnifiedFeed.ts`):
   - Add `sourceFilter` parameter to the `invoke('get_unified_feed', ...)` and `invoke('fetch_all_sources', ...)` calls
   - Use the current `effectiveSourceFilter` value from the feed state
   - Backend sprint v17 adds `source_filter` param to these commands

2. **Trigger full reload when source filter changes**:
   - The source filter re-fetch trigger was added in `b23e6e4`, but verify it clears the cache AND re-fetches from backend with the filter applied
   - The `filterKey` already includes `effectiveSourceFilter` (line ~268) — when it changes, `rawEntries` should reset
   - Verify this works end-to-end: changing filter → cache clear → re-fetch with filter → 50 filtered results

3. **Keep client-side filtering as fallback**: Don't remove the client-side source filter in `feedEntries` useMemo — it acts as a safety net for cached data that hasn't been re-fetched yet.

### Files to modify

| File | Changes |
|------|---------|
| `src/hooks/useUnifiedFeed.ts` | Add `sourceFilter` param to invoke calls, verify re-fetch on filter change |

### Acceptance criteria

- Selecting "Bluesky" source filter shows ~50 Bluesky posts (not 2-5)
- Selecting "Mastodon" source filter shows ~50 Mastodon posts
- Changing filters triggers visible loading state and full data reload
- No regression in "All Sources" mode

---

## P2: Profile Page Image Previews

### Problem

User reports: "Image previews on the profile page do not work, it's just a small box with an X attached to it, but no image loads."

### Investigation

The profile screen (`src/screens/ProfileScreen.tsx`) renders posts via `FeedItemRenderer`. Media paths come from `mediaBlobs` (preferred) or `mediaSources` (fallback). The "box with X" symptom means `<img>` tags render but `src` URLs are broken.

### Requirements

1. **Debug the profile media path resolution**:
   - Check if `list_vault_posts_paged` returns correct `media_blobs` / `media_sources` for profile posts
   - Check if the `resolveMediaPath` function converts blob hashes to valid URLs
   - Check if the media files actually exist at the resolved paths (the blob_cache eviction may have deleted them)

2. **Fix the broken path resolution**:
   - Profile posts likely use `convertFileSrc()` for local paths — verify the paths are correct
   - If blob hashes need re-export (evicted from cache), ensure the lazy resolution works on the profile screen too
   - Consider using the same `useLazyBlobUrl` hook that `FeedMedia` uses

3. **Add error state for broken images**: Instead of showing a broken image icon, show a placeholder with a retry button.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Fix media path resolution for profile post images |
| `src/components/feed/FeedMedia.tsx` | May need to ensure FeedMedia is used consistently on profile too |

### Acceptance criteria

- Profile page shows image thumbnails for posts with media
- No broken image icons (small box with X)

---

## P3: Data Import — Remove Auto-Redirect

### Problem

User says: "Data import should not be automatic redirect to the profile. It should just be the landing page for you to upload... Redirect to the profile makes no logical sense, when you want to import more posts."

### Requirements

1. **Remove the auto-redirect countdown** in `src/screens/ImportScreen.tsx` (approx line 107-127):
   - Delete the `useEffect` that starts a 5-second countdown after `importComplete`
   - Delete the `redirectCountdown` state and the "Redirecting to profile in Xs..." UI
   - Keep the import success message but without any navigation

2. **Keep a "View Imported Posts" button** as a manual navigation option (not auto-redirect).

3. **Keep the import screen as the landing state** after import completes, so users can import another archive.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ImportScreen.tsx` | Remove auto-redirect useEffect, keep manual navigation button |

### Acceptance criteria

- After import completes, user stays on the import screen
- User can manually click to view imported posts
- User can immediately start another import

---

## P4: Instagram Label — "insta"

### Problem

User wants all Instagram posts labelled as "insta" instead of "Instagram".

### Requirements

1. **Update SourceBadge config** (`src/components/SourceBadge.tsx`):
   - Add a `label` field to the source config map
   - For `instagram`, set `label: 'insta'`
   - Render `config.label || source` instead of raw `source`

2. **Update platform abbreviation** (`src/utils/platform.ts`):
   - Change `instagram: 'nstgrm'` to `instagram: 'insta'`

3. **Search for any other places** that render the raw "Instagram" string and update them.

### Files to modify

| File | Changes |
|------|---------|
| `src/components/SourceBadge.tsx` | Add label field, render "insta" for Instagram |
| `src/utils/platform.ts` | Update abbreviation to "insta" |

### Acceptance criteria

- Instagram posts show "insta" badge in feed
- Instagram filter option shows "insta" label
- No hardcoded "Instagram" strings visible in UI

---

## Verification

Before committing any changes:

```bash
cd bytescape
npx tsc --noEmit                     # 0 errors
pnpm test -- --run                   # all tests pass
cargo check --manifest-path src-tauri/Cargo.toml  # 0 errors (if Rust files touched)
```

Document all changes in `progress.md`.

---

## Track Boundaries Reminder

- **Track B owns**: `src/` (all React/TypeScript, CSS)
- **Track A owns**: `src-tauri/src/` (Rust)
- **Shared files** (require PM coordination): `src/types/index.ts`, `src/hooks/useUnifiedFeed.ts`, `src-tauri/src/lib.rs`
- Check `git log --oneline -5 -- <filepath>` before editing any shared file

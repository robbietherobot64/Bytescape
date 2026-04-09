# Session Handoff — Feed Fixes (April 5, 2026)

**HEAD**: `9803005` (main)

---

## ✅ What's Working

1. **Hashtag filtering** — User confirmed "working very well." Logic uses `&&` instead of `||` so hashtags only filter when BOTH feeds and follows are disabled. Following tab is excluded from hashtag filtering entirely.

2. **Active Follows toggle** — Wired end-to-end. Checkbox on All tab controls `followsDisabled` state that flows into `useUnifiedFeed`. `__disabled__` token preserved through validation effects.

3. **Feeds auto-disable for non-Bluesky** — When source filter is not Bluesky, custom feeds section auto-disables since feeds are Bluesky-only.

4. **Build is clean** — 0 TypeScript errors, 273/273 tests pass.

---

## 🔧 What Was Just Changed (This Commit)

**Media grid DOM restructure**: Moved `<MediaGrid>` from inside `.feed-entry-body` (which has `padding: 8px 10px`) to be a **direct child of `.feed-entry-card`**. This eliminates the entire negative-margin hack (`margin: 0 -10px; width: calc(100% + 20px)`) that was fighting with `contain: layout style` and WebKitGTK's percentage resolution quirks. Now the grid is just `width: 100%` — no tricks needed.

**`--content-max-width` fallback**: `.feed-main` used `var(--content-max-width)` with NO fallback. That variable is only defined in the geocities theme. On win98/midnight/hacker themes, `.feed-main` would collapse to content width. Added `960px` fallback.

**USER HAS NOT TESTED THIS COMMIT YET.** The previous 20+ attempts all used the negative-margin approach inside `.feed-entry-body`. This is a fundamentally different approach.

---

## ❌ Known Remaining Issues

### Issue 2: Bluesky source + follows + hashtags shows too few posts
- **Root cause identified**: Source filter (`archiveSourceFilter`) is **client-side only**. It doesn't trigger a re-fetch — it just filters the existing cache. If the cache was populated with mixed sources (Bluesky + vault + Mastodon), filtering to "Bluesky" throws away most results.
- **Fix needed**: Add `effectiveSourceFilter` to the `filterKeyRef` dependency in `useUnifiedFeed.ts` (line ~473) so changing source triggers `feedCache.setCache(activeTab, [], null)` + `fetchFeed()`. Or pass source to backend.
- **Secondary issue**: Backend `searchPosts` uses bare word search (strips `#`), not exact hashtag matching. May return irrelevant results.

### Issue 3: Media images off-centered / not filling width
- **Status**: Latest commit (`9803005`) takes a fundamentally new approach — needs user testing.
- **If still broken after testing**, investigate:
  1. `contain: layout style` on `.feed-entry-card` — try removing it temporarily to see if it's the culprit
  2. WebKitGTK inspector (run `WEBKIT_INSPECTOR_SERVER=127.0.0.1:9222 pnpm tauri dev`) to check actual computed widths
  3. Nuclear option: remove ALL `contain` and `content-visibility` from the feed chain, verify media works, then add them back one by one

### Other (from user-feedback file)
- **P0**: Memory leak when filtering by video (32+ GB). Instagram videos don't load, CPU spikes on launch.
- **P0**: Video playback stuttery across all sources (GStreamer pipeline issue).
- Instagram images don't load in feed (work in vault).
- Mastodon posts appear doubled.
- Profile page image previews broken.
- Import screen UX improvements needed.

---

## Key Files

| File | What it does |
|------|-------------|
| `src/hooks/useUnifiedFeed.ts` | Core feed hook — fetching, caching, client-side filtering. Source filter is client-side only (line ~473 `filterKeyRef`). |
| `src/components/FeedEntryCard.tsx` | Card renderer. MediaGrid is now a direct child (line ~465), NOT inside `.feed-entry-body`. |
| `src/App.css` (lines 1303-1420) | Media grid CSS. Now uses simple `width: 100%`, no negative margins. |
| `src/screens/FeedScreen.css` (line 14) | `.feed-main` flex/max-width with `--content-max-width` fallback. |
| `src/screens/FeedScreen.tsx` | Feed screen — manages `followsDisabled` state, active tab, filter chips. |
| `src/components/FeedRightPanel.tsx` | Right sidebar — Active Follows checkbox, feed/tag chip selection. |

---

## Commit Chain (This Session)

```
9803005 fix(track-b): move media grid outside feed-entry-body, content-max-width fallback
aabd3e7 fix(track-b): absolute positioning on media img/video for WebKitGTK
5722061 fix(track-b): hashtag filter only when both streams off, full-bleed media grid
3eea882 fix(track-b): hashtag filter, Active Follows toggle, media spacing
61927f2 feat(track-b): consistent hashtag filtering, feeds auto-disable, Active Follows toggle
```

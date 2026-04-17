# Frontend Sprint v22 — Media Regression Fix, Blog Embeds & ProfileScreen Decomposition

**Sprint**: v22 (Bug Fixes + Blog Embeds + Tech Debt)
**Track**: B (Frontend — React/TypeScript)
**Date**: 2026-04-17
**Previous sprint**: v21 delivered the profile sign-off punch list
**HEAD at prompt time**: `e5d07c2`

---

## Sprint Context

Sprint v21 successfully delivered hover cards, connection avatars, media lightbox, blog modules, profile picture/background upload, module ordering, GIF improvements, and RSS blog feed subscriptions. Build is clean: tsc 0 errors, 273/273 tests, cargo check clean.

However, **two regressions and one feature gap** remain per `user-feedback`:

1. **CRITICAL**: Media gallery/vault no longer shows imported media (Facebook/Instagram archives, Bluesky/Mastodon linked accounts). Only manually uploaded media appears. This is a regression introduced during v21.
2. **Lower priority**: Blog modules only show the URL and "No content preview available" with a "Read full post" link. The user wants seamless embedded blog content, like iframes showing the actual posts.

Additionally, the **code audit** found:
- **Track boundary violation**: The frontend agent created `src-tauri/src/commands/blog_feed.rs` (a Rust backend file). This must be acknowledged — Track A now owns that file.
- **ProfileScreen.tsx is 2,820 lines** — a "god component" that needs decomposition.
- **ProfileScreen.css is 2,246 lines** — growing but manageable.

---

## P0: Fix Media Gallery Regression (CRITICAL)

### Problem

The media tab in the vault/profile loads zero imported media. Only user-uploaded files appear.

### Root cause (from audit)

In `ProfileScreen.tsx` around line 510, `loadMedia()` calls `vault_cleanup_repost_media` **before** listing files on every tab load. This function (in `vault.rs`) iterates ALL vault files, looks up their linked item, and **deletes any file whose item has `reposted_by_did` set**.

The likely failure chain:
- `vault_cleanup_repost_media()` runs unconditionally on every media tab open
- It may be deleting files that should not be deleted (false positive on the repost check)
- OR it's running before vault files have finished syncing
- OR the repost-flagging logic in import paths is stamping the wrong items

### Required fix

1. **Remove** the unconditional `vault_cleanup_repost_media()` call from `loadMedia()`. Media cleanup should NOT run on every gallery view — it should be a one-time maintenance pass or triggered explicitly, not blocking every media load.

2. **Verify** that after removing the call, imported media from Facebook/Instagram archives and Bluesky/Mastodon linked accounts appears in the gallery again.

3. If the cleanup is genuinely needed (to hide repost thumbnails), move it to a background task that runs once at app startup and does NOT block the media query path.

### Files to modify

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Remove `vault_cleanup_repost_media` call from `loadMedia()` |

### Backend coordination

If the root cause is deeper (e.g., media files are genuinely being tombstoned in the vault), coordinate with Track A. But the first step is removing the frontend's unconditional cleanup call and testing whether media reappears.

---

## P1: Rich Blog Post Embeds

### Problem

Blog modules show only "No content preview available" with a bare URL link. The user wants actual blog content visible inline, similar to how embeds work for music services.

### Current state

- `blog_feed.rs` fetches RSS/Atom feeds and returns `FeedBlogEntry` objects with `body` (plain text — HTML is stripped server-side), `title`, `author`, `timestamp`, `external_url`
- `renderBlogs()` in ProfileScreen.tsx displays entries in an accordion with the plain text body
- If the body is empty → "No content preview available"
- The HTML stripping in `blog_feed.rs` discards all formatting, images, and links from the feed content

### Required approach

Implement a **sandboxed iframe embed** for blog posts:

1. **For entries with an `external_url`**: Render a sandboxed iframe pointing to the blog post URL. Use `sandbox="allow-scripts allow-same-origin"` with restrictive permissions. Add a loading indicator and a fallback for sites that block iframes (X-Frame-Options).

2. **Fallback rendering**: If the iframe fails to load (detected via `onError` or timeout), fall back to the current text-based preview + "Read full post" link.

3. **User controls**: Add a "View inline" / "View in browser" toggle per entry so users can choose between iframe and external link.

4. **Size**: Iframes should have a sensible default height (400-500px) and be resizable or scrollable.

### Security requirements

- `sandbox` attribute is mandatory on all iframes
- Do NOT use `allow-top-navigation` in the sandbox
- Do NOT render raw HTML from RSS feeds via `dangerouslySetInnerHTML`
- The iframe approach is safer than injecting fetched HTML into the DOM

### Backend change needed (coordinate with Track A)

Ask Track A to add a `body_html` field to `FeedBlogEntry` that preserves the original HTML content alongside the stripped `body` text. The frontend can then offer a toggle between iframe view and rendered HTML preview (sanitized with DOMPurify).

### Files to modify

| File | Work |
|------|------|
| `src/screens/ProfileScreen.tsx` | Update `renderBlogs()` with iframe embed + fallback |
| `src/screens/ProfileScreen.css` | Blog iframe container, loading states, fallback styles |

---

## P2: ProfileScreen Component Decomposition

### Problem

`ProfileScreen.tsx` is 2,820 lines — a god component containing module rendering, media loading, blog logic, connection hover cards, avatar/background upload, page editing, and layout management all in one file.

### Required outcome

Extract at least the following into separate component files:

| New file | Extracted from | Approx lines |
|----------|---------------|--------------|
| `components/profile/BlogModule.tsx` | `renderBlogs()` + blog state/effects | ~300 |
| `components/profile/MediaGallery.tsx` | `loadMedia()` + media grid + lightbox | ~250 |
| `components/profile/ConnectionsModule.tsx` | `renderConnections()` + hover card | ~350 |
| `components/profile/ModuleEditor.tsx` | Module CRUD, reorder, width toggle | ~200 |

### Constraints

- Each extraction must be a **standalone commit** that does not change behavior
- Run `npx tsc --noEmit` and `pnpm test -- --run` after each extraction
- ProfileScreen.tsx should import and compose the new components
- Do NOT refactor logic during extraction — pure move only
- Target: ProfileScreen.tsx under 1,500 lines after extraction

### Files to create/modify

| File | Work |
|------|------|
| `src/components/profile/BlogModule.tsx` | New — extracted blog rendering |
| `src/components/profile/MediaGallery.tsx` | New — extracted media gallery |
| `src/components/profile/ConnectionsModule.tsx` | New — extracted connections + hover card |
| `src/components/profile/ModuleEditor.tsx` | New — extracted module CRUD controls |
| `src/screens/ProfileScreen.tsx` | Import and compose new components |

---

## Guardrails

### Track boundary reminder

- **Track B owns**: `src/` (React/TypeScript, CSS)
- **Track B does NOT own**: `src-tauri/src/` (Rust backend)
- The `blog_feed.rs` file created in v21 was a **track violation** — Track A now owns it. Do NOT modify Rust files in this sprint. If you need backend changes, document the request and leave it for Track A.

### Do

- Fix the media regression first — it is the critical user-facing bug
- Test media loading with both archive-imported and linked-account content
- Keep iframe security tight (sandbox attribute, no allow-top-navigation)
- Split decomposition into atomic commits

### Do not

- Do not add new Rust backend files or modify existing ones
- Do not add a profile theme picker (profile inherits app system theme)
- Do not jump ahead to hosted identity/publish work
- Do not refactor module logic during the decomposition step — pure extraction only

---

## Verification

Before each commit:

```bash
cd bytescape && npx tsc --noEmit       # 0 errors
cd bytescape && pnpm test -- --run      # all tests pass
cd bytescape/src-tauri && cargo check   # 0 errors
```

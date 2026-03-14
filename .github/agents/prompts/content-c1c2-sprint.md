# Content Agent — Sprint Kickoff (2026-03-15)

## PM Sprint Review Summary

### ✅ Accepted Work
All deliverables verified and accepted:
- **C1.1b/C1.2b/C1.3b**: Facet rendering + autocomplete frontend — 53 tests, commit `38cf4c2` ✅
- **C6.1 CRDT Storage Layer**: `yrs` backend + `useCollabDoc` hook + ArticleEditor collab mode — 18 tests, commit `16a9c69` ✅
- **C6.2 Frontend Yjs Integration**: Delivered within C6.1 scope — `useCollabDoc` hook IS the frontend integration ✅

### ✅ Good Decisions
- **XmlFragment vs Y.Text**: Correct architecture — Tiptap owns ProseMirror↔Yjs binding, Rust stores CRDT state + rendered snapshots from frontend. The `snapshot_collab_to_item(content: &str)` parameter design is clean.
- **Building on Backend's GAP-4 collab.rs**: Good call assessing and extending rather than recreating.
- **Merging both main and track-b**: Your branch has the widest integration — good merge hygiene.

### ⚠️ Note on C6.2
PM has marked C6.2 as complete in the ROADMAP since your C6.1 delivery covers the full frontend integration. The remaining C6 items (C6.3 P2P Sync, C6.4 Offline Merge, C6.5 Station Relay, C6.6 Access Control) are all future sprints and require Track A dependencies.

---

## Priority 0 — Merge `main` Into Your Branch (REQUIRED FIRST)

Your branch `track-c/c1-facet-frontend` is **missing 1 critical commit from `main`**:

```
fe890b4 fix(a0): PM audit remediation — master password, OAuth tests, DM pagination, feed auth, debug cleanup, restore guard
```

Your branch previously merged an **older** version of main. This remediation commit landed on main AFTER your last merge.

### Merge Instructions

```bash
git checkout track-c/c1-facet-frontend
git merge main
```

**Expected conflicts:** Likely minimal — your branch already has most of main's code from the prior merge (`cc85d0f`). The remediation commit (`fe890b4`) touches `lib.rs`, `oauth.rs`, and `oauth_test.rs`. Your prior merge already resolved `lib.rs` conflicts, so this should be a cleaner merge. If conflicts occur in `lib.rs`, accept the main version for OAuth-related code blocks while preserving your collab command registrations.

After resolving:
```bash
git add -A && git commit -m "merge: main into track-c (get fe890b4 remediation)"
```

Verify: `pnpm tauri dev` builds, `pnpm test` passes.

---

## Priority 1 — C1.4: Character Count & Limits

### Context
A basic 10K char counter already exists from the MED-4 security hardening (input length validation in ComposeBox). Your job is to make it **platform-aware and intelligent**.

### Requirements

#### Per-Target Character Counting
- **Bluesky**: 300 **graphemes** (not characters, not bytes). Use `Intl.Segmenter` with `granularity: 'grapheme'` for accurate counting. Emoji like 👨‍👩‍👧‍👦 count as 1 grapheme.
- **ActivityPub**: Configurable per-server, default 500 characters. For now, hardcode 500.
- **Local vault**: 10K characters (existing MED-4 limit).

#### UI Components
1. **Live counter** below the ComposeBox text area — shows `{current}/{limit}` for each active target
2. **Color coding**: Green (0-80%), Yellow (80-95%), Red (95-100%), Disabled/error (>100%)
3. **Multi-target display**: When posting to "All", show counters for each target side-by-side:
   ```
   🦋 142/300  🌐 142/500  💾 142/10000
   ```
4. **Truncation warning**: If text exceeds a target's limit, show which targets will truncate. E.g., "Text exceeds Bluesky limit by 42 graphemes — post will be truncated on Bluesky."
5. **Post button state**: Disable the Post button if ALL targets exceed their limits. If only some targets exceed, allow posting but show a confirmation modal.

#### Implementation Notes
- Create a `useCharacterCount(text: string, targets: string[])` hook that returns counts per target
- The hook should handle grapheme segmentation (Bluesky), character counting (AP), and byte counting if needed
- Integrate into `ComposeBox.tsx` — this is your component to modify
- Don't break the existing MED-4 hardening (10K overall limit, 8K warning)

### Tests
- Grapheme counting: ASCII, emoji, ZWJ sequences, CJK characters
- Multi-target counter display
- Color state transitions
- Truncation warning logic
- Post button disable state

---

## Priority 2 — C2.1: Link Card Embeds

### Backend (Rust)

Create a new Tauri command for URL unfurling:

```rust
// src-tauri/src/commands/unfurl.rs

#[derive(Serialize, Deserialize, Clone)]
pub struct LinkCard {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
    pub favicon_url: Option<String>,
}

#[tauri::command]
pub async fn unfurl_link(url: String) -> Result<LinkCard, String> {
    // 1. GET the URL with reqwest (already in Cargo.toml)
    // 2. Parse HTML with scraper (already in Cargo.toml)
    // 3. Extract: og:title, og:description, og:image, og:site_name
    // 4. Fallback to twitter:title, twitter:description, twitter:image
    // 5. Fallback to <title> tag and first <meta name="description">
    // 6. Return LinkCard
}
```

Register in `lib.rs` invoke_handler. Add `LinkCard` to `src/types/index.ts`.

**Safety:**
- Set a timeout (5s) on the fetch
- Limit response body to 1MB (don't download entire pages)
- Only fetch `text/html` content types
- Don't follow more than 3 redirects
- Sanitize all extracted strings (strip HTML tags, limit lengths)

### Frontend (React)

1. **URL detection**: In ComposeBox, detect URLs as the user types (debounced, 500ms after last keystroke). Use a simple regex: `https?://[^\s]+`
2. **Preview card**: Below the ComposeBox text area, render a `LinkPreviewCard` component with the unfurled metadata — thumbnail, title, description, domain
3. **Remove action**: User can dismiss the link card preview (X button)
4. **On publish**: Include the link card data in the post payload:
   - For Bluesky: emit `app.bsky.embed.external` with `{uri, title, description, thumb}`
   - For local vault: store as structured metadata on `BytescapeItem`
   - For AP: emit as `attachment` with `type: Link`

### Tests
- URL regex matches valid URLs, rejects invalid ones
- LinkPreviewCard renders title, description, image
- Dismiss action removes the preview
- Handles unfurl errors gracefully (show URL-only fallback)

---

## Branch & Commit Strategy
- Stay on `track-c/c1-facet-frontend`
- Commit after each priority: `merge: ...`, `feat(track-c): C1.4 character count & limits`, `feat(track-c): C2.1 link card embeds`
- Update `progress.md` at the end of your session

## Anti-Patterns
- Do NOT modify `ProfileBuilderScreen`, `ProfileBrowserScreen`, or lens components — Track B owns those
- Do NOT modify Rust connector files (`connectors/*.rs`) — Track A owns those
- Do NOT add items to ROADMAP.md — PM scope only
- Do NOT start unassigned items after completing these priorities — report back via progress.md
- Do NOT break the existing MED-4 input validation when adding character counting

# Backend Agent — Sprint v7 (2026-03-19)

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` — **NOTE: this file is stale/outdated.** Rely on this prompt for current priorities.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Backend/Track A — `src-tauri/src/`, `Cargo.toml`) and the Frontend agent (Track B — all of `src/`).

### Performance Standards
- **Research before coding.** Read existing code before changing it.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine attempts, document what you tried in `progress.md` and move on.
- **Terminal discipline:** Kill background processes when done. Close every terminal you open.
- **Do NOT modify files in `src/`** — that is Frontend scope.

---

## 🚨 Codebase State (as of commit `a1e5be7`)

### What was fixed by the PM before this sprint
1. **Source filter: frontend now passes `sourceFilter` to backend** — The `all` tab in `useUnifiedFeed.ts` now sends `sourceFilter` to `get_timeline_unified`. Your sprint v6 backend work is now fully wired.
2. **System browser re-enabled** — External URLs use system browser. The in-app WebView (`add_child()`) is broken on Linux/WebKitGTK and has been disabled.

### Sprint v6 results (your previous work)
- ✅ Source filter backend: `source_filter` param on `get_timeline_unified`, composite cache keys, vault query passthrough, post-merge safety filter
- ✅ `get_or_create_bsky_convo` exposed as Tauri command (OAuth + app-password paths)
- ✅ WebView window label fixed to "main" + diagnostic logging + fallback
- ✅ Profile sections persistence: `save_profile_sections` / `get_profile_sections` commands
- ✅ Facebook import verification: quality confirmed against workspace archive
- ✅ 30 new Rust unit tests (FeedCache isolation, source filter, profile sections, Facebook import quality)

**All P1-P6 completed.** Strong sprint.

**Build:** cargo ✅ (0 warnings), tsc ✅, 264/264 tests ✅

---

## Sprint Theme: "Profiles, Instagram, & Chat Robustness"

The backend is in good shape after sprint v6. This sprint focuses on: supporting the frontend's new native profile view, adding Instagram archive import, and hardening the chat system.

---

## Phase 1 — Support Frontend Features

### P1 — Expose `resolve_profile` Command ⚡⚡⚡ CRITICAL

The Frontend agent is building a native Bluesky profile screen (rendering profile + posts in React instead of a broken WebView). They need a command to resolve a Bluesky profile by handle.

**Check first:** A `resolve_profile` or `get_profile` command may already exist. Search `lib.rs` and `commands/` for existing profile-fetching commands.

**If it doesn't exist, create:**
```rust
#[tauri::command]
pub async fn resolve_profile(
    state: State<'_, AppState>,
    handle: String,
) -> Result<ProfileView, String>
```

Where `ProfileView` includes: `did`, `handle`, `displayName`, `description`, `avatar`, `banner`, `followersCount`, `followsCount`, `postsCount`.

This should call the Bluesky `app.bsky.actor.getProfile` endpoint via the existing OAuth/XRPC infrastructure.

**Also verify `get_author_feed` works for arbitrary DIDs** (not just the logged-in user). The frontend will call `get_author_feed({ actor: targetDid, limit: 20 })` to show a profile's posts.

### P2 — Harden `get_or_create_bsky_convo` ⚡⚡

The command works but may be unreliable under certain conditions:
1. **Add timeout:** If the Bluesky chat API takes >10 seconds, return an error instead of hanging
2. **Handle expired sessions:** If the OAuth session expired, retry once with session refresh
3. **Handle missing chat scope:** The chat endpoint requires `chat.bsky.convo` scope. If the user logged in without it, return a clear error message like "Re-login required for chat (missing chat scope)"
4. **Add logging:** Log the member_did and result (success/error) to help debug issues

### P3 — Instagram Archive Import ⚡⚡ NEW FEATURE

There's an Instagram archive in the workspace at `instagram-gostaticband-2026-02-22-GIeT6CmP/`.

Create a new connector: `src-tauri/src/connectors/instagram.rs`

**Instagram archive structure:**
- `personal_information/` — account info, profile
- `media/posts/` — post images
- `media/reels/` — video reels
- `media/stories/` — story images/videos
- `your_instagram_activity/content/` — posts, stories, reels with captions
- `connections/followers_and_following/` — social graph

**Implementation:**
1. Create `connectors/instagram.rs`
2. Add `Instagram` variant to `Source` enum in `connectors/mod.rs`
3. Parse the JSON files in `your_instagram_activity/content/` for posts with:
   - Caption text → `content`
   - Timestamps → `original_timestamp`
   - Media references → `media_paths` (link to files in `media/` directory)
4. Import media files as blobs (same pattern as Facebook import)
5. Normalize to `BytescapeItem` schema
6. Register an `import_instagram_archive` command in `lib.rs`
7. Apply the same quality filters as Facebook (skip activity logs, ads, etc.)

**Reference:** Follow the pattern in `connectors/facebook.rs` for archive parsing.

---

## Phase 2 — Infrastructure

### P4 — Unified Contact List ⚡⚡

Currently, contacts come from multiple sources but aren't unified:
- Bluesky follows (via `list_follows`)
- P2P peers (via `list_peers`)
- Imported contacts (Facebook friends, Instagram follows — not yet merged)

Create a `list_all_contacts` command that merges all sources into a unified contact list:
```rust
#[tauri::command]
pub async fn list_all_contacts(
    state: State<'_, AppState>,
) -> Result<Vec<UnifiedContact>, String>
```

Where `UnifiedContact` includes: `did`, `handle`, `display_name`, `avatar`, `sources: Vec<String>` (which platforms this contact is known from).

This enables the frontend to show a unified friends list that includes Bluesky follows, P2P peers, and imported contacts.

### P5 — Rate Limit Handling ⚡

Some Bluesky API calls fail silently when rate-limited. Add proper rate limit handling:
1. Check response headers for `RateLimit-Remaining` and `RateLimit-Reset`
2. If rate-limited (429), wait and retry once
3. Log rate limit events so the frontend can optionally display a warning
4. Add rate limit awareness to: `get_timeline`, `get_author_feed`, `get_unified_feed`, `list_follows`, `list_followers`

---

## Phase 3 — Technical Debt (as time allows)

### P6 — Improve Error Messages ⚡

Many commands return generic `"Failed to..."` error strings. Improve:
1. Include the HTTP status code in API error messages
2. Distinguish between auth errors (401/403 → "Session expired, please re-login") and server errors (5xx → "Bluesky servers are down, try again later")
3. Return structured error types where possible

### P7 — Archive Import Progress Events ⚡

Large archive imports (Facebook, Instagram) can take 30+ seconds. Emit progress events:
```rust
app.emit("import-progress", ImportProgress {
    stage: "Parsing posts...",
    current: 150,
    total: 500,
});
```

The frontend can listen for these to show a progress bar instead of a generic spinner.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend scope
- ❌ Do NOT spawn multiple build processes or leave zombie terminals
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT guess at API behavior — read the Bluesky AT Protocol docs

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] `resolve_profile` command exists and returns full Bluesky profile data
- [ ] `get_author_feed` works for arbitrary DIDs (not just logged-in user)
- [ ] `get_or_create_bsky_convo` has timeout, session refresh, missing-scope handling
- [ ] Instagram archive connector parses posts + media from workspace archive
### Phase 2 (Features — high priority)
- [ ] `list_all_contacts` returns unified contacts from all sources
- [ ] Rate limit handling on key Bluesky API commands
### Phase 3 (Debt — as time allows)
- [ ] Error messages include HTTP status and actionable guidance
- [ ] Archive imports emit progress events
### Build verification
- [ ] `cargo check` — 0 errors, 0 warnings
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages

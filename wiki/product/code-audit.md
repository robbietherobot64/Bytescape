---
title: Bytescape Code Audit vs MVP Scope
created: 2026-10-04
status: active — evidence base for the keep/polish/cut roadmap
---

# Code Audit vs MVP Scope

Audit frame: the six MVP demo steps in `mvp-scope.md`. Sources: two parallel
read-only code audits (frontend, backend) + build smoke checks, 2026-10-04.
**Bottom line: keep. All six demo steps exist end-to-end; the work is
consolidation and rewiring, not construction.**

## Build status

- Frontend: `tsc --noEmit` exit 0, `vite build` exit 0. Bundle ships an 897 kB
  `matrix-wasm` chunk (Matrix is dead per AD-01d) and an 806 kB main chunk.
- Backend: `cargo check` exit 0, dead-code warnings only.
- Tests: ~6 unit tests backend (3 `identity.rs`, 3 `import_facebook.rs`), **zero
  frontend tests**, zero integration tests.
- **VCS risk — RESOLVED (2026-10-04):** the codebase was fully gitignored at audit time;
  it is now under version control and the trees live at repo root. (Kept for history.)

## Per-step verdicts

| # | Step | Frontend | Backend | Verdict |
|---|---|---|---|---|
| 1 | Sign up | done — `BlueskyLanding.tsx` (76) + `LoginFlow.tsx` (118); no email/invite UI | done — `api/auth.rs` (613, invite codes, verification, refresh) + `api/oauth.rs` (994) + `oauth/bluesky.rs` (2048, PAR+DPoP) | **keep** |
| 2 | Import | done — `ImportStep.tsx` (471) ZIP+SSE+4 live counters; **duplicate SSE reader** in `AccountsScreen.tsx` (~50 ln); ImportStep mounted only via broken tour | done — `api/import.rs` (996) SSE phases; `import_facebook.rs` (2040) DOM+JSON parser; `import_instagram.rs` (880); dead ~320 ln streaming fn; divergent stream/non-stream paths (non-stream skips contacts) | **keep + polish** |
| 3 | Friend matching | **ORPHANED** — `ImportResults.tsx` (441): match list, badges, Follow-All; `sessionId` dropped by ImportStep → unreachable | done — handle matching (`certain`, auto-follow both directions) + mutual-archive reverse name matching (`likely`); `GET /api/import/{id}/social-graph`; N+1 queries; migration 035 gap (see below) | **keep + rewire** (smallest fix of the demo gaps) |
| 4 | Profile populates | done — `ProfileScreen.tsx` (638) → `ProfileGrid.tsx` (215) → 10 live widgets; `WidgetPosts` timeline, `WidgetPhotoGallery` lightbox | done — `api/profile.rs` (1402) merges local+imported+linked posts, source/date filters; offset pagination, N+1 reshare resolution | **keep** |
| 5 | Customization | real drag/resize (`react-grid-layout/legacy`), `ProfileToolbar` palette, `WidgetThemeEditor`, music embeds (Bandcamp/SoundCloud/Spotify), custom links; GIF upload via gallery; **3 preset themes = dead CSS** (`styles.css` ~2745-3164) wired only to orphaned `ProfileEditForm`/`ProfileLayout`; no linked-GIF (Tenor/Giphy) widget | done — generic JSONB store: `users.widget_layout/widget_themes/theme/custom_css` (migrations 001/014/025); no widget catalog (frontend owns semantics) | **polish** — decide preset-theme story: rewire or delete ~600 lines |
| 6 | Feed | done UI — `FeedColumn.tsx` interest picker (min 3) → `saveInterests` → `subscriptionApi.bootstrap`; `FeedLayout.tsx` (756) two-phase seed/full; `PostCard.tsx` (1276) media-first render (HLS, carousel, grids); zero-friends → discover fallback | **rough** — `api/feed/mod.rs` **3578-line monolith**, ~10 overlapping endpoints; **media-first ordering unimplemented** (`has_media` populated but never queried); dual interest mechanisms (`users.interests` read vs `bootstrap_hashtag` rows written-never-read); `feed_follow_status` unrouted | **rewrite/consolidate `mod.rs`**; keep clean submodules (`sources/filter/normalize/sort/paginate`) + `interest_sources.rs` + `subscriptions.rs` |

## Demo blockers (small, ordered) — **SHIPPED 2026-10-07** (commits `fa26b5d`, `247cae0`) — kept for history

1. **Orphaned `ImportResults`** — the "find your friends" moment never displays.
   Fix: pass `session_id` from `ImportStep.onComplete` and mount the screen.
2. **Onboarding tour dead-ends at step 2** — coachmark targets
   `[data-tour="import-button"]`, which exists only in orphaned `ProfileFeed.tsx`;
   `CoachMark` retries 6× then renders null. Worse: `onboarding_complete='false'`
   re-routes to Profile on every reload (`App.tsx:88-97`).
3. **Preset themes invisible** — the "pick a theme" beat of the demo has no live UI.

## Landmines (fix before any external user touches it)

- `pds/mod.rs:643` `createSession` accepts **any password**; `:667` `getSession`
  returns `did:web:placeholder`; `oauth_server/mod.rs:457` DPoP jkt =
  `"placeholder_jkt"`; `api/identity.rs:12-13` return literal `"TODO"`.
- **Migration runner skips 12 files** (032-035, 037, 042-049; admitted in `055`
  header). `muted_accounts` (049) and `imported_contacts.category` (035) are
  queried by shipping code but absent from fresh DBs/backups — social-graph
  handler can 500. Replace with sqlx migrate before any deploy. Confirmed live
  2026-10-07: a fresh DB also lacks `users.visitor_count` (it exists in NO
  migration file) — signup 500s until patched manually. After any manual schema
  patch, restart the server: the pool's prepared statements otherwise raise
  `cached plan must not change result type`.
- `APPVIEW_CONSUMER=true` is opt-in → fresh deploy has an empty local index.
- Hardcoded `http://localhost:8080` in 4 frontend files (`api.ts`,
  `FeedLayout.tsx`, `ComposerModal.tsx`, `EventsColumn.tsx`).
- Composer exists (`ComposerModal.tsx`, 190 ln) — answers the open MVP question
  cheaply if a post composer is wanted in v1.

## Dead-weight inventory (cut candidates)

**Frontend ~2,900 lines + deps:** 19 orphaned files (~2,460: `ImportResults` is
NOT dead — rewire it; the rest is), ~420 lines dead preset-theme CSS,
`matrix-js-sdk` dep + `utils/matrix.ts`, `utils/crypto.ts`, `utils/formatSource.ts`,
duplicate SSE reader. Non-MVP screens still in nav: Forums (334), Exchange (499),
Events (596 + orphaned `EventsScreen.tsx` 317), Chat (~390).

**Backend ~3.5k+ lines:** events stack (843 + db + 4 migrations, Mobilizon
leftovers, no-op `bootstrap_event_follows`), forums (295), exchange (410),
matrix shim (326), bluesky_chat (742), rss (428) + rss_sync (137, **never
started**), google_import (106, **pure stub** — marks "completed" without
importing), wall, cached_posts (unused inserts), Lemmy remnants (~10 dead match
arms + `fix_lemmy.py`), `games` orphan table, `rsky-firehose`/`palomar-sync`
vendored-unused. Load-bearing keepers: `vendor-rsky-min` (rsky-repo MST/commits
for the PDS), appview consumer + endpoints, auth/oauth, import parsers, profile.

## Roughness totals

- 0 TODO/FIXME in frontend (roughness is architectural, unannotated); 6 TODOs in
  backend (the landmines above).
- ~15 `console.log` debug lines in feed hot path; multiple silent `catch {}`;
  `#![allow(dead_code)]` at top of `api/mod.rs` and `db/mod.rs`.
- Frontend size: ~14,100 TS/TSX lines + 6,097-line `styles.css`. Backend src:
  ~30,000 lines (92 files). Monoliths: `feed/mod.rs` 3578, `oauth/bluesky.rs`
  2048, `import_facebook.rs` 2040, `profile.rs` 1402.

## Verdict

**Keep — from scratch is not justified.** Steps 1, 2, 4 are demo-able today
(import via Accounts fallback), step 5's editor is real, step 6's UI is done.
The demo gaps are rewiring (ImportResults, tour selector, preset themes) — days,
not months. The real work is: (0) put code under version control, (1) demo
blockers, (2) security/migration landmines, (3) dead-weight cut (~6.5k lines
across both sides), (4) feed `mod.rs` consolidation + media-first mode.

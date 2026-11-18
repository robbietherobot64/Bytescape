# Bytescape v2 — Screen-by-Screen UI Planning Index

> **Status note (2026-10-10):** these are **v2 design specs** — target-state designs, not
> descriptions of shipped code. Implementation status varies per screen (Profile and
> Feed are largely built; Settings and Notifications screens do not exist yet), and the
> nav model described below is not what ships today (the code still uses the v1 9-tab
> shell). Where a spec conflicts with `DECISIONS.md`/`VISION.md` (phase-1 signup, feed
> direction), the canon wins; the feed spec is superseded in direction by
> `openspec/changes/feed-refocus/`. Citations of AD-07/AD-12/AD-13 refer to decisions
> that were never recorded in `wiki/architecture/decisions.md`.

**Status:** 12 screen specs exist (the index below lists 13 — `groups-forums.md` was
never written). See the status note above.
**Date:** 2026-08-12
**Epic:** Plane `[Epic] v2 UI Planning — Screen-by-Screen Design`

This index is the cross-cutting inventory produced by the methodical per-screen process:
(1) v1 implementation inventory → (2) PRD FR mapping → (3) salvage verdict → (4) design spec → (5) Plane work items.

## Master inventory

**Addendum (2026-11-02):** Stories — a cross-cutting surface, not a route: the
rail (below the tab row, post-width-bounded), full-screen viewer with post
interactions, composer (media + status modes), Stories explorer overlay, and
the Vault story archive. Specs live in `openspec/changes/add-stories/specs/`
(since synced); cross-protocol mapping in
`wiki/architecture/stories-cross-protocol.md`.

| # | Screen | Route | PRD FRs | Salvage | v1 sources (lines) | Spec |
|---|---|---|---|---|---|---|
| 1 | Profile | `/@{handle}` | FR-9, FR-11, FR-24 | **HIGH** — keep + refine | `ProfileScreen.tsx` (638), `components/profile/**` (~2,050), `components/profile/widgets/**` (~1,575) | `profile.md` |
| 2 | Feed | `/` | FR-10, FR-13, FR-14, FR-15 | **HIGH** — keep + refine | `FeedScreen.tsx` (33), `FeedLayout.tsx` (755), `FeedColumn.tsx` (468), `PostCard.tsx` (1,276), `CategorySidebar.tsx` (303), `SocialPanel.tsx` (203), `ComposerModal.tsx` (190) | `feed.md` |
| 3 | Onboarding / Login | `/login` | FR-27, FR-30 | **MEDIUM** — dual-PDS rework | `LoginFlow.tsx` (118), `BlueskyLanding.tsx` (76), `OnboardingTour.tsx` (158), `CoachMark.tsx` (230), `TerminalBoot.tsx` (61), `TerminalMenu.tsx` (68), `InterestStep.tsx` (139) | `onboarding-login.md` |
| 4 | Import | `/import` | FR-1, FR-2, FR-3 | **MEDIUM** | `ImportStep.tsx` (471), `ImportResults.tsx` (441) | `import.md` |
| 5 | Vault | `/vault` | FR-5 | **MEDIUM** | `VaultScreen.tsx` (110) | `vault.md` |
| 6 | Events | `/events` | FR-17 | **MEDIUM** | `EventsScreen.tsx` (317), `EventsColumn.tsx` (407), `EventsLayout.tsx` (49), `EventsSidebar.tsx` (79), `EventsRightPanel.tsx` (61) | `events.md` |
| 7 | Exchange | `/exchange` | FR-21 | **LOW** — redesign | `ExchangeScreen.tsx` (499) | `exchange.md` |
| 8 | Chat / DMs | `/chat` | FR-19 | **LOW** — Bluesky chat | `ChatScreen.tsx` (129), `apiChat.ts` (54) | `chat-dms.md` |
| 9 | Groups / Forums | `/groups` | FR-20 | **LOW** — `space.bytescape.*` | `ForumsScreen.tsx` (334) | `groups-forums.md` |
| 10 | Following | `/following` | FR-4, FR-10 | **MEDIUM** | `FollowingScreen.tsx` (183) | `following.md` |
| 11 | Accounts | `/accounts` | FR-27, FR-30 | **LOW** — dual-PDS | `AccountsScreen.tsx` (764) | `accounts.md` |
| 12 | Settings | `/settings` | FR-26, §8 Security | **NEW** | — | `settings.md` |
| 13 | Notifications | `/notifications` | FR-25 | **MEDIUM** | `NotificationBell.tsx` (199) | `notifications.md` |

Total v1 frontend: **~20,245 lines** (`styles.css` 6,097 + 65 `.tsx`/`.ts` files ~14,148), 12 screens + 55 components, 6 utils.

## Salvage summary

- **HIGH (2):** Profile, Feed — user-confirmed good; keep concepts and most code, refine internals.
- **MEDIUM (6):** Onboarding/Login, Import, Vault, Events, Following, Notifications — keep concept, rework for v2 architecture.
- **LOW (4):** Exchange, Chat/DMs, Groups/Forums, Accounts — v2 architecture (dual-PDS, Bluesky chat, `space.bytescape.*`, app-DB state) changes the model; redesign, mine v1 for parts.
- **NEW (1):** Settings.

## Cross-cutting v2 architecture deltas that reshape screens

1. **Dual-PDS (AD-01b).** Every screen renders two user classes — Bytescape-hosted (`did:web`) and Bluesky-hosted (`did:plc`). The frontend calls one API; the split is invisible except where noted (identity edit, composer latency, account model).
2. **Mastodon/Lemmy/AP removed (AD-00).** All five-platform UI (FollowingScreen tabs, AccountsScreen fediverse linking, PostCard Mastodon source badges, cross-post-to-Mastodon toggle) is dropped.
3. **RSS deferred to Phase 2 (FR-7/FR-8).** RSS source category, RSS subscriptions, and RSS social-graph subscriptions leave the v2.0 surface.
4. **Chat = Bluesky DMs (AD-01d).** Matrix client is dead; 1:1 DMs ride `chat.bsky.conversation.*`.
5. **Groups = `space.bytescape.*` (FR-20).** Forums (community topics) → Space/Room/Message structure.
6. **Widgets/wall/comments = app-DB, not PDS records** (decision 2026-08-09). Profile customization writes to `users.*` tables, invisible to the PDS split.
7. **Notifications/mute lists = app-DB** (FR-25/FR-26), generated by the AppView consumer + app-DB events.

## Navigation model (v2)

The v1 TabBar (`Feed · Forums · Exchange · Events · Accounts · Profile · Chat · Vault · Following`) is reworked to:

- **Pinned primary tabs:** Feed (unclosable), Profile, Vault, Events, Exchange.
- **Hamburger/"More" menu:** Chat, Groups, Following, Accounts, Settings.
- **Global chrome:** NotificationBell (→ Notifications screen) + identity menu, in the top bar.
- **Mobile:** bottom nav `Feed · Profile · ＋ · Chat · More` (v1 `MobileNav` pattern carried forward).

Each spec states its own tab placement; the shell is documented inline (there is no separate shell screen in scope).

---

## Where the code lives (verified 2026-10-10)

The specs above are designs; this is where each area is actually implemented today.

| Area | Frontend | Backend |
|---|---|---|
| Profile & widgets | `apps/web/src/screens/ProfileScreen.tsx`, `src/components/profile/ProfileGrid.tsx`, `widgetTypes.ts`, `WidgetThemeEditor.tsx` | `bytescape-server/src/api/profile.rs` |
| Feed | `src/components/FeedLayout.tsx`, `FeedColumn.tsx`, `PostCard.tsx` | `src/api/feed/mod.rs` + `feed/{sources,filter,normalize,sort,paginate}.rs`, `src/api/posts/` |
| Import | `src/components/ImportStep.tsx`, `ImportResults.tsx` | `src/api/import.rs`, `import_facebook.rs`, `import_instagram.rs`, `src/db/imports.rs` |
| Events | `src/components/EventsLayout.tsx`, `EventsColumn.tsx`, `EventsSidebar.tsx` | `src/api/events.rs`, `src/db/events.rs` |
| Vault | `src/screens/VaultScreen.tsx` (media grid only) | `src/api/vault.rs` |
| Accounts | `src/screens/AccountsScreen.tsx`, `src/components/OAuthCallback.tsx` | `src/api/accounts.rs`, `src/api/oauth.rs`, `src/oauth/bluesky.rs` |
| Notifications | `src/components/NotificationBell.tsx` (bell + polling) | `src/api/notifications.rs`, `src/db/notifications.rs` |
| Following | `src/screens/FollowingScreen.tsx` | `src/api/follow.rs`, `src/db/follows.rs` |
| Settings | *(no screen — appearance lives in ProfileScreen/ProfileToolbar)* | `src/api/settings.rs` (posting identities only) |
| Database | — | `bytescape-server/migrations/`, `src/db/` (see `wiki/architecture/database.md`) |

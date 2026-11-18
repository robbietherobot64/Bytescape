# Screen Spec: Profile (v2)

**Status:** Draft — all questions resolved 2026-08-09 (four decision rounds: wall deferred / widgets in app DB / stack kept; interests simplified / active visitor counter / pagination note; visitor counter old-school redesign; option-B phase re-draw — wall, custom pages, CSS editor pulled into launch scope)
**Date:** 2026-08-09
**Route:** `/@{handle}` (bytescape.social/@{handle})
**Satisfies:** FR-9 (Customizable Profiles), FR-11 (Profile Feed), FR-24 (Wall Posts — launch scope per option-B re-draw); contributes to FR-10 (follow actions), FR-13 (composer entry point), FR-5 (import entry point), SM-5 (≥40% customize within 14 days)

**Sources:**
- v1 implementation: `apps/web/src/screens/ProfileScreen.tsx` (638 lines), `src/components/profile/**` (18 files, ~2,050 lines), `src/components/profile/widgets/**` (15 files, ~1,575 lines), `src/styles.css` (profile sections)
- v1 redesign spec: `_ref/artifacts/_bmad-output/evolution/specs/profile-layout-redesign-spec.md`
- v2 constraints: `wiki/product/prd.md` (FR-9, FR-11, FR-24 launch scope), `wiki/architecture/database.md` (users table), `wiki/architecture/decisions.md` (AD-01b dual PDS, AD-01c lexicons)

> **Decision 2026-08-09 — widget storage:** Widgets are NOT a custom lexicon. There are no `com.bytescape.widget.*` records. Widget layout, per-widget config, and themes live in the application database (`users.widget_layout`, `users.widget_themes`, `users.theme`, `users.custom_css`). Only profile identity (name, bio, avatar, banner) stays an `app.bsky.actor.profile` record. This overrides the PRD FR-9 consequence "one-record-per-widget pattern" — that pattern is dead. The v1 storage model (and the v1 save API path) is confirmed as the v2 model.

> **Decision 2026-08-09 (round 3) — theme scope: app-wide by default, opt-out checkbox.** The profile theme applies across the whole app by default — the signed-in user's selected theme (Terminal / 1999 / XP Aero) drives the app chrome beyond the profile. A **"Apply theme across the app"** checkbox (default checked) sits with the theme selector in the identity edit modal; unchecked, the theme stays profile-only and the rest of the app keeps the default Bytescape theme. Flag stored in the app DB (`users.theme_app_wide`, consistent with the widget-storage decision). Visiting another profile always renders that profile's owner theme — the visitor's own theme never overrides someone else's profile. See 4.5, R-10.

> **Decision 2026-08-09 (round 3) — visitor counter: old-school hit counter, comments decoupled.** Replaces OQ-10 / NQ-2. The counter counts **every profile page load — owner included** — a nostalgic "times this page has been loaded" number. First-party counting is not the third-party tracking (Site Meter, digits.com cookies) that killed the old counters, so: no Register button, no dedup, no optimistic states, no login redirect for counting. The comment affordance decouples from counting: a 📝 button opening the **comment popup** (same shell as the post composer, text/media only), **login required** (identity from the handle — no anonymous spam), comment appears in the widget's recent list. Owner can **delete** any comment; deletion never touches the count. A new comment notifies the owner via the existing `notifications` table + bell (FR-25 surface — `NotificationBell.tsx`, 30s polling). Plain visits never notify. Full wall (FR-24) stays deferred; these comments are its Phase 2 seed data. See 1.2, 4.6, 5.4, 6.8.

---

## 1. Overview

The profile screen is a user's customizable home page: identity header, post history, and a set of user-arranged widgets. It is the product's MySpace-inspired centerpiece — the widget grid, per-widget theming, and custom profile fields are core identity, not decoration. FR-9 is validated by success metric SM-5, so the customization surface must be prominent and inviting, not buried.

The screen serves three audiences with different affordances:

| Viewer | Sees | Can do |
|---|---|---|
| Owner | Full profile + edit affordances | Edit identity, customize layout/widgets/themes, post/import |
| Authenticated visitor | Public profile + follow | Follow/unfollow |
| Unauthenticated visitor | Public profile (FR-9: unauthenticated viewing) | View only; follow prompt routes to login |

### 1.1 Dual PDS rendering (AD-01b)

The same screen renders two user classes:

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Identity record | `app.bsky.actor.profile` on Bytescape PDS | `app.bsky.actor.profile` on bsky.social PDS |
| Widget layout / themes / custom CSS | App DB (`users.widget_layout`, `users.widget_themes`, `users.theme`, `users.custom_css`) | Same — app DB. Widgets never touch the PDS, so customization works identically for both user classes and needs no extra OAuth scopes |
| Identity edit writes | Direct PDS write | XRPC to bsky.social (AD-01b write table) |

Frontend consequences:
- A user (either class) who never customized has empty `widget_layout` → render `DEFAULT_LAYOUT` (section 3.1) and treat layout as unsaved until they edit.
- Widget/layout persistence is an app-DB concern for both user classes; only identity fields (name, bio, avatar, banner) follow the PDS/XRPC split. The frontend just calls the profile API — the split is invisible to it.
- An optional small badge on the header distinguishes hosted vs. Bluesky accounts (e.g. `🦋 bluesky account` / `📦 bytescape home`) — useful context for visitors, cheap to render from `users.user_type`. See refinement R-4.

### 1.2 Data sources (v2)

| Data | Location | Notes |
|---|---|---|
| Name, bio, avatar, header | `app.bsky.actor.profile` record | Standard; visible on Bluesky too (AD-01c) |
| Extended fields (location, website, pronouns, birthday, relationship, occupation, interests, custom sections) | App DB: `users` columns + `users.custom_profile_fields` JSONB | Bytescape-only; v1 shape kept |
| Widget placement, size, per-widget data | App DB: `users.widget_layout` JSONB | v1 model confirmed (decision 2026-08-09). One blob per user, same shape as v1 `Widget[]` |
| Per-widget themes | App DB: `users.widget_themes` JSONB | v1 shape kept: `{id, name, background, border_color, border_radius, text_color, font_family, padding, shadow}` |
| Preset theme | App DB: `users.theme` | Terminal / 1999 / XP Aero. Applies app-wide by default (`users.theme_app_wide`, round-3 decision); opt-out checkbox in the identity modal |
| Custom CSS | App DB: `users.custom_css` | Allowed; editor ships in v2.0 (option-B re-draw) — sanitized/allowlisted subset, never applies to edit-mode chrome (see 5.5) |
| Profile feed | AppView query mixing `app.bsky.feed.post` + `com.bytescape.archive.post` (FR-11) | Chronological, filterable by source/type |
| Visitor count + visit comments | App DB (server-side) | Incremented on **every profile page load — owner included** (round-3 decision). No passive *tracking* — the server already sees the request; no analytics, no cookies, no third parties. Count persists whether or not the widget is placed. Comments: login-required, owner-deletable, notify the owner (see `visitor_counter` in 4.6) |
| Wall | App DB (server-side) | **Launch scope** (option-B re-draw). Wall posts stored in the app DB, not PDS records (decision 2026-08-09). Login-required, owner-deletable, owner notified. See 4.3, 6.4 |

### 1.3 What changed vs. v1 at a glance

1. Widget storage stays in the app DB — the v1 model is confirmed, not migrated to PDS records (decision 2026-08-09). The v1 save path (`PUT /api/profile/me` with `widget_layout` + `widget_themes`) survives as-is.
2. Profile feed gains source/type filtering (FR-11) — v1 API supports filters; v1 UI never exposed them.
3. Widget catalog completes: 4 widget components exist in v1 but are unreachable (About, AvatarName, Banner, Stats), plus a new `visitor_counter` widget and the `wall` widget (registered — launch scope per option-B re-draw).
4. Preset themes actually apply: v1 saves `theme` but the CSS targets a deprecated container.
5. Custom nav pages move from localStorage into the nav widget's saved data (app DB) — no more per-device loss.
6. Edit-mode shell is de-duplicated (v1 ProfileScreen: 15 useState hooks, 3 render branches) — on the **kept** stack and patterns (React 19 + Vite + TS, useState/useEffect, localStorage caching, custom DOM event bus; decision 2026-08-09).
7. Nav becomes a movable widget whose items switch pages: Profile (the grid), Followers and Following (each a single full-size widget) — see 3.4.
8. Wall (FR-24) is **launch scope** (option-B re-draw): app-DB storage, login-required, owner-deletable, owner notified. Private profiles remain a planned slot-in when the AT Protocol privacy spec lands — the wall is no longer gated on it.

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is or with trivial touch-ups), **REFINE** (keep concept and most code, improve), **REWORK** (keep concept, rebuild internals), **DROP** (obsolete; mine for parts), **DEFER** (concept preserved intact, design work postponed to Phase 2).

Summary: **18 KEEP · 8 REFINE · 6 REWORK · 4 DROP · 0 DEFER** — consistent with the decision that the v1 profile UI is good and gets salvaged with refinement, and with the option-B re-draw pulling the wall into launch scope.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `screens/ProfileScreen.tsx` (638) | **REWORK** | Right behaviors, unmaintainable shell: 15+ useState hooks, 3 duplicated render branches, window CustomEvent bus, dead code paths, desktop follow button missing |
| 2 | `ProfileGrid.tsx` (215) | **REFINE** | The 20-col drag/resize engine (react-grid-layout, `compactType=null`, `preventCollision`) implements FR-9 exactly; needs widget registry completion + 900px breakpoint |
| 3 | `ProfileToolbar.tsx` (95) | **REFINE** | Edit/Preview/Add/Reset/Save/Cancel flow is right; palette incomplete, needs sticky bar + unsaved-changes guard |
| 4 | `WidgetThemeEditor.tsx` (116) | **KEEP** | Per-widget bg/border/radius/text/font/padding/shadow editor = FR-9 per-widget customization, fully built. Comic Sans option stays |
| 5 | `ProfileNav.tsx` (211) | **KEEP** | Clean nav with default items, removable custom items, inline Add-page form (Blog/Gallery/Custom Link). Needs app-DB persistence (nav widget data) + real page switching |
| 6 | `ProfileHeaderBar.tsx` (137) | **REFINE** | Good compact header (banner, overlapping avatar, stats row, edit button); missing follow button, detail chips, theme hooks |
| 7 | `ProfileHeader.tsx` (142) | **DROP** | Gen-1 header superseded by WidgetProfileHeader; mine the chip row + `ageFromBirthday` helper (currently duplicated in 3 files) |
| 8 | `ProfileFeed.tsx` (270) | **REFINE** | [+] Post/Import dropdown, post cards, portal modals — keep; add filter bar (FR-11), fix load-more pagination bug |
| 9 | `ProfilePosts.tsx` (42) | **KEEP** | Simple list renderer with solid empty/loading states; folds into ProfileFeed |
| 10 | `ProfileWall.tsx` (88) | **REWORK** | "📖 Sign My Guestbook!" guestbook is the wall's soul — launch scope (option-B re-draw). Rebuild as the wall widget: login-required posting, owner-delete, owner notification, promote-to-feed. Fake hardcoded visitor counter spun out into the real `visitor_counter` widget instead |
| 11 | `InterestWidget.tsx` (171) | **KEEP** | Tag list + gear → picker modal, save-with-revert, max 20. Consolidate with its near-twin `WidgetInterests.tsx` |
| 12 | `ProfileEditForm.tsx` (185) | **REWORK** | Unwired in v1 screen, but holds the full identity field form + theme swatch selector (Terminal/1999/XP); rebuild as the identity edit modal |
| 13 | `ProfileDetails.tsx` (125) | **DROP** | Duplicate of ProfileEditForm field set, unwired |
| 14 | `MobileProfileRender.tsx` (120) | **REWORK** | Unwired, but its accordion grouping (About Me / My Posts / Connections / More) is the right mobile pattern; rebuild as the mobile renderer |
| 15 | `ProfileSidebarLeft.tsx` (39) | **KEEP** | Friends list body (avatars, fallbacks, click-through) |
| 16 | `ProfileSidebarRight.tsx` (42) | **KEEP** | Following list body incl. `🌐` external-account marker — dual-PDS-aware already |
| 17 | `ProfileTabs.tsx` (29) | **DROP** | Wall/Posts/Edit tabs superseded by widgets + nav |
| 18 | `ProfileLayout.tsx` (18) | **DROP** | Gen-1 CSS 3-column shell superseded by grid; its `theme-*` class hook concept moves to the grid container (see R-10) |
| 19 | `widgetTypes.ts` (11) | **REFINE** | Add missing types (`about`, `avatar_name`, `banner`, `stats`, new `visitor_counter`; `wall` registered — launch scope); grow into a widget registry with labels/icons/defaults |
| 20 | `widgets/WidgetProfileHeader.tsx` (697) | **REWORK** | Most valuable, messiest: banner + avatar + configurable fields + custom sections + portal config panel + inline editing. The `custom_profile_fields` system must survive; split into 3 subcomponents |
| 21 | `widgets/WidgetInterests.tsx` (156) | **KEEP** | Merge into one Interests implementation with #11 |
| 22 | `widgets/WidgetNavigation.tsx` (33) | **KEEP** | Thin wrapper over ProfileNav |
| 23 | `widgets/WidgetPosts.tsx` (48) | **REFINE** | Data-fetch wrapper; merge with ProfileFeed, fix offset pagination |
| 24 | `widgets/WidgetWall.tsx` (57) | **REWORK** | Works (fetch, post, paginate) but unregistered in the grid — register and rebuild for launch scope (option-B re-draw): app-DB storage, login-required, owner-delete, owner notification |
| 25 | `widgets/WidgetFriends.tsx` (21) | **KEEP** | Fetch wrapper over ProfileSidebarLeft |
| 26 | `widgets/WidgetFollowing.tsx` (21) | **KEEP** | Fetch wrapper over ProfileSidebarRight |
| 27 | `widgets/WidgetMusicPlayer.tsx` (92) | **KEEP** | Spotify/Bandcamp/SoundCloud embeds; fix bug: regex matches album/playlist URLs but embed always builds `/embed/track/` |
| 28 | `widgets/WidgetPhotoGallery.tsx` (90) | **REFINE** | Grid + lightbox good; replace base64-dataURL storage with real media upload (bloats the `widget_layout` JSONB) |
| 29 | `widgets/WidgetCustomLinks.tsx` (101) | **KEEP** | Link list CRUD, fine as-is |
| 30 | `widgets/WidgetStatus.tsx` (37) | **KEEP** | "💬 status/mood" one-liner, fine as-is |
| 31 | `widgets/WidgetAbout.tsx` (94) | **REFINE** | Unregistered; overlaps WidgetProfileHeader fields — rework as standalone About box fed by `custom_profile_fields` |
| 32 | `widgets/WidgetAvatarName.tsx` (74) | **KEEP** | Unregistered; register in catalog |
| 33 | `widgets/WidgetBanner.tsx` (39) | **KEEP** | Unregistered; register in catalog |
| 34 | `widgets/WidgetStats.tsx` (16) | **KEEP** | Unregistered; register + make counts clickable |

### 2.1 v1 defects fixed by this spec (evidence-grounded)

1. **Load more is broken** — `ProfileFeed.tsx` "Load more" calls `loadPosts()` which always fetches `offset 0`; the cursor from `FeedResponse` is stored but never used. (`WidgetPosts.tsx` does pagination correctly with an offset counter — use that pattern.)
2. **No follow button on desktop** — `FollowButton` renders only in the mobile/tablet branches of ProfileScreen; desktop visitors can't follow.
3. **Nav is decorative** — `setActiveNav` changes highlight only; Followers/Following/custom pages render nothing.
4. **Custom nav pages live in localStorage** (`bytescape_nav_items`) — lost across devices. Fixed by the 2026-08-09 decision: nav is a widget, its items persist in `users.widget_layout` (app DB).
5. **5 widget types unreachable** — `WidgetAbout/AvatarName/Banner/Stats/Wall` exist but are absent from `widgetTypes.ts`, the `ProfileGrid.renderWidget` switch, and the toolbar palette. (About/AvatarName/Banner/Stats get registered; Wall gets registered and rebuilt for launch scope per the option-B re-draw; a new `visitor_counter` widget joins the catalog.)
6. **Themes don't apply** — `ProfileEditForm` saves `theme` (`terminal`/`1999`/`xp`), and `styles.css` defines `.profile-layout.theme-terminal` / `.theme-1999` / `.theme-xp` — but `.profile-layout` (ProfileLayout.tsx) is no longer mounted; the widget grid never receives a theme class.
7. **Fake visitor counter** — `ProfileWall.tsx` hardcodes `001,247 visitors`. Fixed properly: a real `visitor_counter` widget counting every profile page load server-side (round-3 decision: old-school hit counter — no Register button; count persists whether or not the widget is placed).
8. **Dead edit path** — ProfileScreen's `showInfoForm`/`handleOpenInfoForm`/`handleSaveInfo` are never rendered; identity editing happens inline inside WidgetProfileHeader only.
9. **Interest picker result via localStorage** — InterestWidget reads `bytescape_interests` back from localStorage after the modal closes instead of receiving the selection as a callback.
10. **Base64 photos in widget data** — WidgetPhotoGallery stores FileReader dataURLs in `widget.data.photos`.
11. **ProfileScreen duplication** — 15+ useState hooks and three near-identical render branches (desktop/tablet/mobile). The window CustomEvent bus itself (`bytescape:refreshFeed`, `bytescape:profileUpdated`, `bytescape:interestsUpdated`, `bytescape:tourAction`, …) is **kept** per the 2026-08-09 stack decision — the rework targets duplication, not the state approach.

---

## 3. Layout

The v2 profile keeps the v1 default arrangement, which already realizes the redesign spec's 3-column structure as widgets on the 20-column grid: nav `w=5` (25%), header `w=10` `h≈15` + feed `w=10` (50%), interests `w=5` (25%). The nav is itself a widget — placed left by default, movable/deletable like any other (decision 2026-08-09).

### 3.1 Default layout (no customization yet)

```
┌────────────────────────────────────────────────────────────────────────────┐
│ TOOLBAR (own profile only)                    [✏️ Edit Profile]            │
├──────────────────┬─────────────────────────────────────┬───────────────────┤
│ NAVIGATION  25%  │  HEADER (profile_header widget) 50% │ INTERESTS    25%  │
│ ┌──────────────┐ │  ┌─────────────────────────────────┐│ ┌───────────────┐ │
│ │ Navigation   │ │  │ [banner image]                  ││ │ ⭐ Interests ⚙│ │
│ │  (widget —   │ │  │   ┌────┐  Display Name          ││ │               │ │
│ │   movable)   │ │  │   │ av │  @handle               ││ │ 🎮 Gaming     │ │
│ │ ▸ 👤 Profile │ │  │   └────┘  bio text...           ││ │ 🎵 Music      │ │
│ │ ▸ 👥 Followers│ │  │   📍 loc · 🎂 bday · ⭐ ints    ││ │ 📚 Books      │ │
│ │ ▸ 👤 Following│ │  │   12 posts · 45 followers · 30  ││ │               │ │
│ │              │ │  └─────────────────────────────────┘│ └───────────────┘ │
│ │ ──────────── │ │  ┌─ FEED (posts widget) ────────[+]┐│ │ ┌───────────────┐ │
│ │ + Add page   │ │  │ [All ▾] [Source ▾] [Type ▾]     ││ │ 👁 001,043     │ │
│ │  (own only)  │ │  │ ┌─ post card ──────────────────┐││ │               │ │
│ └──────────────┘ │  │ │ author · date · 📦 imported  │││ │ [📝 Leave a   │ │
│                  │  │ └──────────────────────────────┘││ │  comment]     │ │
│ (FOLLOW button   │  │ ┌─ post card ──────────────────┐││ │               │ │
│  for visitors —  │  │ └──────────────────────────────┘││ │ @mika — "love │ │
│  in header)      │  │        [ Load more ]            ││ │  the theme!"  │ │
│                  │  └─────────────────────────────────┘│ │ @jon — "back  │ │
│                  │                                     │ │  from 1999"   │ │
│                  │                                     │ └───────────────┘ │
├──────────────────┴─────────────────────────────────────┴───────────────────┤
│  grid: 20 cols · rowHeight 10 · no compaction · margin 16px               │
└────────────────────────────────────────────────────────────────────────────┘
```

Notes:
- Everything on the screen is a widget — including the nav. The columns are the *default* placement, not hard structure — users can drag the interests widget under the feed, move the nav to the right side, add a music player, delete the nav entirely. FR-9 requires we keep this freedom.
- The **wall is launch scope** (option-B re-draw) — its default placement is the center column below the feed.
- The right column is the natural home for optional widgets (visitor counter, music player, photo gallery, links, status, friends, following) — the v1 default just ships Interests there.
- Selecting Followers/Following in the nav swaps the whole grid for a single full-size widget — see 3.4.

### 3.2 Tablet (900–1200px)

Nav widget collapses out of the left column into a horizontal pill bar above the content (v1's tablet branch already does this); grid drops to two effective columns.

```
┌────────────────────────────────────────────────────────────┐
│ [👤 Profile] [👥 Followers] [👤 Following] [+ Add page]     │  ← nav pills
├────────────────────────────────────────┬───────────────────┤
│  HEADER                                │ INTERESTS         │
│  ┌────────────────────────────────────┐│ ┌───────────────┐ │
│  │ banner · avatar · name · bio · ⋯  ││ │ tags…         │ │
│  └────────────────────────────────────┘│ └───────────────┘ │
│  FEED  [+]                             │ (extra widgets    │
│  ┌────────────────────────────────────┐│  stack below)     │
│  │ post cards…                        ││                   │
│  └────────────────────────────────────┘│                   │
└────────────────────────────────────────┴───────────────────┘
```

### 3.3 Mobile (≤900px)

Single column (PRD FR-9: collapses below 900px — v1's grid used 768px; align to 900). Nav becomes a hamburger dropdown (v1 pattern kept). Widgets stack in **mobile order — a separate arrangement from the desktop grid** (decision 2026-08-09): moving a widget up/down on mobile changes the mobile stack only and never affects grid position. Optional: accordion grouping per MobileProfileRender (see 7.2).

```
┌──────────────────────────┐
│ ← Back to my profile     │  (when viewing someone else)
│ ┌──────────────────────┐ │
│ │ ☰ Navigation       ▼ │ │
│ └──────────────────────┘ │
│ ┌─ HEADER ─────────────┐ │
│ │ banner/avatar/name   │ │
│ │ bio · chips · stats  │ │
│ │ [ Follow ] / [ Edit ]│ │
│ └──────────────────────┘ │
│ ┌─ FEED ────────────[+]┐ │
│ │ filters              │ │
│ │ post cards…          │ │
│ └──────────────────────┘ │
│ ┌─ INTERESTS ──────────┐ │
│ └──────────────────────┘ │
│ (more widgets stack      │
│  in the user's mobile    │
│  order — independent of  │
│  the desktop grid)       │
└──────────────────────────┘
```

### 3.4 Nav pages (Profile / Followers / Following)

Decision 2026-08-09: Followers and Following are not separate screens — they are widgets of their own, and the nav menu is also a widget that switches between pages:

```
┌──────────────────┬─────────────────────────────────────────────────────────┐
│ NAVIGATION       │                                                         │
│ ┌──────────────┐ │   FOLLOWING widget — expanded to full size              │
│ │ ▸ 👤 Profile │ │   ┌───────────────────────────────────────────────────┐ │
│ │ ▸ 👥 Followers│◄│───│ following list (avatars, handles, 🌐 external,   │ │
│ │ ▸ 👤 Following│ │   │ follow/unfollow actions, pagination)              │ │
│ └──────────────┘ │   └───────────────────────────────────────────────────┘ │
└──────────────────┴─────────────────────────────────────────────────────────┘
```

- **Profile** (default): the widget grid described in 3.1.
- **Followers**: the `friends` widget expanded to full size — everything except the nav bar.
- **Following**: the `following` widget expanded to full size — everything except the nav bar.
- The nav widget stays visible on all pages (it's the page switcher); on tablet/mobile it takes its collapsed pill/hamburger form.
- Custom nav pages (Blog, Gallery, Custom Link) render their page type — launch scope (option-B re-draw). Blog: post list + composer entry. Gallery: media grid. Custom Link: external redirect. Page content editing is a follow-up; the page shells ship in v2.0.

---

## 4. Component Breakdown

### 4.1 Screen shell

**`ProfileScreen` (REWORK)**
- Purpose: resolve the profile by handle, hold profile state, choose view vs. edit mode, lay out columns, switch nav pages.
- Props: `handle` (route param), viewer context from auth.
- State: `profile`, `status: loading | error | ready`, `mode: view | edit | preview`, `layout: Widget[]`, `themes: WidgetTheme[]`, `dirty: boolean`, `activeNav`.
- Loads via `GET /api/profile/{handle}` (unauthenticated allowed — public profiles), `GET /api/profile/me` for own.
- **Stack decision (2026-08-09):** React 19 + Vite + TypeScript with the existing patterns — useState/useEffect, localStorage caching, and the custom DOM event bus (`bytescape:profileUpdated`, `bytescape:refreshFeed`, …) are all KEPT. The rework removes duplication (15 useState hooks collapsed into a few grouped states or a reducer; one responsive render tree instead of three near-identical branches), not the state approach.
- Nav page switching: `activeNav` selects the page — Profile (widget grid), Followers (full-size `friends` widget), Following (full-size `following` widget), custom pages (Blog/Gallery/Custom Link shells — launch scope). See 3.4.

### 4.2 Left column (default placement)

**`ProfileNav` (KEEP + wiring)** — body of the `navigation` widget
- Purpose: old-school site nav and page switcher for the profile area. It is a widget: placed left by default, movable/resizable/deletable like any other (decision 2026-08-09).
- Props: `items: NavItem[]`, `activeItem`, `onSelect`, `onAddPage(name, type)`, `onRemovePage(id)`, `isOwnProfile`. `NavItem = { id, label, type: 'default' | 'custom', icon?, pageKind? }`.
- Default items: Profile, Followers, Following (not removable). Custom items removable (hover ✕ — v1 behavior kept).
- Add-page form (v1 kept): name input + type select (Blog, Gallery, Custom Link) + Create/Cancel.
- **v2 changes:**
  - Items (including custom pages) persist in the nav widget's `data` inside `users.widget_layout` (app DB) — no more localStorage-only storage.
  - `onSelect` switches pages: Profile → the widget grid (3.1); Followers/Following → their single full-size widget with the nav still visible (3.4); custom pages → their page shell (Blog: post list + composer entry; Gallery: media grid; Custom Link: external redirect — launch scope per option-B re-draw).
  - If the owner deletes the nav widget, page switching goes with it — the Profile grid is still the screen's default view. (Restoring the nav is one palette click.)

### 4.3 Center column

**`ProfileHeaderBar` (REFINE)** — the default header widget body
- Purpose: banner, avatar, display name, handle, bio, detail chips, stats row, primary actions.
- Props: `profile`, `isOwnProfile`, `isFollowing`, `onEditProfile`, `onFollow`, `followLoading`.
- Kept from v1: banner with overlapping avatar (avatar `bottom: -40`, 3px bg-colored ring), initials fallback, edit button top-right on banner, stats row with top border.
- Added:
  - **Follow button** in header (all viewports) — fixes the desktop gap. Visitor: `Follow` / `Following`. Own: `✏️ Edit`. Unauthenticated: `Follow` → login redirect.
  - Detail chips row (mine from ProfileHeader/WidgetProfileHeader): 📍 location, 🎂 birthday/age, 💼 occupation, ❤️ relationship, 🏷️ pronouns, 🔗 website, ⭐ interests — driven by visible `custom_profile_fields` (same source as WidgetProfileHeader).
  - Account badge (bytescape-hosted vs bluesky-hosted), refinement R-4.
  - Theme hooks: colors/fonts come from widget CSS variables (`--widget-bg`, `--widget-text`, …) so per-widget theming applies.
- In edit mode this widget swaps to inline inputs (name, bio) and avatar/header click-to-upload — v1 WidgetProfileHeader behavior, kept.

**`ProfileFeed` (REFINE)** — the posts widget body
- Purpose: unified profile feed mixing `app.bsky.feed.post` and `com.bytescape.archive.post`, chronological (FR-11).
- Props: `username`, `isOwnProfile`.
- State: `posts`, `cursor`, `hasMore`, `filterSource: 'all' | source`, `filterType: 'all' | type`, `loading`.
- Kept from v1: `[+]` dropdown (✏️ Post → ComposerModal, 📦 Import → ImportStep, portaled to body), post card shape (author, date, text, media thumbs ≤4), empty state copy, own-profile-only `[+]`.
- Added:
  - **Filter bar**: `All` / source dropdown (Bluesky, Facebook, Instagram, …) / type dropdown (image, video, event, text). The v1 client already sends `source`/`date_from`/`date_to` query params — expose them. FR-11 also wants date sorting; expose via the same bar if cheap, else phase 2.
  - **Source badge on cards**: imported posts get a `📦 imported · {platform}` tag; live posts unmarked (or `🦋 live`). FR-11 treats them equally in ordering — the badge is the only visible difference, matching the PRD.
  - Pagination fixed: cursor/offset actually advances (WidgetPosts pattern).
  - Reshare affordance for archive posts (bytescape.social link / native reshare per FR-11) — uses shared PostCard actions, detailed in the post-card spec.

**`WallSection` — launch scope (option-B re-draw)**
- FR-24 (Wall Posts) is in v2.0. Wall posts are app-DB records (decision 2026-08-09), not PDS records — the earlier "blocked on Bluesky private-posts spec" deferral is superseded: the wall's visibility model (visible only on the recipient's profile) is enforced by the app, and private profiles remain a separate planned slot-in (5.5).
- v1 already contains a complete, lovable guestbook implementation (`ProfileWall.tsx` + `WidgetWall.tsx`: "📖 Sign My Guestbook!", zebra entries, "Submit Entry") — that skin is the starting point. Rebuild adds: login-required posting, owner-delete, owner notification (`profile_comment` type), promote-to-feed, and real posting rules.
- The wall widget's default placement is the center column below the feed (3.1). The visitor_counter widget's comments (6.8) are the wall's seed data.

### 4.4 Right column

**`InterestWidget` (KEEP, consolidated; picker REWORKED)**
- Purpose: interests as tags; gear opens the interest picker; saving bootstraps hashtag subscriptions.
- Props: `interests`, `isOwnProfile`, `onInterestsChange`.
- Kept: tag chips (accent tint, mono 11px), gear ⚙ own-only, `MAX_INTERESTS = 20` with scroll past 8, empty states ("No interests yet. Click ⚙ to add some." own / "No interests selected." visitor), saving indicator, revert-on-error. Read-only for visitors (no gear).
- Fixed: picker returns the selection via callback (no localStorage round-trip); single implementation shared by grid widget and any future surface (v1 has two near-identical copies).

**Interest picker (simplified 2026-08-09): static categories + manual entry.** The earlier suggestion-tree idea (pick a band → similar bands appear) was dropped as unnecessary — "the static list of category-based interests and manual entry should be enough on its own to do what we need for now."

```
┌─ Pick your interests ────────────────────────────────────────┐
│ [ type your own…                                    ] [Add]  │
│                                                              │
│  🎵 Music                    🎮 Gaming                       │
│  [ Rock ] [ Jazz ] [ Techno ] [ RPG ] [ FPS ] [ Retro ]     │
│                                                              │
│  📚 Books                    🎬 Film & TV                    │
│  [ Sci-fi ] [ Fantasy ] …    [ Anime ] [ Horror ] …         │
│                                                              │
│  🏃 Sports                   🍜 Food                         │
│  [ Cycling ] [ Climbing ] …  [ Cooking ] [ Coffee ] …       │
│                                                              │
│  Selected (7/20):  [🎸 Rock ×] [📚 Sci-fi ×] [The Cure ×] … │
└──────────────────────────────────────────────────────────────┘
```

- **Static category list**: v1's `INTEREST_CATEGORIES` structure (categories → interests with emoji + label) is the picker's content. Curating the list is an editorial task, not an engineering one.
- **Manual entry**: a text input for anything not in the list. Manual entries stored as-is.
- **No suggestion engine, no ranking, no API beyond what exists.** The picker is pure frontend over a static definition.
- Storage: one flat `users.interests` array mixing preset ids and manual strings (v1's `getLabel` fallback — unknown id renders as plain text — already supports this).

**Additional widget slots** — the right column (and any free grid space) hosts user-added widgets from the catalog (4.6).

### 4.5 Edit mode (owner only)

**`ProfileToolbar` (REFINE)**
- View mode: single `✏️ Edit Profile` button (own profile only — v1 kept).
- Edit mode: `👁 Preview` toggle · `+ Add Widget` (palette) · `🔄 Reset` · `💾 Save` · `Cancel`.
- Refinements: sticky top while editing; unsaved-changes confirmation on Cancel / route leave; palette sourced from the widget registry (all shipping types incl. previously unreachable ones + `visitor_counter` + `wall`).

**`ProfileGrid` (REFINE)**
- react-grid-layout Responsive, kept settings: 20 cols, `rowHeight=10`, `margin=[16,16]`, `compactType=null`, `preventCollision=true` (FR-9: "widgets stay where placed, no auto-compaction").
- `isDraggable`/`isResizable` only in edit mode (not preview).
- Theme resolution per widget → CSS variables (v1 mapping kept: `--widget-bg`, `--widget-border`, `--widget-radius`, `--widget-text`, `--widget-font`, `--widget-padding`, `--widget-shadow`).
- Edit-mode widget controls (v1 kept): `⚙` theme editor, `×` delete; header widget additionally gets `📝` configure-sections.
- Changes: breakpoints `{ lg: 1200, md: 900, sm: <900 → 1 col }` (align 900px per FR-9); config panel opens as a real modal instead of a window CustomEvent + fixed-position portal; `data-tour` anchors preserved.

**`WidgetThemeEditor` (KEEP)**
- Modal: theme name, background color (picker + raw input), border color, border-radius slider 0–24, text color, font family (System/Monospace/Retro/Comic Sans/Segoe UI), padding slider 0–40, shadow.
- Refinements: live preview of the target widget while editing; optional "apply to all widgets" action; preset swatches per active profile theme.

**`IdentityEditModal` (REWORK from ProfileEditForm)**
- Purpose: edit identity fields that map to `app.bsky.actor.profile` + Bytescape extras, without entering grid edit mode.
- Fields (v1 set kept): display name, bio, avatar upload, header upload, location, website, pronouns, relationship status (v1 option list), birthday, occupation, interests (opens the interest picker, 4.4).
- Theme selector (Terminal / GeoCities 1999 / Windows XP Aero swatches — v1 UI kept) moves here or into the toolbar; see R-10 for actually applying it. The selector carries the **"Apply theme across the app"** checkbox (default checked — round-3 decision): on, the theme drives the whole app's chrome; off, it stays profile-only. Stored as `users.theme_app_wide`.
- Custom CSS: not in this modal. It gets its own power-user surface, shipping in v2.0 (option-B re-draw; see 5.5).
- Save writes via the profile update path; for Bluesky-hosted users the backend forwards identity fields to bsky.social XRPC (AD-01b) — the frontend just calls the API and surfaces per-field failures.

### 4.6 Widget catalog (registry)

Single registry (`widgetTypes.ts` grown up): id, label, icon, default size, data schema, configurable-in-place? — drives the palette, the grid switch, and mobile grouping.

| Widget type | v1 status | v2 notes |
|---|---|---|
| `profile_header` | wired | Banner+avatar+fields+stats; configurable sections (custom_profile_fields) |
| `navigation` | wired | ProfileNav body; movable widget, drives page switching (3.4) |
| `posts` | wired | ProfileFeed body |
| `interests` | wired | InterestWidget body |
| `friends` | wired | ProfileSidebarLeft body; doubles as the full-size Followers page (3.4). Full-size mode needs paginated endpoints — backend implementation concern, tracked separately (not a design question) |
| `following` | wired | ProfileSidebarRight body; doubles as the full-size Following page (3.4). Same pagination note as `friends` |
| `music_player` | wired | Spotify/Bandcamp/SoundCloud embeds; embed-type bug fixed |
| `photo_gallery` | wired | Real uploads instead of base64; lightbox kept |
| `custom_links` | wired | As-is |
| `status` | wired | As-is |
| `about` | **unwired** | Standalone About box over custom_profile_fields |
| `avatar_name` | **unwired** | Register (for layouts without the big header) |
| `banner` | **unwired** | Register |
| `stats` | **unwired** | Register; counts clickable → Followers/Following pages |
| `visitor_counter` | **NEW in v2** | **Old-school hit counter (round-3 decision).** Every profile page load increments the count server-side — owner included. Retro odometer digits (fixed width, leading zeros, layout never jumps). Read-only; no dedup, no manual trigger, no tracking semantics. Below the digits: a **"Leave a comment…"** affordance (login required) opening the **comment popup** — same shell as the post composer, simplified to text/media only (round-3 decision: same popup spec as ComposerModal minus mood/tagging/location/visibility; see 6.8). Comments appear in the widget's recent list; owner notified (`profile_comment` notification type via the existing bell); owner sees ✕ per comment to delete; deletion never touches the count. Count persists server-side whether or not the widget is placed, so removing/re-adding never loses it. Not the full wall (still deferred) — flat comments, no threading. See 6.8 |
| `wall` | **unwired** | **Launch scope** (option-B re-draw). Guestbook skin ("📖 Sign My Guestbook!", zebra entries) rebuilt: app-DB storage, login-required posting, owner-delete, owner notification, promote-to-feed. Default placement: center column below the feed. Visitor_counter comments are its seed data |

Ships in v2.0: 16 widget types — the PRD's "12+" fully delivered, including the wall.

### 4.7 Shared bits

- **`FollowButton`**: optimistic update with count ±1 and rollback on error (v1 pattern kept), loading state `...`, hidden for own profile and while in edit mode.
- **`BackButton`**: "← Back to my profile" when owner views via navigation; v1's `bytescape:viewOwnProfile` event pattern kept (event bus is retained — decision 2026-08-09).
- **Avatar fallback**: initials in accent color (v1 kept, both header and list variants).
- **PostCard**: shared with the main feed (out of scope here) — profile feed cards must render source badges (4.3).

---

## 5. States

### 5.1 Loading

| Surface | v1 | v2 |
|---|---|---|
| Whole screen | Bare "Loading..." text | Column skeletons: header block shimmer, 3 post-card shimmers, interests tag shimmers; nav renders immediately (static defaults) |
| Feed page fetch | Cards stay, "Loading..." only when empty | Spinner row appended; existing cards stay |
| Follow action | Button text `...` | Same (kept) |

### 5.2 Error

| Case | Behavior |
|---|---|
| Profile fetch fails (network/5xx) | Error panel with retry button (v1 showed the raw message — keep the message, add retry) |
| User not found (404) | "Profile not found" + hint: check the handle, search link. v1 text kept, add navigation |
| Save failed (layout/identity) | Inline error near toolbar/modal, edits retained (v1 kept `setError` — surface it as a toast/banner, don't drop user work) |
| Follow fails | Rollback optimistic state + toast (v1 set a screen-level error — too heavy; refine to toast) |
| Bluesky-hosted identity write fails (XRPC) | Specific message: "Reconnect your Bluesky account to make changes" with re-auth action (AD-01b edge). Widget/layout saves are app-DB writes and are unaffected |

### 5.3 Empty

| Case | Copy / behavior |
|---|---|
| No posts (own) | v1 kept: "No posts yet. Click [+] to make your first post." |
| No posts (visitor) | "No posts yet." (no [+] hint) |
| No interests (own) | v1 kept: "No interests yet. Click ⚙ to add some." |
| No interests (visitor) | v1 kept: "No interests selected." |
| No widgets saved yet (either user class — empty `widget_layout`) | Render DEFAULT_LAYOUT; no "empty grid" state ever shown to visitors |
| Empty layout (owner deleted everything) | Grid shows a dashed "Add a widget" drop hint + toolbar palette; visitors of such a profile see the empty area as-is (their layout, their choice) |
| Followers/Following page with nobody in it | Full-size widget shows its empty state ("No followers yet" / "Not following anyone yet") |
| No avatar / no banner / no bio | Initials fallback / `--surface-secondary` block / omit (never "No bio") — redesign spec edge cases kept |
| Visitor counter with no comments yet | Count + 📝 button render normally; recent-comments area omitted (no "no comments" noise) |
| Feed filter returns nothing | "Nothing from {source} yet." + clear-filter action |

### 5.4 Viewer-role matrix

| Affordance | Owner | Auth visitor | Anon visitor |
|---|---|---|---|
| View profile/feed/interests | ✅ | ✅ | ✅ (FR-9 public viewing) |
| Follow button | hidden | ✅ | ✅ → login redirect |
| Wall comment / visit comment (6.4, 6.8) | hidden (self-visits still count toward the number — it's a page-load counter) | ✅ leave comment (login required) | ✅ view count; comment → login redirect |
| Edit Profile / toolbar | ✅ | hidden | hidden |
| `[+]` Post/Import | ✅ | hidden | hidden |
| Interests gear | ✅ | hidden | hidden |
| Add/remove nav pages | ✅ | hidden | hidden |

### 5.5 Edge cases

- **Private profiles — planned, slot-in ready (decision 2026-08-09):** AT Protocol does not support private profiles yet; Bluesky's private-posts spec is incomplete. v2 is designed so a private mode slots in when the protocol supports it:
  - The profile API response already carries everything the frontend needs to branch on a future `visibility` field — no redesign required, just a new value.
  - **Locked variant (spec'd now, built when needed):** avatar + name + handle + "This profile is limited" notice. No feed, no widgets, no interests, no stats. Follow button remains (follow requests are a natural companion to privacy).
  - The wall (FR-24) is launch scope and not gated on private posts — its visibility model (visible only on the recipient's profile) is enforced by the app. When private posts/profiles land, the wall and private mode become one feature family.
- **Suspended/deleted user** (`users.status = 'deleted'`): tombstone card — "This account is gone" with the handle, no widgets.
- **Very long bios / display names**: clamp bio at ~6 lines with expand; names wrap (v1 unstyled overflow — refine).
- **Custom CSS** (allowed — decision 2026-08-09): no-code customization (themes, per-widget editor, visual layout editing) comes first and foremost. The editor ships in v2.0 (option-B re-draw) — sanitized/allowlisted subset only, and it never applies to edit-mode chrome so users can't hide their own controls.
- **Owner with tour not complete**: OnboardingTour mounts (own + no handle param), anchors in 5.6.

### 5.6 Onboarding tour anchors (contract preserved)

The v1 contextual tour targets real modules; v2 keeps the anchor contract so the tour survives the rebuild:

| `data-tour` | Element |
|---|---|
| `interests` | InterestWidget container |
| `import-button` | Feed `[+]` button |
| `post-option` / `import-option` | Dropdown items |
| `edit-profile` | Header edit button / toolbar edit button |

Tour steps (interests → post → import → customize) and their dialogue flow are unchanged from the redesign spec; the tour keeps driving the UI through the retained custom-event bus (`bytescape:tourAction`, `bytescape:tourAdvance`, …) — stack decision 2026-08-09.

---

## 6. Interactions

### 6.1 Follow / unfollow
1. Visitor clicks `Follow` → optimistic: button flips to `Following`, count +1.
2. `POST /profile/{handle}/follow` (or unfollow). Success: nothing further. Failure: rollback + toast.
3. `Following` hover shows `Unfollow` affordance (v1 showed static "Following" — refine).
4. Unauthenticated: click routes to login, returns here after.

### 6.2 Edit identity (quick path)
1. `✏️ Edit` in header → IdentityEditModal (4.5) prefilled.
2. Save → profile update API → header re-renders; interests sync to InterestWidget + subscription bootstrap (v1 behavior kept).
3. Avatar/header upload: file picker → upload endpoint → preview immediately in modal (v1 uploaded on select; keep, with progress).

### 6.3 Customize layout (the MySpace path)
1. `✏️ Edit Profile` (toolbar) → edit mode: grid becomes draggable/resizable, widget controls appear, toolbar switches to edit actions.
2. **Add**: `+ Add Widget` palette (16 shipping types) → widget appended at bottom (`y = max`, v1 kept) → user drags it home.
3. **Move/resize**: drag/resize with collision prevention; layout state held locally, `dirty=true`.
4. **Theme a widget**: `⚙` → WidgetThemeEditor → save attaches `theme_id` (new or updated theme in `widget_themes`).
5. **Configure header sections**: `📝` on header widget → sections modal (toggle built-in fields, add/remove custom sections — v1 `custom_profile_fields` CRUD kept, UI moved from floating portal to modal).
6. **Delete**: `×` with inline confirm for content-bearing widgets (gallery, links). Deleting the `visitor_counter` widget does NOT reset the count (server-side, 4.6).
7. **Preview**: toggles editing off visually while staying in edit mode (v1 kept).
8. **Save**: `PUT /api/profile/me` with `widget_layout` + `widget_themes` — the v1 path, kept verbatim (app-DB storage, decision 2026-08-09). Success exits edit mode and fires `bytescape:profileUpdated`. **Reset**: reverts to last saved. **Cancel**: confirm if dirty, revert.
9. Identical for both user classes — widget storage is an app-DB write; only identity fields follow the PDS/XRPC split (AD-01b), and only when identity is edited (6.2).

### 6.4 Wall (launch scope — option-B re-draw)

FR-24 is in v2.0. Wall posts are app-DB records; the wall is visible only on the recipient's profile, never in the main feed.

1. Visitor (authenticated, not the owner) sees the wall widget below the feed: "📖 Sign My Guestbook!" header, zebra-striped entries, and a "Submit Entry" affordance (v1 skin kept).
2. Posting opens the same simplified composer popup as visitor comments (6.8) — text/media, login required. Anonymous: routes to login, returns after.
3. Owner sees ✕ per entry to delete (moderation is part of v2.0, not deferred). Deletion never affects the visitor counter.
4. A new wall post notifies the owner via the `notifications` table (`profile_comment` type, FR-25) — same as visitor comments. Clicking routes to the poster's profile.
5. **Promote-to-feed**: the owner can promote a wall post to a regular `app.bsky.feed.post` (FR-24 consequence) — a "share" action on the entry.
6. The visitor_counter widget's comments (6.8) are the wall's seed data — same storage family, same moderation rules.

### 6.5 Profile feed filtering (FR-11)
1. Default: All sources, All types, chronological (imported + live interleaved).
2. Source filter (Bluesky / Facebook / Instagram / …) → refetch with `source=` param (v1 API already supports).
3. Type filter (image / video / event / text) → `content_type` param.
4. Filters compose additively; active filters shown as clearable chips; empty result state per 5.3.

### 6.6 Navigation (nav widget page switching)
1. Nav item click swaps the page: **Profile** → the widget grid (3.1); **Followers** → full-size `friends` widget; **Following** → full-size `following` widget (3.4). The nav widget stays visible on every page.
2. `+ Add page` (own only) → inline form → creates nav item, saved with the nav widget's data in `widget_layout` (app DB); custom pages render their page shell (Blog: post list + composer entry; Gallery: media grid; Custom Link: external redirect — launch scope per option-B re-draw).
3. Custom item hover → `✕` remove (default items protected — v1 kept).
4. Owner deletes the nav widget entirely → page switching disappears with it; the screen defaults to the Profile grid. Restorable from the palette.

### 6.7 Interest selection (categories + manual entry)
1. Gear ⚙ (own profile) → picker modal opens with the static category list (v1 `INTEREST_CATEGORIES`).
2. Tapping an interest toggles it (chip appears/disappears in "Selected", max 20).
3. Manual entry: type any interest → `Add` → stored alongside presets.
4. Save → `PUT /api/profile/me` with the interests array → subscription bootstrap (v1 kept) → widget updates, `bytescape:interestsUpdated` fires. Revert on error (v1 kept).

### 6.8 Visitor counter + wall comments (old-school, decoupled)

The counter is a **page-load counter**, not analytics and not a registration system (round-3 decision). Its lineage is the GeoCities hit counter — a number that increments every time the page loads, owner included, and that nobody reads as a privacy signal because it learns nothing about anyone. First-party counting is not the third-party tracker (Site Meter, digits.com cookies) that killed the old counters — the server already sees the profile request, and the counter adds no cookies, no analytics, no dedup, no per-visitor state.

```
┌─ visitor_counter widget ──────────────┐
│        👁 You are visitor № 001,043   │
│                                       │
│  [ 📝 Leave a comment ]               │
│                                       │
│  Recent:                              │
│  @mika — "love the theme!"       [✕] │
│  @jon — "back from 1999"         [✕] │
└───────────────────────────────────────┘
```

1. **Counting is automatic.** Each profile page load increments `visitor_count` server-side (app DB). No button, no dedup, no optimistic state, no login requirement — the count is a byproduct of the page existing, exactly like the old `<img>` tag counters. The odometer digits are decorative; the count renders with a text alternative for screen readers (R-16).
2. **Owner visits count.** It is a "times this page has been loaded" number, not a unique-visitors metric — the owner's own reloads are part of it (that was always part of the joke, and it stays).
3. **Comments open a popup.** Below the digits: a **📝 Leave a comment** button. It opens the **comment popup** (round-3 decision) — deliberately the *same popup spec as the post composer* (`ComposerModal`): same modal shell, text area, media upload, and submit flow. The only deltas: no mood/status prefix, no tagging, no location check-in, no visibility select, no cross-post toggles, no char-counter emphasis — just text + media in a popup, submitted as a comment. The popup is what it looks like; the backend difference (comment record vs. post record) is invisible to the user. Comment stores the author's handle; it appears in the widget's recent list.
4. **Login required to comment** (round-3 decision). Anonymous visitors see the 📝 button; clicking routes to login and returns to the comment popup after. Identity gives comments their voice (`@mika — "love the theme!"`), makes owner notifications meaningful, and is the primary spam deterrent.
5. **Owner deletes comments.** The owner sees ✕ per comment; deleting removes it from the widget immediately. Moderation is part of the v2.0 feature (round-3 decision), not deferred.
6. **Owner notification.** A new comment creates a `profile_comment` notification in the existing `notifications` table (FR-25), surfaced by the bell — no new notification infrastructure. Clicking routes to the commenter's profile. **Plain visits never notify.**
7. **Persistence.** The count lives server-side whether or not the widget is placed. No widget → no comment form and no visible count, but the number keeps accumulating (a page load still hits the server); re-adding the widget restores the true count.
8. **This is not the wall.** Flat comments, no threading, no promote. The full wall (FR-24) is launch scope (6.4) — these comments are its seed data, and its moderation rules inherit this section's precedent (owner-deletable, login-required).

---

## 7. Responsive Behavior

| Breakpoint | Layout | Nav | Grid |
|---|---|---|---|
| Desktop >1200px | 3 columns 25/50/25 | Left sidebar widget | 20 cols, free placement |
| Tablet 900–1200px | Nav pills on top; content 2 cols (center ~60% + right ~40%) | Horizontal pill bar (v1 tablet branch kept) | Widgets reflow to 2-col effective |
| Mobile ≤900px | Single column stack | Hamburger dropdown (v1 kept) | 1 col, widgets stack in mobile order (independent of grid position — decision 2026-08-09; PRD threshold, v1 used 768 — changed) |

Additional rules:
- Header widget: banner height shrinks (120px → 88px), avatar 80px, chips wrap; stats row stays horizontal (fits).
- Toolbar in edit mode collapses to icon buttons below 550px (v1 has a 550px query — reuse).
- Modals (theme editor, identity, composer, import) render full-screen sheets below 900px.
- Mobile edit mode: no grid drag — widgets move up/down in the mobile stack (order only, independent of grid position — decision 2026-08-09). Resize via per-widget steppers if needed.
- **Accordion option (from MobileProfileRender, REWORK)**: below 900px, widgets may group into collapsible sections — About Me (header/about/stats), My Posts (feed/gallery), Connections (friends/following), More (music/links/status). Default: About Me + My Posts open. Group map from v1 kept; this is a refinement, not a requirement for v2.0 — the plain stack is acceptable first. Accordion grouping is compatible with the mobile-order stack (groups are a presentation layer over the same order).

---

## 8. Refinement List (concrete deltas over v1)

**ProfileScreen**
- R-1: De-duplicate, keep the stack. Collapse 15 useState hooks into grouped state (or one reducer) and replace the three near-identical desktop/tablet/mobile render branches with one responsive tree. The existing patterns — useState/useEffect, localStorage caching, custom DOM event bus — are KEPT per the 2026-08-09 decision; the event bus is not replaced.
- R-2: Render FollowButton on desktop too (header placement).
- R-3: Nav items switch real pages: Profile grid, Followers/Following as full-size widgets (3.4), custom page shells (Blog/Gallery/Custom Link — launch scope) — instead of decorative highlighting.

**Header**
- R-4: ProfileHeaderBar gains follow button, detail-chip row (driven by `custom_profile_fields`), account-type badge (bytescape vs bluesky), theme variable hooks; stats become clickable (followers/following → sub-views).
- R-5: WidgetProfileHeader split: `HeaderBanner` / `HeaderIdentity` / `HeaderFields` + sections modal; keep the configurable-fields system intact (built-in visibility toggles + custom sections with label/content).

**Feed**
- R-6: Filter bar (source + type) per FR-11; source badges on imported cards; cursor-based pagination fix.

**Wall**
- R-7: Wall is launch scope (option-B re-draw). Rebuild the v1 guestbook skin ("📖 Sign My Guestbook!", zebra entries) as the wall widget: app-DB storage, login-required posting, owner-delete, owner notification, promote-to-feed. The fake visitor counter it carried is replaced by the real `visitor_counter` widget (R-15).

**Widgets / themes**
- R-8: Register About, AvatarName, Banner, Stats, Wall + new `visitor_counter` in registry + palette (16 shipping types).
- R-9: WidgetThemeEditor: live preview, apply-to-all, preset swatches.
- R-10: Make preset themes real — retarget `.theme-terminal` / `.theme-1999` / `.theme-xp` CSS from the dead `.profile-layout` container onto the grid root via CSS custom properties so every widget inherits the theme; theme selector lives in IdentityEditModal/toolbar. Theme applies **app-wide by default** via `users.theme_app_wide` (default true — round-3 decision); the "Apply theme across the app" checkbox in the identity modal opts out. When app-wide, the theme's CSS custom properties mount on the app root, not just the profile grid.
- R-11: Nav items persist in the nav widget's saved data (`users.widget_layout`, app DB) — kills the `bytescape_nav_items` localStorage-only storage. `bytescape_interests` stays as a cache/sync aid (stack decision keeps localStorage patterns), with the server's `users.interests` as source of truth.
- R-12: PhotoGallery uploads to media storage (no base64 in `widget_layout`); MusicPlayer embed-type fix (track vs album vs playlist).
- R-13: Consolidate InterestWidget + WidgetInterests into one component; picker returns selection via callback; picker is the simplified static-categories + manual-entry design (4.4).
- R-14: Mobile breakpoint aligned to 900px; hamburger nav kept; accordion grouping as follow-up.
- R-15: New `visitor_counter` widget — **old-school page-load counter** (round-3 decision): automatic count, owner included, odometer digits, no Register button; decoupled 📝 comment button opening the simplified composer popup (text/media, login required), owner-deletable comments, `profile_comment` notification via the existing bell. Retro hit-counter styling; count persisted server-side whether or not the widget is placed.
- R-16: Accessibility floor (v2.0, decision 2026-08-09 — low-hanging items pulled in; heavy items post-launch). **In v2.0:** aria-labels on all icon-only buttons (⚙ ✕ 📝 [+]); focus trap + focus return in modals (composer, identity, theme editor, comment popup); Escape closes modals and dropdowns; form labels on all inputs; aria-live for optimistic updates (follow flip, count changes) and import progress; visitor counter text alternative (odometer digits decorative, count readable); color-not-only-signal on badges/chips/status (icon, text, or shape alongside color); `prefers-reduced-motion` respected. **Post-launch:** full keyboard path for the grid (arrow-key move, keyboard resize or steppers), WCAG AA contrast audit for chrome, ≥44px touch targets, 200% zoom without breakage, color-blind mode (platform-level palette remap — chrome unconditional, user themes get a best-effort "render in safe palette" option).

**Aesthetic guardrails (keep, do not flatten)**
- Terminal palette (`#00ff41` accent, `--font-mono` Share Tech Mono, `--font-retro` VT323), bordered boxes, emoji iconography, Comic Sans in the font picker, retro hit-counter styling for `visitor_counter`. Guestbook copy and zebra entries return with the wall in v2.0 (launch scope). GeoCities 1999 (magenta) and XP Aero (blue) themes ship as real themes this time (R-10).

---

## 9. Open Questions

**None outstanding.** All 13 original OQs and the 3 follow-up NQs were resolved on 2026-08-09 (two decision rounds, log below).

### Resolved decisions (2026-08-09)

| OQ | Resolution |
|---|---|
| OQ-1…4 (Wall) | **LAUNCH SCOPE (option-B re-draw).** The earlier deferral (tied to private posts/profiles and the Bluesky private-posts spec) is superseded: wall posts are app-DB records, the visibility model is app-enforced, and private profiles remain a separate planned slot-in (5.5). v1 guestbook is the starting skin (4.3, 6.4) |
| OQ-5 (Private profiles) | **Plan for it.** Defensive locked state kept (5.5); design slots private mode in when AT Protocol supports it |
| OQ-6, OQ-8, OQ-13 (Widget lexicons) | **No widget lexicons.** Widgets are not PDS records. Layout/config live in the app DB (`users.widget_layout`, `users.widget_themes`, `users.theme`, `users.custom_css`); identity stays `app.bsky.actor.profile`. One-record-per-widget is dead |
| OQ-7 (Custom CSS) | **Allowed, ships in v2.0 (option-B re-draw).** No-code first (themes, widget customization, visual editor); the custom CSS editor ships in v2.0 — sanitized/allowlisted subset, never applies to edit-mode chrome (5.5) |
| OQ-9 (Followers/Following sub-views) | **They are widgets.** Nav is also a widget (left by default, movable). Nav pages: Profile (the grid), Followers and Following (each a single full-size widget, nav bar excluded). See 3.4 |
| OQ-10 (Visitor counter) | **Yes — as a widget.** Count stored server-side whether or not the widget is placed (removal/re-add never loses it). Not high priority, doable, included. v1's hardcoded "001,247" noted as the fake it always was. (Counting mechanism refined by NQ-2) |
| OQ-11 (Interests) | Preset interests + manual entry, both stored in `users.interests`. (Picker mechanism refined by NQ-1) |
| OQ-12 (Frontend stack) | **Keep the existing stack.** React 19 + Vite + TypeScript with existing patterns: useState/useEffect, localStorage caching, custom DOM event bus. No new state library. ProfileScreen rework targets duplication only (R-1) |
| NQ-1 (Interest suggestions) | **SIMPLIFY.** The suggestion tree was an idea, not a requirement. Final design: static category-based interest list (v1 `INTEREST_CATEGORIES`) + manual entry — "enough on its own to do what we need for now." No suggestion engine, no new API (4.4, 6.7) |
| NQ-2 (Visitor counter semantics) | **OLD-SCHOOL HIT COUNTER (round 3).** NQ-2's active-visit design was over-engineered on a misapplied privacy analogy — passive *counting* ≠ passive *tracking*; what killed the old counters was third-party widget services, not first-party page-load counts. v2: automatic count on every profile load (owner included), odometer display, no button/dedup/states. Comments decouple into a separate login-required form; owner-deletable; owner notified via the existing bell (4.6, 6.8) |
| NQ-3 (Followers/Following pagination) | **Not a design question.** Backend implementation note: full-size pages need paginated endpoints. Recorded on the `friends`/`following` catalog rows (4.6), tracked separately |

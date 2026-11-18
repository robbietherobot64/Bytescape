# Screen Spec: Feed (v2)

> **Status note (2026-10-10):** the mutuals-first / category-sidebar thesis in this spec
> is **superseded in direction** by `DECISIONS.md` (blend-by-default Home, source chips,
> ephemeral tabs, saved lenses, stars) and the open change
> `openspec/changes/feed-refocus/`. Its data-layer tables largely don't exist yet —
> treat as target-state design.

**Status:** Draft — complete, pending user decisions on 8 open questions (section 9; OQ-2 Subscriptions model is the gating one)
**Date:** 2026-08-09
**Route:** `/` (Feed is the pinned default tab — the only tab that cannot be closed in the TabBar)
**Satisfies:** FR-10 (Feed with Per-Source Control) — primary surface; FR-13 (Post Composer) — composer entry point + modal; FR-15 (Reactions and Engagement) — all engagement lives on feed post cards; FR-14 (Cross-Posting to Bluesky) — composer toggle + per-platform result badges on cards. Contributes to FR-25 (notification bell placement), FR-26 (mute from post card), FR-19 (chat in the social panel), FR-28 (AppView query surface), SM-4 (feed loads ≤2s).

> **FR numbering note:** the sprint brief referenced "FR-12 (Cross-Posting to Bluesky)". In the current PRD, FR-12 is the **Media Screen** and cross-posting is **FR-14**. This spec uses the PRD numbering.

**Sources:**
- v1 implementation: `apps/web/src/screens/FeedScreen.tsx` (33 lines), `src/screens/FollowingScreen.tsx` (183), `src/components/FeedLayout.tsx` (755), `FeedColumn.tsx` (468), `PostCard.tsx` (1276), `CategorySidebar.tsx` (303), `SocialPanel.tsx` (203), `MobileNav.tsx` (49), `TabBar.tsx` (77), `ComposerModal.tsx` (190), `NotificationBell.tsx` (199), `src/styles.css` (6,098 lines — feed sections)
- v1 UX design session: `_ref/artifacts/_bmad-output/ux-designs/ux-bytescape-feed-2026-07-15/` — `EXPERIENCE.md` (IA, component/state patterns, accessibility floor, 3 flows), `DESIGN.md` (tokens, component specs, do's/don'ts), `.decision-log.md` (D1–D12)
- v1 evolution: `_ref/artifacts/_bmad-output/evolution/scenarios/sc-mobile-sort-2026-07-22.md` (mobile toolbar + bottom-nav redesign — implemented in v1, carried forward)
- v2 constraints: `wiki/product/prd.md` (FR-10, FR-13, FR-14, FR-15; FR-16 privacy deferred), `wiki/architecture/database.md` (appview_* tables), `wiki/architecture/decisions.md` (PDS-first writes, derived Postgres index, read-after-write, dual PDS)
- Format template: `wiki/design/screens/profile.md`

---

## 1. Overview

The feed is Bytescape's primary surface and its thesis statement: **mutuals-first, reverse-chronological, no algorithm, per-source control**. The user sees everything from their sources in one timeline, newest first, with genuine connections (mutual follows — "Friends") loaded and prioritized before feeds and hashtags. There is no engagement ranking by default and no opaque curation anywhere: every post in the feed is traceable to a source the user chose, and every source is selectable, filterable, searchable, and deletable from the left panel. FR-10 is validated by SM-4 (≤2s load), so the loading architecture is a first-class design concern, not an afterthought.

The screen is a desktop 3-panel workspace — source sidebar (control center) / feed column (timeline) / social panel (friends + chat) — collapsing to a single column with a slide-out drawer and bottom nav on mobile (v1 pattern, kept; D8).

### 1.1 What the feed shows — and never shows

| Content | In the feed? | Why |
|---|---|---|
| `app.bsky.feed.post` from followed accounts, feeds, hashtags, subscriptions | ✅ | The feed's body (FR-10) |
| User's own new posts | ✅ immediately | Read-after-write: the API handler does a direct index insert after the PDS write, so the composer closes onto the post already in the feed — no polling, no "your post will appear shortly" |
| Imported archive posts (`com.bytescape.archive.post`) | ❌ never | D7, now enforced by architecture: archive posts are profile-only and are not delivered to followers (FR-11). v1's feed carried import-source filter chips (facebook/instagram/threads/google) — they are removed in v2 (see 2.2) |
| Posts from muted authors | ❌ | FR-26; mute from the post-card menu removes the author's posts (v1 behavior kept) |
| Reposts | ✅ attributed | `app.bsky.feed.repost` surfaces with a "🔁 Reposted by …" banner (v1 kept) |

### 1.2 Data sources (v2)

Postgres is a **derived index** of the firehose; the feed reads only from the local AppView — never from `public.api.bsky.app` (D6/D12, confirmed by v2 architecture).

| Data | Table / record | Notes |
|---|---|---|
| Post body, author, embeds, counts | `appview_posts` | Denormalized: `uri`, `cid`, `author_did`, `author_handle`, `text`, `created_at`, `embeds` JSONB, `reply_parent/root`, `like_count`, `repost_count`, `reply_count`, `labels` |
| Like state + counts | `appview_likes` | `UNIQUE(actor_did, subject_uri)` — one like per user per post; the viewer's own like is a row lookup |
| Following graph + mutuals | `appview_follows` | `UNIQUE(actor_did, subject_did)`. **Mutual** = both (viewer→author) and (author→viewer) rows exist. This join defines the mutuals-first seed phase (3.x) and the Friends list in the social panel |
| Author identity / avatars | `appview_profiles` | `handle`, `display_name`, `avatar_url` — drives post headers, sidebar items, search results |
| Feed sources (the user's selections) | App DB (v1 `subscriptions` + follows, carried forward) | Following = Bluesky follows; Feeds = custom feed generators; Hashtags; Subscriptions. RSS is **not** in the v2 FR-10 source list (FR-8 is Phase 2) — see 2.2/D11 |

### 1.3 Dual PDS note (AD-01b)

Both user classes read the **same** feed: content is indexed from the relay firehose regardless of where it was written.

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Reading the feed | Identical — AppView queries only | Identical |
| Posting from the composer | Signed commit to Bytescape PDS + direct index insert (read-after-write) | XRPC write to bsky.social; indexed when the relay event arrives (near-instant, no client difference) |
| Follows that power "Following" / mutuals | `app.bsky.graph.follow` records on their PDS → `appview_follows` | Same |

Frontend consequence: the feed UI is PDS-agnostic. The only place the split surfaces is composer latency expectation — and read-after-write makes even that invisible for hosted users.

### 1.4 Frontend stack (kept)

React 19 + Vite + TypeScript with the v1 patterns retained per the 2026-08-09 stack decision: `useState`/`useEffect`, localStorage caching (`bytescape_feed_sort`, `bytescape_interests`), and the custom DOM event bus (`bytescape:refreshFeed`, `bytescape:openComposer`, `bytescape:openSidebar`, `bytescape:toast`, `bytescape:viewProfile`, `bytescape:loadMore`, `bytescape:accountLinked`, `bytescape:profileUpdated`). No new state library. The feed rework targets duplication and defects, not the state approach.

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is or with trivial touch-ups), **REFINE** (keep concept and most code, improve), **REWORK** (keep concept, rebuild internals), **DROP** (obsolete; mine for parts), **DEFER** (concept preserved, design work postponed to Phase 2).

Summary: **3 KEEP · 6 REFINE · 1 REWORK · 2 DROP · 1 DEFER** — consistent with the user's verdict that the v1 feed is good and gets salvaged with refinement. The one REWORK (FeedLayout) keeps every behavior but rebuilds the internals; nothing on this screen is torn out wholesale.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `FeedLayout.tsx` (755) | **REWORK** | All the right behaviors — two-phase seed/full loading (the SM-4 mechanism and the natural mutuals-first vehicle), cursor pagination with dedup + prefetch, category/filter/search orchestration, event-bus wiring — buried under 15+ useState hooks, console.log debugging spam, and two parallel platform-filter pipelines (server-side `source_categories` vs client-side `selectedPlatforms`) that must merge into one |
| 2 | `FeedColumn.tsx` (468) | **REFINE** | Toolbar, loading/error/empty states, inline interest picker, active-filter chips all kept. Fixes: any click on a filter chip clears **all** filters instead of removing that chip (line 428: `onClick={() => onClearFilters?.()}`); duplicate sort controls (`<select>` **and** toggle buttons); import-source filter chips (D7 violation — DROP'd in 2.2); text-only loading states become skeletons per EXPERIENCE.md |
| 3 | `PostCard.tsx` (1276) | **REFINE** | The richest salvage: unified media/embed pipeline (HLS video, image carousel + lightbox with swipe, YouTube/SoundCloud/Bandcamp embeds, OG link previews, quote-post embeds), emoji reaction picker with toggle-off, share/quote dialog, inline comments, follow-on-card, mute menu — all kept. Split into ~4 files (card / media+embeds / quote embed / dialogs); move the ~300 lines of inline styles to CSS; replace `window.confirm`/`alert()` delete with the styled DeleteConfirmationPopup pattern; fix avatar fallback (`handle[1]` assumes a leading `@`) |
| 4 | `CategorySidebar.tsx` (303) | **REFINE** | Becomes the UX session's **SourceSidebar**: global search with mode chips + smart `#` detection (this is D9 "future search" shipped early — kept), per-category local filter inputs, category-level include checkboxes, chip items with ✕, delete confirmation with Cancel auto-focus (matches D4). Fixes: popup dismisses on overlay click (spec: not dismissable — Escape/Cancel only); ✕ hit target sits inside the selection chip (DESIGN.md don't: separate the click targets); drawer behavior at ≤900px kept verbatim (85vw / max 320px / 200ms ease-out) |
| 5 | `SocialPanel.tsx` (203) | **REFINE** | FRIENDS/CHAT tabs, mutuals + follows merge with dedup, click-friend → open-or-start DM, chat expand — kept. **DROP the fake presence dots** (`statusIndicator()` cycles hardcoded `online/idle` — same family as the profile's fake visitor counter); avatars come from `appview_profiles` instead of rendering dot placeholders for everyone |
| 6 | `MobileNav.tsx` (49) | **KEEP** | Exactly the sc-mobile-sort desired state: `Feed · Profile · ＋ · Chat · More` with the centered accent composer button dispatching `bytescape:openComposer`. Untouched |
| 7 | `TabBar.tsx` (77) | **KEEP** | App chrome rather than feed internals, but the feed contract is here: Feed is the pinned, unclosable tab and `.tab-bar-left` stays exactly 280px to align with the sidebar (EXPERIENCE.md key constraint). NotificationBell lives in the bar. Note: styles.css applies `width: 280px !important` to `.tab-bar-left` *inside* the ≤900px query — a v1 layout bug removed in the responsive rebuild (section 7) |
| 8 | `ComposerModal.tsx` (190) | **REFINE** | Modal shell, media upload-then-post flow, platform toggles, char counter, error surfacing — kept and upgraded to FR-13 (mentions, mood/status prefix, location check-in, link attachments, grapheme-accurate counting: v1 counts UTF-16 `length`, AT Proto counts 300 **grapheme clusters**). Two removals: the Mastodon cross-post toggle (v2 cross-posting is Bluesky-only per FR-14) and the visibility select (public/followers/friends) — unenforceable until FR-16 privacy modes, Phase 2 (see 2.2) |
| 9 | `NotificationBell.tsx` (199) | **REFINE** | Unread badge (9+ cap), dropdown with per-item + mark-all read, 30s polling — works. Move ~115 lines of inline styles into CSS classes; pause polling when the tab is hidden; notification click should route (feed-adjacent: a reaction/follow notification lands on the relevant post/profile) |
| 10 | `FollowingScreen.tsx` (183) | **DEFER** *(partial KEEP)* | The bulk follow-management list (source tabs, unfollow, unsubscribe) is the feed sidebar's companion, but its v1 shape is built around the five-platform model (Bluesky/Mastodon/Lemmy/RSS tabs). Its salvage is gated on the v2 Subscriptions decision (OQ-2 in section 9); the sidebar remains the primary source-management surface in v2.0, and this screen follows in the same phase as the source-model resolution |

**Feature-level DROPs (inside surviving components):**
- **Import-source filter chips** in FeedColumn (`facebook/instagram/threads/google` options) — imported posts never appear in followers' feeds (D7 + v2 architecture). DROP.
- **Fake presence indicators** in SocialPanel. DROP.
- **`chronological` sort migration shim** in FeedLayout (v1 silently rewrote stored `chronological` → `popular`) — obsolete once the default is fixed; see 2.3 defect #1.

### 2.1 v1 UX design tokens — what carries to v2

The UX session's DESIGN.md and the shipped `styles.css` disagree on color, and the shipped code wins. DESIGN.md specced a blue/cyan-on-slate theme (`#1a1a2e` bg, `#4c9ed9` chips/selection, GitHub-grays `#c9d1d9`/`#8b949e`/`#21262d`); the v1 that actually shipped — and that the profile spec's aesthetic guardrails already ratified — is the **green-phosphor terminal**: `--bg-primary: #0a0a0a`, `--bg-secondary: #111`, `--accent/--text-primary/--border-color: #00ff41`, `--accent-dim: #0a3a0a` borders, `--danger: #ff3333`. The blue tokens were draft-stage and are dated relative to the product's own identity; they do not carry.

| Token group | Verdict | v2 treatment |
|---|---|---|
| **Fonts** — VT323 (display/headers), Share Tech Mono (body/UI), Press Start 2P (rare accents) | **Carry** | Untouched. The mono-everywhere density is the product's voice; feed headers (`FEED`, category names) stay VT323 uppercase, sidebar items and chips stay Share Tech Mono 11–12px |
| **Theme** — dark-only, green phosphor, `>` prompt prefixes on buttons/placeholders | **Carry** | Dark-only stays (DESIGN.md don't: no light theme). Prompt prefixes stay on CTAs (`> LOAD MORE POSTS`, `> RETRY`, `> FOLLOW N INTERESTS`) — retro without becoming a costume |
| **Functional colors** — platform badge hues (Bluesky `#0085ff`, Mastodon `#6364ff`, RSS `#ff8800`, Lemmy `#00bc7d`, Bytescape `#3fb950`), DELETE red | **Carry, re-valued** | Badge hues are semantic, not decorative — kept on post badges and search results. DELETE red harmonizes to `--danger` (`#ff3333`) instead of DESIGN.md's `#da3633`. Contrast: several badge hues fail 4.5:1 at 11px on `#0a0a0a` — badges render at ≥12px/600 weight or shift to border+icon treatment (accessibility floor from EXPERIENCE.md applies) |
| **Interaction tokens** — chip active/inactive, selection highlight, search highlight | **Carry the slots, re-value** | DESIGN.md's `rgba(76,158,217,…)` values re-mapped into the green system: selected chip = `rgba(0,255,65,0.12)` bg + `--accent` border; sidebar selection = 2px left border `--accent` + accent text; hover = `rgba(0,255,65,0.05)` |
| **Spacing & layout** — 280px left panel, feed content max 600px, category padding 8/12px, source item 6/12px, 28px search bar, chip bar 8px | **Carry** | Shipped CSS confirms: grid `280px 1fr 300px`, drawer 85vw/max 320px. These numbers are proven at real usage density — no change |
| **Shapes & elevation** — 4px inputs, 12px pill chips, 6px delete, 8px popup; flat panels with 1px borders, shadow only on drawer/popup | **Carry** | The flat-with-one-shadow-layer system reads correctly as "terminal, not Material" — kept wholesale |
| **Motion** — 200ms drawer ease-out, 200ms accordion max-height, `--transition: 0.15s` micro-states | **Carry** | Fast, mechanical transitions suit the aesthetic. One addition: skeleton shimmer for first feed paint (EXPERIENCE.md state pattern; v1 shipped text-only loading) |

### 2.2 D1–D12 validity in v2

| Decision | v2 status |
|---|---|
| **D1** reverse-chrono combined feed, "All" default | **Valid — and restored.** v1 code drifted: default sort shipped as `popular` (localStorage init, FeedLayout line 33). v2 re-asserts D1: default is reverse-chronological ("New"), mutuals-first within it. The Popular toggle survives only as an explicit opt-in sort (see OQ-1) |
| **D2** left panel as control center (5 categories) | **Valid, minus RSS.** Four categories at v2.0 launch — Following, Feeds, Hashtags, Subscriptions (FR-10's list). RSS was the fifth; FR-8 (RSS subscriptions) is Phase 2, so the RSS section is deferred, not deleted — the sidebar reserves the slot |
| **D3** additive multi-select filtering | **Valid, unchanged.** v1 implements it correctly except the chip-removal bug (2.3 #2) |
| **D4** delete with confirmation (big red DELETE, Cancel default focus) | **Valid, unchanged.** Shipped popup matches, including autoFocus on Cancel; overlay-dismiss gets removed to match the "forces explicit choice" wording |
| **D5** per-category search bars, context-appropriate | **Superseded by something better.** v1 shipped a single global search with mode chips (All/People/Hashtags/Feeds/Communities) + per-category *local filter* inputs. That's D5's functionality with less chrome; v2 keeps the shipped evolution |
| **D6/D12** own AppView off the relay firehose, no `public.api.bsky.app` dependency | **Validated and strengthened.** This is now the v2 architecture itself (`appview_posts` etc.). Quote-post and OG lookups that v1 proxied through Bluesky CDNs move onto the local index (2.3 #8) |
| **D7** imported posts profile-only | **Valid, now architecture-enforced.** `com.bytescape.archive.post` is not delivered to followers; feed UI drops the import-source chips |
| **D8** desktop sidebar + mobile drawer | **Valid, shipped, kept** (drawer geometry verified in styles.css) |
| **D9** universal search as future footnote | **Half-shipped.** Sidebar global search is live; top-center universal search remains future (out of this spec) |
| **D10a** no external account linking required for feed content | **Valid, stronger than ever.** With FR-30 native signups + the local AppView, a user with zero linked accounts gets full search and a full feed |
| **D10b** "link Bluesky for better search" upsell | **Obsolete.** Local AppView gives every user full `searchPosts`. The upsell copy dies; Bluesky OAuth stays only for cross-posting (FR-14) and follow import |
| **D11** category→data-source map | **Changed.** Following = `appview_follows`-driven; Feeds = feed generators via local AppView; Hashtags = local index search (v1: Mastodon hashtag API); Subscriptions = **open question** (v1: Lemmy/PieFed — see OQ-2); RSS = deferred |

### 2.3 v1 defects fixed by this spec (evidence-grounded)

1. **Default sort is `popular`** — contradicts D1, v2's mutuals-first reverse-chrono default, and the no-algorithm identity. Fixed: default `new`; stored-value migration handles the old `popular`/`chronological` localStorage states explicitly.
2. **Filter chips clear everything** — `active-filter-chip` `onClick` calls `onClearFilters()` regardless of which chip (or its ✕) is clicked; EXPERIENCE.md promises per-chip removal. Fixed: chip ✕ removes one filter; "Clear all" stays as the separate control.
3. **Two parallel platform filters** — sidebar category checkboxes drive the server-side `source_categories` param while the mobile "Platforms" dropdown filters client-side over already-fetched posts (FeedColumn comment admits it: "Sources dropdown is UI-only"). One server-side filter pipeline in v2; the mobile dropdown becomes a thin client over it.
4. **Duplicate sort controls** — desktop toolbar renders both a `<select>` and a Popular/New toggle. One control per breakpoint (toggle everywhere; select removed).
5. **Console.log instrumentation in production paths** — `loadMore` logs 8+ lines per invocation. Removed.
6. **`window.confirm`/`alert()` for post delete and delete failures** — replaced by the DeleteConfirmationPopup pattern + toasts on the event bus.
7. **~300 lines of inline styles in PostCard** (share dialog, ⋯ menu, carousel arrows, counter pills) and **~115 in NotificationBell** — moved to CSS classes so theming and responsive rules apply.
8. **External lookups where the local index suffices** — QuotePostEmbed renders images via `cdn.bsky.app` and video via `video.bsky.app`; v2 resolves quoted posts and media from the local AppView (D6/D12).
9. **Fake presence indicators** in SocialPanel (hardcoded `online/idle` cycle). Removed; no replacement until real presence exists.
10. **Composer counts UTF-16 code units, not grapheme clusters** — miscounts emoji/combined characters against AT Proto's 300-grapheme limit. Fixed with `Intl.Segmenter`.
11. **Composer visibility select is a promise the protocol can't keep** — public/followers/friends with no enforcement path until FR-16. Removed from v2.0 composer (Phase 2).
12. **Avatar fallback uses `handle[1]`** — assumes a leading `@`; renders `?` for bare handles. Strips then takes `[0]`.
13. **`.tab-bar-left` forced to 280px inside the ≤900px media query** and contradictory `.mobile-sidebar-toggle` rules (`display: block` then `display: none !important` in the same query) — cleaned in the responsive rebuild.
14. **Toasts built two ways** — FeedLayout listens on `bytescape:toast` (correct) while PostCard mints its own `document.createElement` toasts with hardcoded colors. One toast system: the event bus.

---

## 3. Layout

The v2 feed keeps the v1 arrangement — it already realizes the UX session's desktop 3-panel IA (`EXPERIENCE.md`) and the shipped CSS grid (`280px 1fr 300px`, verified in `styles.css`). What changes is the ordering contract inside the feed column (mutuals-first) and the responsive tiers (section 7).

### 3.1 Desktop (>900px) — 3-panel workspace

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR   [.tab-bar-left = 280px]  [📰 Feed] [👤 @you] [＋…]         [🔔 3] [☰] │
├──────────────────┬────────────────────────────────────────┬──────────────────────┤
│ SOURCE SIDEBAR   │  FEED COLUMN                           │ SOCIAL PANEL         │
│ 280px, fixed     │  1fr · content max 600px, centered     │ 300px, fixed         │
│ ┌──────────────┐ │ ┌────────────────────────────────────┐ │ ┌──────────────────┐ │
│ │> Search/add..│ │ │ ☰ [Sources ▾]  [ New | Popular ] ⚙ │ │ │[FRIENDS] [CHAT]  │ │
│ │ All People…  │ │ ├────────────────────────────────────┤ │ ├──────────────────┤ │
│ └──────────────┘ │ │ filter chips (only when active):   │ │ │ ┌─ mutuals ────┐ │ │
│                  │ │ [#cats ✕] [@bodegacats ✕] [Clear…] │ │ │ │ [av] @mika   │ │ │
│ ☑ 👤 FOLLOWING 4▶│ ├────────────────────────────────────┤ │ │ │ [av] @jon    │ │ │
│   > filter…      │ │ ┌─ post card ─────────────────────┐│ │ │ │ [av] @skeeter│ │ │
│   ┌────────────┐ │ │ │[av] Mika  @mika…  [🦋 bsky]  2h ││ │ │ └──────────────┘ │ │
│   │ [av] @mika✕│ │ │ │ post text · media · embeds     ││ │ │                    │ │
│   │ [av] @jon ✕│ │ │ │ ❤️ 3 · 👍 1   ↗ 2   ↩ 5      ⋯  ││ │ │ click a friend →   │ │
│   └────────────┘ │ │ └─────────────────────────────────┘│ │ │ CHAT tab opens/    │ │
│ ☑ 📡 FEEDS     3▶│ │ ┌─ post card ─────────────────────┐│ │ │ starts a DM        │ │
│ ☑ # HASHTAGS   2▶│ │ │ …                                ││ │ │                    │ │
│ ☑ 🔔 SUBSCRIPT.1▶│ │ └─────────────────────────────────┘│ │ │ ⛶ expands chat     │ │
│ ☐ 📰 RSS (Ph.2) ▶│ │       [ > LOAD MORE POSTS ]        │ │ └──────────────────┘ │
│                  │ │       (infinite-scroll sentinel)   │ │                      │
└──────────────────┴────────────────────────────────────────┴──────────────────────┘
 grid: 280px · 1fr · 300px — each panel scrolls independently; tab bar fixed
 left panel flat, 1px right border (--border-dim); no shadows except drawer/popups
```

**Mutuals-first default arrangement.** Mutuals-first is an *ordering and loading* contract, not a visual section — the timeline stays one reverse-chronological stream (D1), and the philosophy shows through in what arrives first:

1. **Phase 1 — seed (blocking, target <500ms):** posts from mutual follows only (both follow edges exist in `appview_follows`), newest first. First paint is genuine connection — this is the SM-4 mechanism and the identity statement in the same request.
2. **Phase 2 — full (background):** the merged reverse-chronological feed (mutuals + one-way follows + feeds + hashtags + subscriptions) is prefetched and becomes the next page. No visible swap, no re-sort jank: the prefetched page is deduped against what's on screen and appended (v1 `prefetchedPage` pattern, kept).
3. **No "For you" / "Friends" tab split.** The mutuals-first tuning lives in the seed, and users who want pure mutuals select the Following category only (category checkbox, one click). Tabs would re-impose the algorithmic-feed structure the product exists to reject.

The social panel's FRIENDS tab renders the same mutuals set (plus non-mutual follows below, dimmed) — the right panel is the human face of the left panel's data.

### 3.2 Tablet (551–900px) — feed + drawer

The grid collapses to `1fr` (v1 shipped behavior). The social panel is not rendered — chat stays reachable via the Chat tab (its own screen) — and the sidebar becomes a drawer.

```
┌────────────────────────────────────────────┐
│ TAB BAR  [📰 Feed] [👤 @you]     [🔔] [☰] │
├────────────────────────────────────────────┤
│ ☰ [Sources ▾]      [ New | Popular ]   ⚙  │  ← toolbar (compressed)
│ ┌─ post card ────────────────────────────┐ │
│ │ …                                      │ │
│ └────────────────────────────────────────┘ │
│ ┌─ post card ────────────────────────────┐ │
│ └────────────────────────────────────────┘ │
│            [ > LOAD MORE POSTS ]           │
├────────────────────────────────────────────┤
│ 📰 Feed · 👤 Profile · ＋ · 💬 Chat · ☰ More│  ← bottom nav, 56px + safe-area
└────────────────────────────────────────────┘

Drawer (☰ in toolbar, or edge swipe):
┌─────────────────────────────┐ ┌─────────────────────────────┐
│ > Search/add…           [✕] │ │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│
│ ☑ 👤 FOLLOWING           4 ▼│ │▒▒▒ overlay rgba(0,0,0,.5) ▒│
│   > filter…                 │ │▒▒▒ click closes drawer    ▒│
│   ┌───────────────────────┐ │ │▒▒▒                         ▒│
│   │ [av] @mika         ✕  │ │ │▒▒▒                         ▒│
│   │ [av] @jon          ✕  │ │ │▒▒▒                         ▒│
│   └───────────────────────┘ │ │▒▒▒                         ▒│
│ ☑ 📡 FEEDS               3 ▶│ │▒▒▒                         ▒│
│ ☑ # HASHTAGS             2 ▶│ │▒▒▒                         ▒│
│ ☑ 🔔 SUBSCRIPTIONS       1 ▶│ │▒▒▒                         ▒│
└─────────────────────────────┘ └─────────────────────────────┘
 85vw, max 320px, slides from left 200ms ease-out,
 box-shadow 4px 0 16px rgba(0,0,0,.3) — v1 geometry kept verbatim
 accordion: ONE category expanded at a time on mobile (DESIGN.md)
```

### 3.3 Mobile (≤550px) — sort-first toolbar, composer in the nav

The sc-mobile-sort scenario (2026-07-22, implemented in v1, carried forward): the toolbar is strictly filtering/sorting — no `+` button, no "Say something…" input — and compose moves to the centered `＋` in the bottom nav.

```
┌─────────────────────────────┐
│ TAB BAR  [📰 Feed]  [🔔] [☰]│
├─────────────────────────────┤
│ ☰ [Sources ▼] [New|Popular]⚙│  ≤550px toolbar
│ ┌─ post card ─────────────┐ │
│ │ [av] @mika [🦋] 2h      │ │
│ │ post text…              │ │
│ │ ❤️ 3 · ↗ 2 · ↩ 5    ⋯   │ │
│ └─────────────────────────┘ │
│ ┌─ post card ─────────────┐ │
│ └─────────────────────────┘ │
│      [ > LOAD MORE POSTS ]  │
├─────────────────────────────┤
│ 📰    👤    ＋    💬    ☰   │  bottom nav: Feed · Profile · ＋ · Chat · More
│ Feed Prof        Chat More   │  ＋ = accent, enlarged, dispatches openComposer
└─────────────────────────────┘  Accounts lives under More (scenario decision)
```

- **Sources ▼** opens a dropdown of the four categories (server-side filter — the v1 client-side-only dropdown is wired to the real filter pipeline in v2; see 2.3 #3).
- **New | Popular** segmented toggle, always visible (the v1 bug where the sort `<select>` was `display:none` at ≤550px is what motivated the scenario; the toggle is the fix and stays).
- Drawer, post cards, and load-more are identical to tablet — only the toolbar row changes.

---

## 4. Component Breakdown

### 4.1 Screen shell

**`FeedScreen` (KEEP)**
- Purpose: mount FeedLayout + ComposerModal; bridge the composer to the event bus.
- Kept from v1: listens for `bytescape:openComposer` (fired by MobileNav `＋`, TabBar, keyboard shortcut), renders ComposerModal conditionally.
- Changed: `onPost` no longer fires a blanket `bytescape:refreshFeed`. Read-after-write guarantees the post is already indexed, so the created post object returned by the API is **prepended to the feed locally** (instant, no refetch, no scroll jump). `bytescape:refreshFeed` is reserved for mutations whose effect can't be computed locally (repost, mute, source delete).
- OAuth callback phase (`?provider=…&success=…` → OAuthCallback) kept for FR-27 Bluesky OAuth returns.

### 4.2 Left panel

**`SourceSidebar` (REFINE from CategorySidebar)**
- Purpose: the control center (D2) — browse, filter, search, add, and delete every feed source.
- Props: `categories: Category[]`, `activeFilters: Set<string>`, `selectedSources: Set<string>`, `onToggleItem`, `onToggleSource`, `onDelete`, `onSearch`, `onAddResult`, `variant: 'panel' | 'drawer'`.
- State (local): `filterTexts` per category, `deleteTarget`, `mobileOpen` (drawer), global search `query/results/searching/mode`.
- Kept from v1: global search with mode chips (All/People/Hashtags/Feeds/Communities) and smart `#` → hashtag-mode detection; 300ms debounce; per-category local filter inputs; category-level include checkboxes (drive the server-side `source_categories` param); chip items with selection highlight + `✕`; drawer geometry and `bytescape:openSidebar` event.
- Changed:
  - **Following renders as rows** (avatar + handle + mutual marker `⇄`), not chips — people are the mutuals-first heart of the product and deserve avatars. Feeds/Hashtags/Subscriptions keep the dense chip wrap.
  - Category sections: all expanded on desktop (v1 default was all-*collapsed* — EXPERIENCE.md says all expanded; v2 follows the spec), accordion (one open) in the drawer.
  - RSS row renders as a reserved Phase-2 slot: dimmed `📰 RSS — Phase 2`, no expand/search (FR-8).
  - Item counts per category header (v1 `category-count` kept).
- Subcomponents (promoted from inline JSX to components):

**`CategorySection`** — header (checkbox + icon + label + count + chevron) + collapsible body (filter input + item list). 200ms max-height transition (DESIGN.md).

**`SourceItem`** — row/chip for one source. Selection = 2px left border `--accent` + accent text (re-valued token, 2.1). `✕` delete affordance: desktop hover-reveal, always visible in drawer; hit target ≥24px and fully outside the selection click area (DESIGN.md don't, v1 refinement).

**`SearchBar` (global)** — 28px input, `> Search/add...` placeholder, spinner while searching, results list below with avatar, label, truncated description, platform badge, and green `+` add button. Click-outside and Escape dismiss results (v1 kept). Adding fires `onAddResult` → toast "Added to your feed!" → the new item appears in its category with a brief highlight (EXPERIENCE.md interaction primitive).

**`DeleteConfirmationPopup` (NEW — promoted to shared component)**
- Purpose: the D4 guard — one confirmation surface for every destructive removal (source delete from sidebar; own-post delete from PostCard).
- Spec (UX session, kept verbatim): centered modal, 320px max, `--bg-secondary`, 8px radius, shadow `0 8px 32px rgba(0,0,0,.5)`; title ("Delete this source?" / "Delete this post?"), the item's name quoted, one warning line; `[Cancel]` secondary + `[🔴 DELETE]` red (`--danger`) bold larger.
- Behavior: **Cancel gets default focus** (Enter = Cancel — v1 already does this with `autoFocus`); focus trap; Escape = Cancel; Tab cycles the two buttons; **overlay click does nothing** (v1 dismissed on overlay click — removed to honor "forces explicit choice").
- After confirm: source delete → API unsubscribe + item fades out (opacity 200ms) + feed refilters; post delete → optimistic removal from the list.

### 4.3 Feed column

**`FeedColumn` (REFINE)**
- Purpose: toolbar + filter chips + the reverse-chronological post stream + inline interest picker.
- Props: `posts`, `loading`, `error`, `loadingMore`, `hasMore`, `sentinelRef`, `activeFilters`, `onRemoveFilter`, `onClearFilters`, `sortMode`, `onSortChange`, `viewMode` (`feed | interests`), interests callbacks.
- State (local): mobile Sources dropdown open/selection (delegates up to the server filter), `sourceFilter`.
- Kept: toolbar shape, `⚙` edit-interests entry, inline interest picker (onboarding + edit), empty/error states with `> RETRY`, infinite-scroll sentinel **and** `> LOAD MORE POSTS` button (both — the button is the fallback and the retro signature), `bytescape:loadMore` dispatch.
- Changed:
  - **Skeleton first paint:** 3 shimmer post-card placeholders during phase-1 load (EXPERIENCE.md state pattern; v1's "LOADING…" text block retired), then fade-in of real cards.
  - Sort control: one Popular/New segmented toggle at every breakpoint (duplicate `<select>` removed). Default **New** (2.3 #1); choice persists to `localStorage.bytescape_feed_sort`.
  - The mobile Platforms dropdown becomes the **Sources ▼** category filter wired to the server pipeline (3.3).
  - Import-source filter chips (facebook/instagram/…) removed (D7).
  - Interest picker consolidates with the profile spec's simplified picker (static `INTEREST_CATEGORIES` + manual entry, profile spec 4.4) — one shared component, selection returned by callback.

**`FilterChipBar` (NEW — extracted from FeedColumn internals)**
- Purpose: make the active additive filter set visible and individually removable (D3, EXPERIENCE.md spec — v1 had the bar but every click cleared everything).
- Rendered only when `activeFilters.size > 0`: pill chips `[icon] [label] [✕]` — `#cats`, `@handle`, feed name — plus a `Clear all` link at the right end when ≥2 chips. Horizontal scroll on overflow (DESIGN.md).
- Interactions: `✕` removes exactly that filter (feed refetches via the sources endpoint); chip body click also removes (matches sidebar toggle-off semantics); `Clear all` empties the set → "All" mode.
- Accessibility floor (EXPERIENCE.md): chips keyboard-focusable, Backspace on a focused chip removes it, and an `aria-live` region announces changes — "Filtered to 2 sources: Cats of Bluesky, #cats".

### 4.4 Post card

**`PostCard` (REFINE — split into 4 files)**
- Purpose: one post from any source — identity row, content, media/embeds, engagement. The most-salvaged component on the screen; every rendering pipeline survives.
- Props: `post: FeedPost`, `currentUserHandle`. State: `expanded` (comments), `selectedEmoji`, `localCounts`, `pickerOpen`, `shareOpen/shareComment`, `menuOpen`, reply state.
- **Split:** `PostCard` (header/body/actions) · `PostMedia` (ImageCarousel, CarouselLightbox, HlsVideo) · `PostEmbeds` (YouTube, SoundCloud, Bandcamp, LinkPreview, LinkifiedText) · `QuoteEmbed` · shared dialogs reuse `DeleteConfirmationPopup`. Inline styles (~300 lines) move to CSS classes.
- Kept pipelines, unchanged in behavior: unified media pipeline (media[] videos → HLS player with autoplay-muted-loop; images → single-cover carousel with arrows, dots, swipe, counter → lightbox with keyboard nav); embeds[] YouTube/SoundCloud/Bandcamp/link cards; content-URL extraction only when media[]/embeds[] are empty (no double-render); Lemmy variant header (title + `c/community` chip) stays conditional for as long as Subscriptions can hold Lemmy sources (OQ-2); repost/boost banners; cross-post badges (🦋/🐘 with 50% opacity on failure — FR-14 per-platform results); `formatTime` future-date guard.
- Changed: avatar fallback strips `@` then takes `[0]` (2.3 #12); toasts go through `bytescape:toast`; delete uses the shared popup (below).

**Reactions (FR-15).** The v1 emoji system is kept whole — it *is* the Facebook-level engagement the PRD wants: main reaction button shows your emoji (or ❤️ default) + count, inline badges for other emojis sorted by count, popover picker with 9 emojis (`❤️ 👍 🔥 😄 😂 😮 😢 😡 👎`), per-emoji counts in the popover, click-same-emoji toggles off. Optimistic count update with server reconciliation, rollback on failure (v1 pattern kept). Record mapping for v2: **❤️ is the canonical like** — on AT posts it writes `app.bsky.feed.like` (federates, feeds `appview_likes`); the other eight emojis write a Bytescape-local reaction record (they don't federate — Bluesky has no emoji-reaction lexicon). Flagged for confirmation in OQ-4.

**Reposts and quote posts (FR-15).** One dialog, two outcomes — v1's share dialog kept and made explicit:
- Comment empty → **repost**: `app.bsky.feed.repost` record; count increments optimistically; the post re-appears in followers' feeds with a "🔁 Reposted by" banner.
- Comment present → **quote post**: new `app.bsky.feed.post` with `app.bsky.embed.record` pointing at the original; renders via `QuoteEmbed`.
- Dialog spec (v1 kept): quoted-post preview box (author, text, ≤4 media thumbs), optional comment textarea, Cancel / Share; success state "Shared! ✓" auto-closes after 1.2s.
- `QuoteEmbed` resolves the quoted post from the **local AppView** (`appview_posts` by URI) instead of v1's `/api/bluesky/post` proxy + `cdn.bsky.app` image URLs (2.3 #8); failure fallback stays: "Quoted post unavailable — view on Bluesky" link.

**Comments.** Card click expands the comment section inline (v1 kept): flat list with author/time/like-count, reply composer at the bottom. v2 adds one level of threading — `parent_id` already exists in the v1 comment shape; replies render indented under their parent. Deeper thread rendering: Phase 2.

**Own-post delete.** `🗑` action (own posts only) → `DeleteConfirmationPopup` ("Delete this post?", post excerpt, "This deletes it from your PDS and it can't be recovered.") → `DELETE /api/posts/{id}` → PDS record deletion; optimistic removal; on failure the popup's error text shows the server's reason (v1 surfaced remote-delete errors — kept, but in-popup instead of `alert()`).

**⋯ menu (others' posts).** Mute author (FR-26 — optimistic feed removal + toast, v1 kept), Report (v1 stub "coming soon" kept as a stub; moderation is out of this spec).

**Follow-on-card.** `+ Follow` / `Following` / `Pending…` + `Cancel Request` inline button (v1 kept) — the discovery affordance that turns a good post into a source.

### 4.5 Right panel

**`SocialPanel` (REFINE)**
- Purpose: the human layer — who's here (FRIENDS) and talk to them (CHAT).
- Props: `onExpandChat`. State: `activeTab`, `friends`, conversations/messages/input.
- Kept: FRIENDS/CHAT tab switch; friends = mutuals merged with one-way follows, deduped by handle (v1 logic, now sourced cleanly: mutuals from the bidirectional `appview_follows` join, avatars from `appview_profiles`); click friend → open existing DM or start one (FR-19); chat room list → conversation view with back button, `⛶` expand; `> TYPE A MESSAGE...` input, Enter sends.
- Changed: fake presence dots deleted (2.3 #9) — no avatar renders a status dot until real presence exists; mutuals render first with `⇄`, one-way follows below, dimmed; empty states: "No mutual connections yet — follow people and get followed to build your friends list." / "No conversations yet. Click a friend to start a DM." (v1 second one kept).
- Rendered only >900px; below that, chat is its own tab.

### 4.6 Composer

**`ComposerModal` (REFINE)**
- Purpose: FR-13 composition — one modal for posts, media, and cross-posting.
- Props: `onClose`, `onPost(created)`. State: `content`, `mediaFiles`, `charCount` (graphemes), `crosspostBluesky`, `submitting`, `error`, mention/autocomplete state (new).
- Kept: modal shell + overlay, media thumbs with remove `×`, `📎 Media` file input (image/video, multiple), error text with status code, disabled-while-posting.
- Added (FR-13): **@mention autocomplete** (searches `appview_profiles`, inserts handle), **mood/status prefix** (retro flavor: a short prefixed tag like `~ feeling nostalgic` prepended to the post text), **location check-in** (optional place name appended as a chip line), **link attachments** (URL in text gets the OG preview at render time — no extra composer work). Rich formatting: AT Proto posts are plain text with facets — bold/italic via Unicode transforms is out of scope; mentions/links/hashtags are the supported facets.
- **Character limit: 300 grapheme clusters** counted with `Intl.Segmenter` (2.3 #10); counter turns `--danger` over limit and disables Post. Empty posts (no text, no media, no link) rejected per FR-13.
- **Cross-posting (FR-14): Bluesky only.** The Mastodon toggle is gone. One switch: `🦋 Cross-post to Bluesky`. For Bluesky-hosted (`did:plc`) users the post lands on bsky.social by definition — the switch is hidden (always true). For Bytescape-hosted (`did:web`) users the record is already an AT Proto post on their PDS; what the switch additionally does (relay bridging visibility, nothing, or a linked-account mirror) is OQ-5. Per-platform result badges on the posted card (🦋 ok / 🦋 50%-opacity failed) kept from v1 — FR-14 requires per-platform success/failure, and Bluesky failure never blocks the Bytescape post.
- **No visibility select** — public-only in v2.0; privacy modes return with FR-16 (Phase 2).
- Media upload: per-file parallel upload with progress (v1 uploaded sequentially and blocked on first failure — refined), then one post create. Success → `onPost(created)` → optimistic prepend (4.1) → close.

### 4.7 App chrome (kept)

**`MobileNav` (KEEP)** — as-is: `Feed · Profile · ＋ · Chat · More`, centered accent `＋` dispatching `bytescape:openComposer`, Accounts under More. Rendered ≤900px, 56px + `env(safe-area-inset-bottom)`.

**`TabBar` (KEEP)** — browser-style open/closable tabs; Feed pinned (no `×`), `@handle` label on Profile when viewing others, hamburger dropdown for unopened tabs + logout. Feed contract: `.tab-bar-left` exactly 280px on desktop only (the ≤900px `!important` bug removed — 2.3 #13), hosts NotificationBell.

**`NotificationBell` (KEEP, CSS hygiene)** — behavior unchanged: unread-count badge (`9+` cap), dropdown on open, per-item read on click, "Mark all as read", 30s polling. The two touch-ups from 2.3: inline styles → CSS classes; polling pauses when `document.hidden`. Notification routing (reaction/follow → relevant post/profile) added as a refinement.

### 4.8 Shared plumbing

**Toast (single system).** One renderer in the screen shell listening on `bytescape:toast` `{message, type: success|error}`; 3s auto-dismiss; bottom-center. All components dispatch — nobody mints DOM (2.3 #14).

**Event bus contract (kept per stack decision):**

| Event | Producer(s) | Consumer(s) |
|---|---|---|
| `bytescape:openComposer` | MobileNav `＋`, TabBar, post-card quote action | FeedScreen |
| `bytescape:refreshFeed` | composer (only for non-local mutations), repost, mute, source add/delete | FeedLayout |
| `bytescape:openSidebar` | FeedColumn `☰` | SourceSidebar (drawer) |
| `bytescape:openInterests` | FeedColumn `⚙`, empty-feed prompt | FeedLayout (viewMode) |
| `bytescape:loadMore` | FeedColumn button | FeedLayout |
| `bytescape:toast` | any component | Toast renderer |
| `bytescape:viewProfile` | PostCard author click (local users) | App (profile tab) |
| `bytescape:accountLinked` | OAuthCallback | FeedLayout (reload sources + feed) |
| `bytescape:profileUpdated` | profile screen (interests changed) | FeedLayout (re-sync interests, reload) |

---

## 5. States

### 5.1 Loading (two-phase seed/full)

The loading design is the SM-4 (≤2s) mechanism made visible: paint fast with what matters most, fill in the rest without disturbing the reader.

| Phase | Surface | Behavior |
|---|---|---|
| First paint | Feed column | 3 skeleton post cards (shimmer blocks in card shape — avatar circle, 3 text lines, action row); sidebar and social panel render immediately from cached/static data (sidebar categories are known before any fetch returns) |
| Phase 1 — seed (blocking, target <500ms) | Feed column | Mutuals-only request; skeletons cross-fade into real cards (150ms). Toolbar and filter chips are already interactive |
| Phase 2 — full (background) | Nothing visible | Merged reverse-chrono page prefetched into `prefetchedPage`; becomes the next pagination page, deduped against on-screen posts (v1 pattern kept). If it fails, fall back to cursor pagination from the seed — silent, no error surface |
| Load more (scroll or button) | Bottom of list | Spinner row "Loading more posts..." + button disabled while fetching; prefetched page applies instantly when available |
| Sort change | Feed column | List clears to skeletons immediately (v1 `onSortChange` resets posts/cursor/prefetch — kept), new sort fetches |
| Search typing | Sidebar | Spinner inside the search bar; results region shows `> Searching...` (EXPERIENCE.md) |
| Source adding | Sidebar | Result row flips to "Adding..." then the item appears at the top of its category with a 600ms accent-highlight pulse (EXPERIENCE.md interaction primitive) |
| Source deleting | Sidebar | Item fades out (opacity 200ms) after confirm, then unmounts |
| Composer posting | Composer | Button "Posting...", media thumbs show per-file upload progress; modal stays open on failure so the draft is never lost |

### 5.2 Empty

| Case | Copy / behavior |
|---|---|
| No sources at all (no follows, no subscriptions, no interests configured) | Feed column swaps to the inline interest picker: "WELCOME — Pick some interests to populate your feed. You can change these anytime." (v1 flow kept; FeedLayout auto-dispatches `bytescape:openInterests`) |
| Has interests/sources but zero posts returned | "NO CONTENT YET — Connect more accounts or add interests to populate your feed." + `⚙` edit-interests entry (v1 kept) |
| Zero mutuals (follows exist, none follow back) | Seed phase returns empty → full merged feed paints as first paint instead (no empty intermediate state); FRIENDS panel: "No mutual connections yet — follow people and get followed to build your friends list." See OQ-7 for whether the seed should widen to all follows |
| Filter active, nothing matches | "NO CONTENT MATCHING FILTERS — Adjust your source or platform filters to see posts." + `Clear all` action inline (v1 copy kept, action added) |
| Sidebar category with no items | "Nothing here yet." under the filter input (v1 kept) + the search bar above is the affordance to fill it |
| Chat: no conversations | "No conversations yet. Click a friend to start a DM." (v1 kept) |
| Search: no results | "No results found for '{query}'" in the results region (EXPERIENCE.md) |
| End of pagination | Button hidden, `hasMore=false`; a quiet `> END OF FEED — you've read it all` line (retro sign-off, new in v2) |

### 5.3 Error

| Case | Behavior |
|---|---|---|
| Feed fetch fails (network/5xx) | Full-column error state: "CONNECTION ERROR" + message + `> RETRY` (v1 kept). Existing posts stay if the failure happens during load-more — append an inline error row with retry instead of clearing the list |
| Seed fails | Fall back to discover endpoint (v1 pattern kept); if that also fails, the error state above |
| Rate limited (429) on any feed/search call | Toast: "Slow down — try again in a moment." Client backs off (no auto-retry storm); the control that triggered it shows a disabled state for the backoff window |
| Source add fails | Toast "Failed to add source" (v1 kept); search result row returns to its `+` state |
| Source delete fails | Toast "Failed to delete source"; item stays mounted (no fade-out on failure) |
| Post create fails | Error text with status inside the composer (v1 kept); draft + media retained |
| Cross-post partial failure (FR-14) | Bytescape post succeeds, Bluesky fails → success toast mentions it ("Posted. Bluesky cross-post failed.") and the card's 🦋 badge renders 50%-opacity with `(failed)` title (v1 badge behavior kept) |
| Reaction/repost/comment write fails | Optimistic state rolled back (v1 pattern kept) + toast |
| Quote embed unresolvable | "Quoted post unavailable — view on Bluesky" fallback link (v1 kept; lookup moves to local AppView per 2.3 #8) |
| OG link preview fails | Bare-domain link card (v1 fallback kept) |

### 5.4 Edge cases

- **Source deleted while it's in the active filter set** — deleting a source also removes its filter entry (v1 does this); if the feed was filtered to it, the filter set shrinks and the feed refetches with what remains; if the set empties, return to "All". A chip arriving from a stale session/state for a nonexistent source is dropped silently on the next fetch — never render a chip without a source behind it.
- **Feed generator deleted/unreachable** — the Feeds item stays in the sidebar (it's a subscription record) but its posts stop arriving; on fetch error specific to a generator, mark the item `⚠ unreachable` in the sidebar (dimmed + title tooltip) rather than failing the whole feed. Feed content is aggregated server-side; one dead source never blanks the timeline.
- **Zero mutuals** — see 5.2; the mutuals-first contract degrades to "everything, reverse-chrono" without ceremony. New users with follows-but-no-mutuals are the onboarding case, not an error case.
- **Blocked user (AT Protocol)** — blocks are bidirectional on the network: posts from authors who block the viewer (or whom the viewer blocks) are excluded server-side from feed responses. The client renders nothing for them — no tombstone, no "blocked" marker in the feed. The mute path (⋯ menu) is the viewer-facing control and stays optimistic-removal + toast.
- **Content labels / NSFW (`appview_posts.labels`)** — new in v2 (v1 ignored labels entirely). Posts carrying adult/graphic-media labels render **blurred media with a click-through veil** ("⚠ sensitive content — click to reveal"); reveal state is per-post per-session, never persisted. Text is never blurred. Which labels blur by default vs. require a settings toggle: OQ-8.
- **Future-dated posts** — `formatTime` already renders them as "now" (v1 spam guard kept); v2 additionally caps them below the mutuals seed so fake timestamps can't buy top position.
- **Duplicate posts across sources** — one post can arrive via Following *and* a hashtag *and* a feed. Dedupe by post `id`/`uri` at merge time (v1 `seen` set kept for pagination; extended to first-page merges).
- **Very long posts** — `post-body-long` clamp kept (v1): collapse past ~6 lines with a `> show more` expander.
- **Imported archive posts** — can never appear here by architecture (1.1); no defensive UI needed, and the v1 import-source filter chips are gone.

---

## 6. Interactions

### 6.1 Source filtering (additive multi-select — D3)

1. Click a source item in the sidebar → it joins the active filter set (selection highlight: 2px left border `--accent` + accent text). The feed refetches showing only that source's posts (debounced 200ms — EXPERIENCE.md primitive).
2. Click more items → **additive OR**: the feed shows posts from any selected source. This is how users build ad-hoc custom feeds — the product's per-source-control thesis in one gesture.
3. The FilterChipBar appears above the feed with one chip per selected source; `✕` (or Backspace on a focused chip) removes exactly that source; `Clear all` appears at ≥2 chips.
4. Click a selected item again → toggles off. Empty set = **"All" mode** — the full merged timeline. There is no explicit "All" button; absence of filters *is* All (D1).
5. **Category-level** checkboxes (sidebar headers) are the coarser control: unchecking `👤 Following` excludes the whole category server-side (`source_categories` param) without touching item-level selections. Both levels compose.
6. Every filter change is announced for screen readers: `aria-live` — "Filtered to 2 sources: #cats, @bodegacats" (EXPERIENCE.md accessibility floor).
7. Pagination resets on any filter change (cursor + prefetch cleared — v1 behavior kept).

### 6.2 Search — global discovery + per-category filter

1. The sidebar's single search bar (v1 evolution of D5, kept): type ≥2 chars → 300ms debounce → concurrent search across people (`appview_profiles`), hashtags (local index), feed generators, communities; results grouped with platform badges.
2. Mode chips (`All · People · Hashtags · Feeds · Communities`) narrow the query; typing a leading `#` auto-switches to Hashtags mode and strips the `#` (v1 smart detection kept).
3. Each result row: avatar, label, truncated description (100 chars), `+` button. Click `+` (or the row) → adds the source to its category (follow / feed subscription / hashtag subscription), clears the query, toast "Added to your feed!", new item pulses in its category. The feed picks the source up on next refresh.
4. Click-outside or Escape dismisses results (v1 kept).
5. **Per-category inputs are local filters, not searches**: they narrow the already-subscribed item list client-side, instant, no API (v1 kept — this is why the global bar owns discovery).
6. Search never blocks the feed (DESIGN.md don't) — results live in the sidebar; the feed updates only when a source is actually added.

### 6.3 Post composer (FR-13)

1. Open: MobileNav `＋`, TabBar, or keyboard → `bytescape:openComposer` → FeedScreen mounts ComposerModal (v1 wiring kept).
2. Compose: textarea auto-focused; `@` triggers mention autocomplete (searches `appview_profiles`, arrow-keys + Enter/Tab insert, facet-aware); optional mood prefix (`~ …`) and location chip; links in text are fine as-is (OG preview happens at render).
3. Attach: `📎 Media` → multi-select images/video; uploads run in parallel with per-thumb progress and `×` removal (v1's sequential blocking upload refined).
4. Counter: `{graphemes}/300` via `Intl.Segmenter`; over-limit turns `--danger` and disables Post. Empty post (no text, no media, no link) rejected per FR-13.
5. Cross-post (FR-14): `🦋 Cross-post to Bluesky` switch — Bluesky-only, no Mastodon toggle. Hidden for `did:plc` users (always true by definition); semantics for `did:web` users per OQ-5.
6. Post → API writes `app.bsky.feed.post` to the user's PDS → **read-after-write**: the response carries the created post, which is prepended to the feed locally — the user closes the modal onto their own post already at the top. No full refetch, no `bytescape:refreshFeed`.
7. Failure: error text with status in the modal; draft, media, and all state retained; retry is one click.
8. Cross-post failure never blocks the Bytescape post (FR-14) — success toast notes it, card badge shows the failed platform at 50% opacity.

### 6.4 Reactions (FR-15 — ❤️ canonical like)

1. The action row shows one main reaction button (your emoji, or ❤️ by default) + count, plus inline badges for every other emoji that has counts, sorted descending.
2. Click the main button or a badge → popover picker: `❤️ 👍 🔥 😄 😂 😮 😢 😡 👎` with per-emoji counts.
3. Click an emoji → **one reaction per user, switchable**: the new emoji is added and the previous one removed in the same write. Click your current emoji → toggles off entirely (v1 semantics kept).
4. Record mapping: **❤️ = `app.bsky.feed.like`** on the viewer's PDS (federates; indexed into `appview_likes`; drives `like_count`). Other emojis = Bytescape-local reaction record (no AT lexicon exists for them — they're a Bytescape feature, not a federation promise). OQ-4 confirms the local lexicon.
5. Optimistic everywhere: counts update instantly, server response reconciles, failure rolls back + toast (v1 pattern kept). Reactions on non-AT sources (e.g. a Lemmy subscription post, while OQ-2 is open) use the v1 local-reaction path.

### 6.5 Repost vs quote post (one dialog, two outcomes)

1. `↗` opens the share dialog: optional comment textarea + quoted-post preview (author, text, ≤4 media thumbs — v1 kept).
2. **Comment empty → repost**: writes `app.bsky.feed.repost`; optimistic count +1; the post re-appears in followers' feeds with "🔁 Reposted by @you".
3. **Comment present → quote post**: writes a new `app.bsky.feed.post` with `app.bsky.embed.record` referencing the original; the new post enters the feed via read-after-write prepend; the original renders its `QuoteEmbed` wherever it appears.
4. Success: "Shared! ✓" state, auto-close after 1.2s (v1 kept), `bytescape:refreshFeed` fires (reposts change *others'* feeds — a refetch is the honest local effect for the repost count/banner on this card).
5. Reposting your own post is allowed (AT Proto permits it); the dialog is identical.

### 6.6 Delete with confirmation (D4 — one popup, two surfaces)

1. **Source delete**: hover a sidebar item → `✕` (always visible on touch) → DeleteConfirmationPopup: "Delete this source?" + quoted name + "This will remove it from your feed. You can always add it again later."
2. **Own-post delete**: `🗑` on your own card → same popup: "Delete this post?" + excerpt + "This deletes it from your PDS and can't be recovered."
3. Popup contract (both surfaces): Cancel has default focus — **Enter cancels**; Tab cycles; Escape cancels; overlay click does nothing (explicit choice only); DELETE is `--danger`, bold, larger.
4. Confirm → source: unsubscribe API + item fade-out + filter cleanup (5.4) + feed refilters; post: `DELETE /api/posts/{id}` (PDS record deletion) + optimistic removal from the list.
5. Failure → toast with the server's reason; nothing unmounts.

### 6.7 Mobile sort + toolbar (sc-mobile-sort, kept)

1. ≤550px toolbar: `☰ | [Sources ▼] | [ New | Popular ] | ⚙` — strictly filtering/sorting; no `+`, no input (compose lives in the bottom nav's `＋`).
2. **New | Popular** segmented toggle, always visible. Switching re-sorts immediately: posts/cursor/prefetch clear, skeletons show, the new sort fetches (v1 `onSortChange` reset kept). Choice persists in `localStorage.bytescape_feed_sort` across sessions.
3. Default is **New** (reverse-chrono — D1 restored, 2.3 #1). **Popular** is an explicit opt-in rank by `like_count + repost_count` from `appview_posts`: a sort the user chooses, not an algorithm that chooses for them. Its continued existence is OQ-1.
4. **Sources ▼** dropdown: the four categories with checkboxes + "All" — the mobile face of the server-side category filter (6.1 step 5), replacing v1's client-side-only Platforms dropdown.
5. `☰` opens the drawer (`bytescape:openSidebar`); `⚙` opens the interest picker (`bytescape:openInterests`).

### 6.8 Event-bus contract — refresh semantics

The event table lives in 4.8; the rule that governs it:

- **Local mutations stay local.** Anything whose outcome the client fully knows — own post created (read-after-write), reaction toggled, filter changed, chip removed — updates state in place. No `refreshFeed`.
- **`bytescape:refreshFeed` fires only when the mutation changes what the server will compute**: repost (banners/counts derived server-side), mute (author exclusion), source add/delete (membership changed), account linked, interests updated. A full refetch is then the cheap, correct answer.
- Producers never call consumers' internals — the bus is the only cross-component channel (stack decision 2026-08-09; no new state library).

---

## 7. Responsive Behavior

| Breakpoint | Layout | Sidebar | Social panel | Toolbar | Nav |
|---|---|---|---|---|---|
| Desktop >900px | 3-panel grid `280px · 1fr · 300px`, feed content max 600px centered | Fixed left panel, all categories expanded, internal scroll | Rendered (FRIENDS/CHAT) | Desktop: `☰ [Sources ▾] [New\|Popular] ⚙` | TabBar only; `.tab-bar-left` = 280px aligns with the sidebar |
| Tablet 551–900px | Single column (`1fr`); feed full-width | Drawer: 85vw / max 320px, left slide 200ms ease-out, overlay `rgba(0,0,0,.5)`, accordion (one category open) | Not rendered — chat via the Chat tab | Same controls, compressed padding | TabBar + bottom nav (56px + safe-area) |
| Mobile ≤550px | Single column, post cards edge-to-edge (`margin: 0 4px` — v1 kept) | Same drawer | Not rendered | Sort-first per sc-mobile-sort: `☰ [Sources ▼] [New\|Popular] ⚙` — no `+`, no input | Bottom nav: `Feed · Profile · ＋ · Chat · More`; `＋` centered, accent-styled, opens the composer |

Additional rules:

- **Breakpoints kept from v1** (900 / 768 / 550 exist in styles.css); 768 carries no feed-specific rules in v2 and folds into the ≤900 tier — two effective tiers plus the ≤550 toolbar swap.
- **Composer is never a toolbar control on touch** — the centered `＋` in the bottom nav is the single entry point ≤900px (the scenario's separate "FAB above the nav" was consolidated into the nav button in the shipped v1; the shipped form is kept). The composer modal renders as a full-screen sheet ≤550px (v1's 768px modal query kept, tightened).
- **Hover-reveal affordances become always-visible on touch**: sidebar `✕` delete buttons and card `⋯` menus are permanent below 900px; all touch targets ≥44px (sidebar chips grow 4px vertical padding in the drawer).
- **Drawer discipline**: opening the drawer scrolls the body behind it lock (`overflow: hidden`); Escape and overlay-click close it; the delete popup renders above the drawer (z-index order per DESIGN.md) so a delete started in the drawer finishes in the drawer.
- **Bottom-nav clearance**: feed scroll container gets `padding-bottom: 56px + safe-area` so `> LOAD MORE POSTS` is never stranded under the nav.
- **Post cards**: media carousel swipe and lightbox gestures are touch-first (v1 already implemented touch handlers — kept); embeds (YouTube/SoundCloud/Bandcamp) keep aspect ratios, iframes lazy-load.
- **No horizontal scroll anywhere** — the v1 `overflow-x: hidden` guard on `.feed-column` at ≤768 stays until the underlying causes (wide embeds) are verified fixed.

---

## 8. Refinement List (concrete deltas over v1)

**Loading & pagination**
- R-1: Mutuals-first made real: seed request gains an explicit mutuals-priority contract (phase 1 = mutual follows from the bidirectional `appview_follows` join), with the full merge prefetched as page 2 — v1's two-phase plumbing kept, its ordering intent specified for the first time.
- R-2: Skeleton shimmer first paint replaces the "LOADING…" text block; sort/filter changes clear to skeletons rather than freezing stale content.
- R-3: Pagination hardened — single cursor pipeline (the v1 cursor-as-both-`cursor`-and-`since` double-pass simplified against the v2 feed API), dedup by `uri` extended from load-more to first-page multi-source merges, `> END OF FEED` sign-off when exhausted.

**Filtering & sources**
- R-4: One server-side filter pipeline — sidebar category checkboxes and the mobile Sources dropdown both drive `source_categories`; the client-side-only Platforms dropdown is retired (2.3 #3).
- R-5: FilterChipBar extracted with true per-chip removal, Backspace support, and `aria-live` filter announcements (v1's clear-everything chip bug, 2.3 #2).
- R-6: Following category renders avatar rows with a `⇄` mutual marker (mutuals-first made visible in the control center itself); other categories keep dense chips.
- R-7: RSS reserved as a dimmed Phase-2 sidebar slot (FR-8) instead of a live category — four categories ship per FR-10.
- R-8: Dead feed generators show `⚠ unreachable` in the sidebar instead of silently starving the feed (5.4).

**Post card**
- R-9: PostCard split into 4 files (card / media / embeds / quote-embed) and ~300 lines of inline styles moved to CSS classes; dialogs reuse the shared DeleteConfirmationPopup and the event-bus toast.
- R-10: Reactions mapped to records — ❤️ writes `app.bsky.feed.like`, other emojis a Bytescape-local record (OQ-4); optimistic updates with rollback standardized across reactions/reposts/comments.
- R-11: Quote and OG lookups resolve from the local AppView — no `cdn.bsky.app`/`video.bsky.app` dependencies in card rendering (2.3 #8).
- R-12: NSFW/content-label handling added: labeled media renders blurred with a per-session click-through veil; text never blurred (OQ-8 for policy).
- R-13: Comment threading one level deep via the existing `parent_id` (FR-15); flat-list rendering retired.

**Composer**
- R-14: FR-13 completion — @mention autocomplete off `appview_profiles`, mood prefix, location chip, grapheme-accurate 300 counter (`Intl.Segmenter`), parallel media uploads with progress; Mastodon toggle and visibility select removed (FR-14 Bluesky-only; FR-16 Phase 2).
- R-15: Read-after-write posting — created post prepended locally on modal close; `bytescape:refreshFeed` no longer fired for own-post creation (6.8 semantics).

**Chrome & system**
- R-16: NotificationBell — inline styles to CSS, polling paused on `document.hidden`, notifications route to their post/profile.
- R-17: Responsive bug sweep — `.tab-bar-left` 280px scoped to desktop only, contradictory `.mobile-sidebar-toggle` rules removed, bottom-nav safe-area padding, drawer body-scroll lock (2.3 #13).
- R-18: Presence honesty — fake `online/idle` dots deleted; nothing simulates a state the backend doesn't have (2.3 #9).

**Accessibility floor (EXPERIENCE.md, enforced this time)**
- R-19: Full keyboard path: sidebar items Tab+Enter toggle, `✕` reachable, popup focus-trapped with Cancel default, chips Backspace-removable; WCAG AA contrast — platform badge hues lifted or border+icon-treated where they fail 4.5:1 at 11px; `aria-live` announcements for filter changes and toast events.

**Aesthetic guardrails (keep, do not flatten)**
- Green-phosphor terminal palette (`#0a0a0a` / `#00ff41`, `--font-retro` VT323 headers, `--font-mono` Share Tech Mono body), `>` prompt CTAs, flat 1px-bordered panels with shadow reserved for drawer/popups, emoji iconography, pill chips, retro sign-offs (`> LOAD MORE POSTS`, `> END OF FEED`). The feed is a control room, not a card cascade — density and legibility over whitespace theater.

---

## 9. Open Questions

Eight items need user input. OQ-2 is the gating one — it shapes the sidebar's fourth category, the PostCard Lemmy variant, and the FollowingScreen salvage.

| OQ | Question | Context / recommendation |
|---|---|---|
| **OQ-1** | **Does the Popular sort survive v2?** | Spec'd as opt-in (default New, 6.7) — a user-chosen rank by `like_count + repost_count`, not an algorithm. If the no-engagement-ranking philosophy should be absolute, remove the toggle entirely; the UI loses one control and the backend loses one query path. **Rec: keep, non-default** — choice is the product's point |
| **OQ-2** | **What are Subscriptions in v2?** | v1: Lemmy/PieFed communities (community search, `c/…` post rendering, instance links). v2's FR-10 lists the category without a platform. Options: (a) keep Lemmy/fediverse communities, (b) AT Protocol lists (`app.bsky.graph.list`), (c) saved feed-generator subscriptions, (d) some mix. **Gates:** the PostCard Lemmy variant, FollowingScreen salvage, community search endpoint |
| **OQ-3** | **Hashtag matching semantics in the local index** | `appview_posts` stores text; AT posts carry `#tag` facets. Match on facets only (precise, misses untagged usage), text-search fallback (noisy, multilingual issues), or facets-first with text fallback? Case folding and language scope need a call. **Rec: facets-first + text fallback**, case-insensitive |
| **OQ-4** | **Reaction record mapping** | 6.4 proposes ❤️ = `app.bsky.feed.like` (federated) + 8 local emojis on a new `com.bytescape.reaction` lexicon (PDS records, indexed like everything else). Confirm the lexicon shape and that non-❤️ reactions are explicitly a non-federating Bytescape feature |
| **OQ-5** | **Cross-post toggle semantics for `did:web` users** | A Bytescape-hosted user's post is already an AT record on the relay — Bluesky apps can see it without any toggle. Does the FR-14 switch become a no-op (hide it for `did:web`, show only for linked-account mirroring), or does it control an explicit bridge/announcement? Determines whether the composer shows the switch at all for most users |
| **OQ-6** | **Feed generator discovery** | How do users find Feeds? v1 had `searchFeeds` + a cached popular-generators list. v2 needs: search over what — a curated registry we maintain, generators already subscribed by users, or live network search? Plus: who validates a generator before it's one click from everyone's feed. **Rec: curated registry + search, starter feeds in onboarding** |
| **OQ-7** | **Zero-mutuals seed behavior** | New user with follows but no mutuals yet: seed returns empty and the full merge paints first (5.2, spec'd). Alternative: widen the seed to all follows so first paint is still people-first. Mutuals-first purists vs. warm-first-impression — user's call |
| **OQ-8** | **Content-label policy** | Which `labels` values blur by default (adult, graphic-media, …), is there a settings surface in v2.0 or a hardcoded blur+veil, and how are unlabeled-but-reported posts handled before moderation tooling exists. Spec assumes blur+click-through, per-post per-session reveal, no settings UI in v2.0 |

**Resolved by this spec (not questions):** default sort is New (D1 restored); four source categories at launch with RSS deferred to Phase 2 (FR-8); imported posts never in the feed (D7, architecture-enforced); visibility controls deferred with FR-16; Mastodon cross-posting dropped (FR-14 is Bluesky-only); the v1 blue/cyan draft palette is superseded by the shipped green-phosphor system (2.1).

# Frontend Agent — Sprint: USER FEEDBACK BLITZ (2026-03-15)

## 🚨 CRITICAL CONTEXT — THE USER IS UNHAPPY

The user gave 16 feedback items. After a full sprint, only 3 were completed (UF.2, UF.4, UF.8). The user says "literally none of my feedback has been listened to." They are correct — 81% of their requests were not addressed.

**This sprint is 100% user feedback. No roadmap items. No new features. Only fixing what the user asked for.**

All work is on **`main`**. Verify with `git branch --show-current` before every commit.

---

## Priority 1 — UF.1: Full-Width + Maximized Window ⚡

**User:** *"Default to full screen upon opening, full width used by the side bars and center content row...they should also adapt to window size if I happen to resize things."*

**What's wrong now:** Window launches at a fixed size. `--content-max-width: 960px` in App.css. Content area is cramped.

### Fix:
1. **`bytescape/src-tauri/tauri.conf.json`** — Add `"maximized": true` to the window config object.
2. **`bytescape/src/App.css`** — Change `--content-max-width: 960px` to `--content-max-width: 100%` or remove it entirely. The feed content area, sidebars, and panels should fill the full viewport width using flexbox/grid proportional sizing. No fixed pixel caps.
3. **Test at multiple sizes** — Resize window and verify everything scales. No dead space. Sidebars and center column should be fluid.

---

## Priority 2 — UF.12: Import Screen + Audit Screen UX ⚡

**User:** *"I hate the import screen. Everything is crushed off to the upper left. 'Shred surveillance & save memories' has to go. Change 'Exorcism Complete' to 'Import Complete'. Be more specific about trackers."*

### What's still broken:
- `AuditScreen.tsx` line 112: Still says `"🗑️ Shred Surveillance & Save Memories"` — change to `"🗑️ Remove Trackers & Save Memories"` or similar non-scary text
- `AuditScreen.tsx` line 125: Still says `"✅ Exorcism Complete"` — change to `"✅ Import Complete"`
- `AuditScreen.tsx` line ~69-84: Tracker list shows name + event count, but no **category/classification**. Each tracker should show what TYPE of tracking it does (ad tracking, location tracking, cross-site tracking, data collection, etc.)
- Verify the import wizard content is centered in the viewport, not cramped to upper-left

---

## Priority 3 — UF.5: Following/Subscribed Tab Cleanup 🔄

**User:** *"My contacts section on the following view is bad. It's just literally a worse version of the friends tab. Subscribed is just a copy of the All sidebar."*

### What's wrong now:
`FeedScreen.tsx` line 264 renders 4 tabs: `all`, `following`, `subscribed`, `you`.

### Fix:
1. **Remove the "subscribed" tab entirely** — its content is identical to "all"
2. **Restructure the "following" tab** — remove the "My Contacts" section (it duplicates the Social/Friends tab). Instead, show posts only from accounts you follow, without a contacts sidebar clone.
3. Keep `all`, `following`, `you` as the 3 remaining tabs. "All" = discovery feed. "Following" = only followed accounts' posts. "You" = your posts + mentions.

---

## Priority 4 — UF.6: Feed Content-Type Filters (Replace Lenses) 🔄

**User:** *"Gallery view sucks. Let's think about feed filters rather than views. For images, filter just image posts. Same for videos, articles, short form text."*

### What's wrong now:
`FeedScreen.tsx` still has Gallery and Reader "lens" toggles (lines 283-285). The user explicitly rejected this paradigm.

### Fix:
1. **Remove the lens toggle buttons** (Gallery/Reader icons)
2. **Add content-type filter chips** on the feed toolbar: `📝 All · 📸 Images · 🎬 Video · 📰 Articles · 🔗 Links`
3. When a filter is active, only show `BytescapeItem` entries whose `item_type` matches. The Content agent is adding `Link` and `Video` to `BytescapeType` — use those once available. For now, use simple heuristics:
   - Images: items with image attachments
   - Video: items with video attachments
   - Articles: items with `item_type === 'Article'` or body text >500 chars
   - Links: items with external link embeds
4. **Add a source filter dropdown**: Bluesky / Local / Archives / All — the user asked for this too
5. Keep the card layout the same for now (don't try to make a grid for images — that's a follow-up)

---

## Priority 5 — UF.11: Settings Consolidation 🔄

**User:** *"Account/connected accounts should be combined. Add search settings, privacy settings, browser customization. Lose the Bluesky Feeds section. Is device sync even relevant?"*

### What's wrong now:
`SettingsScreen.tsx` has: separate Account and Connected Accounts sections, a "Bluesky Feeds" management section (line 235), a "Device Sync (Beta)" section (line 280), and "Clear Vault" (line 338) with no export option.

### Fix:
1. **Merge Account + Connected Accounts** into one "Identity" section — Bytescape identity as the parent account, linked accounts listed below with a "+ Link New Account" button
2. **Remove "Bluesky Feeds" section** — move this into the FeedScreen directly (as a feed management panel)
3. **Remove "Device Sync (Beta)"** or rename to "P2P Connections" — the user's P2P node pairing already covers this
4. **Rename "Clear Vault" to "Delete Account"** — add a confirmation dialog
5. **Add an "Export Data" button** above "Delete Account" (can be a placeholder for now that shows "Coming soon" — Backend will implement the actual export in UF.15)

---

## Priority 6 — UF.9: Profile Page Rework 🔄

**User:** *"Profile posts are entirely too wide. Keep them narrow. Filter by Wall and Posts. Wall is what users have posted to the user's wall (guestbook). Posts is what the user posted on the wider web. Lose the stats. Move networks into a filter box."*

### Fix:
1. In `ProfileScreen.tsx`: Make post cards narrower (max-width ~480px or 2-column grid)
2. Add two sub-tabs: **Wall** (posts by/to the user — guestbook-style) and **Posts** (user's posts across platforms)
3. Remove the stats bar (media count, vault files, storage)
4. Add source/network filter on the Posts tab
5. Add verified badges for linked external accounts

---

## Deferred (Do NOT Start)
- **UF.10** Social module redesign — needs PM design spec, too large for this sprint
- **UF.14** Theme customization (per-color picker, animated backgrounds) — UX design needed
- **UF.15** Export manager backend — Backend agent scope
- **B10.4** Reload/Refresh
- **Navigation unification** (useNavStore → useLocationStore)

---

## Anti-Patterns
- Do NOT modify `ComposeBox.tsx` — Track C owns that
- Do NOT modify Rust connector files (`connectors/*.rs`) — Track A owns those
- Do NOT modify `ROADMAP.md` — PM scope only
- Do NOT start deferred items — report back via progress.md
- Do NOT fabricate item descriptions — ALWAYS verify against ROADMAP.md
- Do NOT merge branches without PM authorization
- **Always run `git branch --show-current` before committing**

## Definition of Done
- [ ] Window launches maximized, no fixed content-width cap
- [ ] AuditScreen: no "Exorcism" or "Shred Surveillance" text, trackers classified
- [ ] Feed has 3 tabs (all/following/you), no "subscribed"
- [ ] Feed has content-type filter chips, no lens toggles
- [ ] Settings: merged identity section, no Bluesky Feeds, no Device Sync
- [ ] Profile: narrow cards, Wall/Posts tabs, no stats bar
- [ ] `pnpm tauri dev` compiles without errors
- [ ] `pnpm test` passes
- [ ] Committed to `main` with semantic messages

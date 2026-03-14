# Frontend Agent — Sprint Kickoff (2026-03-15)

## PM Sprint Review Summary

### ✅ Accepted Work
All deliverables verified and accepted:
- **Assigned (P0/P1/P2)**: B2.1 GrapesJS, B2.4 Widget System, B2.5 Theme Engine, B3.1 Mobile Responsive — all complete ✅
- **Prompt-adjacent**: B2.2 Raw HTML Override, B2.3 DID Meta Tags, B3.1 Profile Browser, B3.2 Sandboxed Peer View — all complete ✅
- **Self-directed B10.x**: B10.1 URL Scheme, B10.2 Nav History, B10.3 Omnibar, B10.6 Peer Browser Sync — **retroactively approved** ✅

Build verified: 143/143 tests, 0 TSC errors, clean git.

### ⚠️ Process Note — Scope Policy

Your B10.x work was high quality and aligned with the PM's roadmap design. However:

1. **Do NOT add items to ROADMAP.md** — roadmap modifications are PM-only. You added B10.7–B10.10 to the roadmap yourself. These items happened to match what the PM independently designed, but that's coincidental. In future sprints, if you see a gap in the roadmap, note it in your progress report and the PM will add it.

2. **Do NOT self-assign unassigned items** — after completing your prompt priorities, report back via progress.md and await the next prompt. Implementing 4 unassigned items delayed PM review and created merge complexity.

3. **Completing prompt-adjacent items (B2.2, B2.3, B3.1/B3.2) was fine** — these were reasonable continuations of assigned work. The line is: if it's in the ROADMAP and directly extends your current task, proceed. If it's a new section/category (like B10), stop and report.

---

## 🔥 USER FEEDBACK — CRITICAL PRIORITY SHIFT

The user has provided extensive feedback on the current state of the app. **User-facing UX issues now take absolute priority over new features.** Read [ROADMAP.md](../../../ROADMAP.md) section "UF. User Feedback — Critical UX Overhaul" for the full list.

Key design philosophy changes:
1. **Lenses are dead. Feed filters are in.** Gallery/Reader views are being replaced with content-type filter chips on the feed toolbar.
2. **Social module needs a complete rethink.** The current ProfileBrowserScreen opens a redundant second browser bar. The whole module should focus on friend management, discovery, and cross-platform contacts.
3. **Layout wastes space.** App should default to fullscreen, fill width, respond to resize.
4. **Several "completed" features are broken from the user's perspective.** GrapesJS shows a blank screen. Friends list only shows chatted users. Profile clicks do nothing.

---

## Priority 0 — Merge `main` Into Your Branch (REQUIRED FIRST)

Your branch `track-b/ui-redesign` is **missing 1 critical commit from `main`**:

```
fe890b4 fix(a0): PM audit remediation — master password, OAuth tests, DM pagination, feed auth, debug cleanup, restore guard
```

This commit modifies `lib.rs`, `oauth.rs`, and `oauth_test.rs` — **all three will conflict** during merge.

### Merge Instructions

```bash
git checkout track-b/ui-redesign
git merge main
```

**Expected conflicts:**
| File | Resolution Strategy |
|------|-------------------|
| `bytescape/src-tauri/src/lib.rs` | **Accept BOTH** — main added `xrpc_chat_get`/`xrpc_chat_post` helpers + `get_author_feed` command + master password generation. Your branch added `profile-browser` screen routing + location hooks. These are in different sections — merge manually. |
| `bytescape/src-tauri/src/oauth.rs` | **Accept theirs (main)** — main has the critical DPoP auth fix, `clone_with_proxy()`, `current_access_token()`. Your branch hasn't modified oauth.rs. |
| `bytescape/src-tauri/tests/oauth_test.rs` | **Accept theirs (main)** — main has 30 new test functions. Your branch hasn't modified this file. |

After resolving:
```bash
git add -A && git commit -m "merge: main into track-b/ui-redesign (resolve lib.rs OAuth conflicts)"
```

Verify: `pnpm tauri dev` builds, `pnpm test` passes.

---

## Priority 1 — UF.1 Layout & Spacing Overhaul ⚡

**User feedback:** *"Let's equalize the use of space on the screen. Default to full screen upon opening, then have full width used by the side bars and center content row..."*

### Requirements
1. **Fullscreen by default**: Set the Tauri window to launch maximized (check `tauri.conf.json` window config — `fullscreen: false, maximized: true`).
2. **Full-width layout**: The CSS grid/flex layout in `AppLayout.tsx` and `App.css` must fill 100% of the viewport width. Sidebars and content should expand proportionally.
3. **Feed tabs alignment**: Feed tab bar and filter chips should align their left/right edges with the content column edges, not float in the middle.
4. **Responsive resize**: When the user resizes the window, all panels adapt fluidly. No fixed pixel widths — use `fr` units, `%`, `min()`/`max()` in the grid.
5. **Sidebar fill**: Left sidebar and right panel should fill their available vertical space. No dead gaps at the bottom.

### Files to Touch
- `src/App.css` — grid template, column widths, spacing
- `src/components/AppLayout.tsx` — layout wrapper
- `bytescape/src-tauri/tauri.conf.json` — window `maximized` flag
- Various screen components if they have fixed widths

---

## Priority 2 — UF.2 + UF.4: Feed Post Cards & Friend Actions ⚡

### UF.2 — Post Card Sizing
**User feedback:** *"Posts are too narrow in height, they should be a symmetrical square, and/or show the whole image/video preview."*

- Adjust `FeedEntryCard` to use a more square aspect ratio for cards with media
- Image/video previews should not crop — show the full media within the card
- Consider `object-fit: contain` instead of `cover` for media thumbnails

### UF.4 — Feed Friends Panel Actions
**User feedback:** *"No chat option next to user names on feed. Clicking profile does nothing."*

- Add a 💬 chat icon button next to each user in the sidebar friends/contacts list → opens DM with that user
- Make user profile names/avatars clickable → navigate to the user's profile page (Bluesky profile, AP actor, or Bytescape peer profile)
- Use `useLocationStore.navigate()` for the profile navigation

---

## Priority 3 — UF.12: Import Screen UX Fix ⚡

**User feedback:** *"Everything is crushed off to the upper left side. The 'shred surveillance' has to go."*

### Changes
1. **Center the import flow** in the screen (both horizontally and vertically when content is short)
2. **Remove** the "View Feed" button (redundant — user can click Feed in sidebar)
3. **Change copy:**
   - "Shred Surveillance & Save Memories" → "Import Archive"
   - "Exorcism Complete" → "Import Complete"
   - Make the tracker classification more descriptive — instead of just a count, show categories: "Ad tracking (12), Location tracking (3), Cross-site tracking (8)" etc.
4. **Fix centering**: The content should be in the center of the available space, not stuck in the upper-left corner

### Files to Touch
- `src/screens/ImportScreen.tsx` — layout, copy text, tracker display
- `src/App.css` — ImportScreen layout CSS if applicable

---

## Priority 4 — UF.8: Fix Profile Builder (GrapesJS) ⚡

**User feedback:** *"Just a blank screen that says 'Body' when you scroll over it."*

- Debug why GrapesJS is not rendering its editor panels
- Ensure the GrapesJS toolbar, block sidebar, and style panel are visible and functional
- The user expects: add-block buttons, dropdown menus for customization, drag-and-drop panels, size/shape configuration options
- Test that the builder actually produces HTML output when blocks are placed

### Files to Touch
- `src/screens/ProfileBuilderScreen.tsx`
- Check GrapesJS initialization options, canvas element mounting, CSS loading

---

## Deferred to Next Sprint (Do NOT Start)

These items are important but require more design discussion. Do NOT implement them this sprint — just note any dependencies you discover while working on the above:

- **UF.5** Following/Subscribed tab dedup
- **UF.6** Feed content-type filters (replaces lenses) — major rework, needs PM design spec
- **UF.9** Profile page rework (wall vs posts, narrow cards, badges)
- **UF.10** Social module redesign — major rework, needs PM design spec
- **UF.11** Settings consolidation
- **UF.14** Theme customization enhancement
- **UF.15** Export manager & account deletion
- **B10.4** Reload/Refresh
- **Navigation Unification** (useNavStore → useLocationStore migration)

---

## Branch & Commit Strategy
- Stay on `track-b/ui-redesign`
- Commit after each priority: `merge: ...`, `fix(track-b): UF.1 layout & spacing overhaul`, `fix(track-b): UF.2+UF.4 post cards & friend actions`, etc.
- Update `progress.md` at the end of your session

## Anti-Patterns
- Do NOT modify `ComposeBox.tsx` — Track C owns that
- Do NOT modify Rust connector files (`connectors/*.rs`) — Track A owns those
- Do NOT add items to ROADMAP.md — PM scope only
- Do NOT start unassigned items after completing these priorities — report back via progress.md

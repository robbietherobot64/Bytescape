# Frontend Agent — Sprint: USER FEEDBACK v3 (2026-03-15)

## 🚨 READ FIRST — Codebase State & Guardrails

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.

### Current Codebase State (as of commit `eef398e`)

These are facts about the current code — not user preferences. Fix or keep based on **user feedback** (in `user-feedback` file), not file state.

- **Lenses**: FeedScreen uses lens toggle buttons (List/Gallery/Reader) via `GalleryLens` and `ReaderLens` components. 
- **Content width**: `--content-max-width: 960px` in App.css.
- **Window**: `tauri.conf.json` does NOT have `"maximized": true`.
- **TypeScript types**: `BytescapeType = 'Status' | 'Gallery' | 'Article'` (3 variants). Rust has 5 (includes Link/Video). No `ReclassifyResult`, no `category` on `TrackerEntry`.
- **Subscribed tab**: FeedScreen renders 4 tabs `['all', 'following', 'subscribed', 'you']`, BUT `useUnifiedFeed.ts` only defines `FeedTab = 'all' | 'following' | 'you'` — **this is a type mismatch** that compiles only because of `as FeedTab[]` cast. The subscribed tab currently has no handler and shows nothing.
- **ReaderLens broken**: `item_type` is hardcoded to `Some(BytescapeType::Status)` for all Bluesky posts in `feed_aggregation.rs`. ReaderLens filters on `item_type === 'Article'` → shows nothing. The `classify_feed_entry` function exists in `models/mod.rs` but is not called.
- **Backend agent's `store/index.ts`**: Still exists at `src/store/index.ts` with `useFeedCacheStore`, imported in `useUnifiedFeed.ts`.

### What user feedback actually says:
- *"Subscribed is now deleted for some reason"* → User noticed it was missing, wants it with a distinct purpose
- *"Use of space is still awkward"* → User wants better layout, proportional sidebars
- *"Article reader is busted"* → Articles don't load, need classify restored
- *"Posts are too narrow in height"* → Want natural aspect ratio images
- *"Customize should be a pencil button"* → Edit mode with drag-and-drop

All work on **`main`**. Run `git branch --show-current` before every commit.

Read the user's latest feedback at `.github/agents/prompts/user-feedback` before starting.

---

## Priority 1 — Article Reader Broken (CRITICAL BUG) ⚡⚡⚡

**User:** *"Article reader view is somehow even more busted than it was. It doesn't load any content whatsoever, when I know I should have posts with articles linked that should show up in this feed."*

**Root cause:** In `feed_aggregation.rs`, `item_type` was hardcoded to `Some(BytescapeType::Status)` for all Bluesky posts. The `classify_feed_entry` function still exists in `models/mod.rs` but is no longer called. The ReaderLens filters on `e.item_type === 'Article'` (with fallback to content length ≥300 or link_url).

### Fix:
1. In `src-tauri/src/commands/feed_aggregation.rs`, restore the `classify_feed_entry` call:
   - Change `item_type: Some(BytescapeType::Status)` back to `item_type: None`
   - After building the FeedEntry, call: `entry.item_type = Some(crate::models::classify_feed_entry(&entry));`
2. The Rust BytescapeType enum has 5 variants (including Link/Video) but TypeScript only has 3. This is fine — the lenses use client-side heuristics as fallback. Any Link/Video items from Rust will appear as unrecognized types and fall through to the list view.
3. Verify the ReaderLens shows articles after the fix.

---

## Priority 2 — Post Height / Aspect Ratio

**User:** *"Posts are too narrow in height, they should be a symmetrical square, and/or show the whole image/video preview."*

### Fix:
- In the feed card CSS, ensure image/video previews render at their natural aspect ratio (not cropped/clipped to a fixed height)
- For images: show the full image. If very tall, cap at a reasonable max-height with "click to expand"
- Posts with only text should have adequate padding

---

## Priority 3 — Profile Click Navigation

**User:** *"Clicking profile does nothing for users in your friends list, should take you to the user's profile."*

### Fix:
- Make user avatars/names clickable in friends/contacts lists
- Navigate to the user's profile URL (e.g., `https://bsky.app/profile/{handle}` for Bluesky users)

---

## Priority 4 — Following Tab "My Contacts" Section

**User:** *"My contacts section on the following view of the feed is bad. It's just literally a worse version of the friends tab."*

### Fix:
- Remove or replace the "My Contacts" section from the Following tab context panel
- Replace with something useful (e.g., "Recently active" list) or eliminate entirely

---

## Priority 5 — Subscribed Tab Purpose

**User:** *"Subscribed is now deleted for some reason, did we not come up with a better idea?"*

The user re-added `subscribed` to the tabs array. They want it with a DISTINCT purpose.

### Fix:
- Subscribed tab shows ONLY content from **custom feeds and followed hashtags**
- All tab shows everything (timeline + subscriptions + vault)
- This gives Subscribed a "curated content" role vs All's "firehose"

---

## Priority 6 — Screen Space Utilization

**User:** *"Use of space is still awkward. Feed is massively wide. Needs a limit that makes sense. Sidebars need to fill the space."*

### Fix:
- Keep `--content-max-width: 960px` (user set this intentionally)
- Make sidebars expand to fill remaining viewport width proportionally
- Layout: `[expanding sidebar] [960px max content] [expanding sidebar]`
- Responsive: when window narrows, sidebars collapse first

---

## Priority 7 — Customize / Profile Edit Mode

**User:** *"Customize should be a pencil button that says 'Customize' under it. Clicking it makes profile components turn into editable modules you can drag and drop."*

### Fix:
- Pencil icon + "Customize" label replaces current button
- Edit mode: sections become draggable, resizable, editable modules
- Save/Cancel buttons in edit mode
- Leverage any existing GrapesJS or drag-drop infrastructure

---

## Priority 8 — Profile Layout: Tabs + Bio Sections

**User:** *"Profile tabs are way off to the left. Empty spots on the profile. Turn it into a robust social media bio."*

### Fix:
- Align profile tabs to start at the left edge of the feed content area
- Fill empty sections with: "About me:", "Connections:", social links
- Editable via Customize mode (P7)

---

## Priority 9 — Import Screen Redesign (3-Terminal Layout)

**User:** *"Scary verbiage about permanently discarding. Rethink: 3 terminals next to each other for import/memories/trackers, each populating data in an ASCII art dashboard."*

### Fix:
1. **Verbiage**: Remove "permanently discard", "shred", "exorcism". Use: "import", "protect", "liberate"
2. **Layout**: 3 equal-width terminal panels side by side:
   - Terminal 1: **Import Progress** — file counts, steps
   - Terminal 2: **Memories Found** — posts, photos, videos
   - Terminal 3: **Trackers Detected** — tracker names, event counts
3. Monospace/terminal aesthetic: dark bg, green text, ASCII borders
4. Import button: calm language ("📦 Import & Protect")

---

## Priority 10 — Social Module (First Pass)

**User:** *"Browse profiles opens a custom browser bar. The social module is the weakest. Needs focus on managing/discovering friends."*

### Fix:
- Remove the secondary browser bar in Social
- Show cross-platform followers/following in one unified view
- Use icon-based navigation pattern

---

## Priority 11 — Connected Accounts Plus Button

**User:** *"Put a big green plus sign under the latest linked account that says 'Connect account.'"*

### Fix:
- Green "+" button below last connected account in Settings
- On click: modal with platform selection → auth/import flow

---

## Priority 12 — P2P Connections Cleanup

**User:** *"Is P2P Connections even relevant still?"*

### Fix:
- Evaluate if it's redundant with peer connection workflow
- If redundant, remove or simplify

---

## Priority 13 — Hacker Theme 🖥️

**User:** *"Please make a hacker off black/green aesthetic theme, with chunky bit style font and console loading screen."*

### Fix:
- New theme preset: `[data-theme="hacker"]`
- Colors: off-black (#0a0a0a), bright green (#00ff41), dim green (#0d4f0d)
- Monospace pixel-style font
- Boot loading screen with console typewriter animation

---

## Anti-Patterns
- Do NOT modify files outside Track B scope (see `agent-guardrails.instructions.md` §3)
- Do NOT add types to Rust enums — Backend scope
- Do NOT modify Rust connectors — Backend scope
- Do NOT modify ComposeBox — Content scope
- Check `git log --oneline -5 -- <file>` before editing shared files (`types/index.ts`, `useUnifiedFeed.ts`)
- **Always run `git branch --show-current` before committing**

## Definition of Done
- [ ] Article reader shows articles (classify_feed_entry restored)
- [ ] Post images/videos show at natural aspect ratio
- [ ] Profile clicks navigate to user profile
- [ ] Following tab: no duplicate "My Contacts" section
- [ ] Subscribed tab: shows only custom feeds/hashtags
- [ ] Import screen: 3-terminal layout, calm verbiage
- [ ] Hacker theme available
- [ ] `pnpm tauri dev` compiles
- [ ] `pnpm test` passes
- [ ] Committed to `main`

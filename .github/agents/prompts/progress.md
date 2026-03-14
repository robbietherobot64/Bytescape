# Frontend Agent (Track B) — Progress Log

## Session: 2026-03-13 → 2026-03-14
**Branch:** `track-b/ui-redesign`

### Completed This Session

#### UI Redesign — Layout Restructure (`65d70e5`)
- Removed `ContextPanel` sidebar entirely — feed management tools no longer live in a separate right panel
- Moved `FeedRightPanel` (discovery tools, subscriptions, contacts) to the **left side** of the feed, rendered inline before the main feed column
- Made `FriendsPanel` always visible on the **right side** of the feed screen with status-aware away messages
- Away messages are duration-aware: "Just left", "Away 5m", "Away 3h", "Away 2d", "Offline"
- Color-coded status dots: green (online), orange (idle), red (away), grey (offline)
- Feed lens picker and tab bar moved inline into the feed header
- Removed lifted `activeLens`/`feedActiveTab` state from App.tsx — FeedScreen manages its own lens state internally
- **Files changed:** `AppLayout.tsx`, `FeedScreen.tsx`, `FeedRightPanel.tsx`, `FriendsPanel.tsx`, `App.css`, `App.tsx`

#### Unified FriendsPanel — Bluesky + P2P (`aed605e`)
- FriendsPanel now fetches **both** `list_peers` (P2P) and `list_follows` (Bluesky) data sources
- Three grouped sections: **Online** (P2P with green dot), **Away** (P2P with duration), **Bluesky** (follows with blue dot)
- Source filter tabs at top: All / 🔗 P2P / 🦋 Bsky
- Bluesky follows display avatar images and `@handle` as status text
- P2P peers with a matching DID are deduplicated from the Bluesky section
- Bluesky follows refresh every 5 minutes; P2P peers refresh every 60 seconds
- Falls back to `list_contacts` table if live `list_follows` API call fails
- Source-specific CSS: blue avatar border for Bluesky, green for P2P
- **Files changed:** `FriendsPanel.tsx`, `App.css`

#### FriendsPanel Scoping (`507ce75`, `7766512`)
- Initially hid FriendsPanel on screens with their own buddy list (Chat, Friends)
- Refined per user feedback: FriendsPanel now renders **only on the Feed screen**
- Uses the existing `screen` prop in `AppLayout` to conditionally render
- **File changed:** `AppLayout.tsx`

---

### Cumulative Track B Delivery Status

| ID | Item | Status |
|----|------|--------|
| **B1.1** | Lens: Gallery | ✅ Complete (on `main`) |
| **B1.2** | Lens: Reader | ✅ Complete (on `main`) |
| **B1.3** | FeedItemRenderer | ✅ Complete (on `main`) |
| **B2.1** | GrapesJS Profile Builder | ⬜ Not started |
| **B2.2** | Raw HTML Override | ⬜ Not started |
| **B2.3** | DID Meta Tags | ⬜ Not started |
| **B2.4** | Widget System | ⬜ Not started |
| **B2.5** | Theme Engine | ✅ Complete (on `main`) |
| **B2.6** | Dual-Format Serving | 🔒 Blocked by A1.1 |
| **B2.7** | "Memory" Post Type | ⬜ Low priority |

### UI Redesign Delivery (this branch)

| Item | Status | Commit |
|------|--------|--------|
| Top bar browser-style navigation | ✅ | `737105a` |
| Context panel per-screen | ✅ | `72d0461` |
| Feed tools moved to left panel | ✅ | `65d70e5` |
| Friends panel with away messages | ✅ | `65d70e5` |
| Bluesky follows in friends list | ✅ | `aed605e` |
| Source distinction (P2P vs Bluesky) | ✅ | `aed605e` |
| Friends panel scoped to feed only | ✅ | `7766512` |
| Chat buddy list + friends panel unification | ✅ | `ae4bf4b` |

### Cross-Track Fixes Delivered on This Branch

| Fix | Commit |
|-----|--------|
| OAuth DPoP auth headers for feed + DMs | `22a0435` |
| User's Bluesky posts in "You" tab | `a870548` |
| Chat proxy header leak prevention | `c29e549` |
| Debug println cleanup | `97c6faa` |
| Parallel chat import with feed import | `ad6af24` |
| Blank buddy handles from convo members | `bf56e20` |

### Open Items / Notes for PM

1. **`ContextPanel.tsx` cleanup**: File still exists but is no longer imported or rendered anywhere. Safe to delete.
2. **B2.1 GrapesJS**: Next priority per the kickoff prompt. Fully independent, no cross-track blockers.
3. **Bluesky follows capped at 100**: The `list_follows` backend command has a hardcoded limit of 100. For users following more, pagination support would need a backend change (Track A scope).
4. **Contact sync never invoked**: `ContactManager::sync_contacts()` in `contacts.rs` exists but is never called from any Tauri command. It could auto-populate the `CONTACTS_TABLE` and enable richer contact data without live API calls. Flagging for Track A consideration.

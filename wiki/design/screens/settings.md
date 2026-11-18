# Screen Spec: Settings (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/settings`
**Satisfies:** FR-26 (Mute Lists), FR-25 (Notifications — preference surface), FR-9 (Customizable Profiles — theme preferences), FR-12 (Media Screen — media display preferences). Contributes to AD-01b (dual PDS — settings storage model), AD-01c (lexicons — settings don't touch custom lexicons, app-DB only).

**Sources:**
- v1 implementation: none — no v1 settings screen exists. Related v1 code: `apps/web/src/components/ProfileEditForm.tsx` (theme selector, 185 lines), `src/components/PostCard.tsx` (mute-from-card action, 1276 lines), `src/components/NotificationBell.tsx` (notification polling, 199 lines), `src/styles.css` (theme classes)
- v2 constraints: `wiki/product/prd.md` (FR-26, FR-25, FR-9, FR-12), `wiki/architecture/database.md` (users, muted_accounts, notifications tables), `wiki/architecture/decisions.md` (AD-01b dual PDS, AD-01c lexicons)

---

## 1. Overview

The settings screen is the authenticated user's control panel for every Bytescape preference, safety tool, and data action that doesn't belong on the profile itself. It is a **new v2 screen** — v1 had no dedicated settings surface. Theme selection lived in the profile edit modal (ProfileEditForm), mute was post-card-only (PostCard context menu), and notification preferences didn't exist (NotificationBell polled unconditionally).

The screen serves one audience — the signed-in account owner — and every action writes to the **application database**, never to PDS records. This is a key architectural point: settings are Bytescape-specific preferences that have no AT Protocol equivalent and no need to replicate across PDS instances.

### 1.1 Dual PDS implications (AD-01b)

Settings are identical for both user classes because they live entirely in the app DB:

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| Theme preference | App DB: `users.theme`, `users.theme_app_wide` | Same |
| Notification prefs | App DB: new `notification_preferences` JSONB column on `users` | Same |
| Mute list | App DB: `muted_accounts` table | Same |
| Blocked accounts | App DB: `muted_accounts` table (blocks = mutes with extra enforcement) | Same |
| Accessibility prefs | App DB: new `accessibility_prefs` JSONB column on `users` | Same |
| Custom CSS | App DB: `users.custom_css` | Same |

Frontend consequence: the settings screen has **zero PDS write paths**. Every save is a `PATCH /api/settings` (or section-specific endpoints) against the app DB. No XRPC calls, no OAuth scope requirements beyond basic auth. This makes the screen simple to build and impossible to break the PDS relationship.

### 1.2 Data sources (v2)

| Data | Location | Notes |
|---|---|---|
| Theme selection | App DB: `users.theme` | Values: `terminal` (default), `1999`, `xp-aero`, `modern` |
| App-wide theme toggle | App DB: `users.theme_app_wide` | Boolean; default `true` (profile spec decision 2026-08-09, round 3) |
| Custom CSS | App DB: `users.custom_css` | Sanitized/allowlisted subset; editor ships in v2.0 (option-B re-draw, profile spec) |
| Notification preferences | App DB: `users.notification_prefs` JSONB (new) | Per-type toggles: likes, reposts, follows, replies, mentions, wall_posts |
| Muted accounts | App DB: `muted_accounts` table | `user_id`, `muted_did`, `created_at` — already in schema |
| Blocked accounts | App DB: `muted_accounts` with `blocked: true` flag (extended schema) | Blocks enforce stronger: no profile views, no DMs, no mentions |
| Accessibility prefs | App DB: `users.accessibility_prefs` JSONB (new) | `reduced_motion`, `color_blind_mode` (deferred), `font_size` |
| Account data | App DB: `users` + derived tables | For data export (FR-26 scope) |
| User profile | `appview_profiles` + `users` | Display name, handle, avatar shown in settings header |

### 1.3 Settings categories

| # | Category | FR coverage | Key actions |
|---|---|---|---|
| 1 | **Appearance** | FR-9 (theme prefs) | Theme selection, app-wide toggle, custom CSS editor |
| 2 | **Notifications** | FR-25 (notification prefs) | Per-type enable/disable toggles |
| 3 | **Privacy & Safety** | FR-26 (mute lists) | Mute list management, blocked accounts list |
| 4 | **Accessibility** | — (cross-cutting) | Reduced motion, font size, color-blind mode (post-launch) |
| 5 | **Data & Privacy** | — (cross-cutting) | Data export, account deletion |

### 1.4 What changed vs. v1 at a glance

1. **New screen entirely.** v1 had no `/settings` route — preferences were scattered across profile edit, post-card menus, and the notification bell.
2. **Theme selection centralized.** Moves from the profile edit modal (ProfileEditForm) to Appearance settings; the profile modal retains the theme swatch for quick access, but the authoritative surface is here.
3. **Mute list management added.** v1 mutes were post-card-only with no way to view or manage muted accounts. FR-26 requires a dedicated mute list with search, add, view, and unmute.
4. **Notification preferences added.** v1 NotificationBell polled unconditionally. v2 adds per-type toggles so users control what arrives.
5. **Accessibility preferences added.** `prefers-reduced-motion` was the only v1 consideration (CSS media query). v2 adds explicit font-size control and reserves a color-blind mode slot.
6. **Data export added.** No v1 equivalent. Users can download their data.
7. **Account deletion added.** No v1 equivalent. Destructive action with multi-step confirmation.

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is or with trivial touch-ups), **REFINE** (keep concept and most code, improve), **REWORK** (keep concept, rebuild internals), **DROP** (obsolete; mine for parts), **DEFER** (concept preserved, design postponed).

Summary: **0 direct salvage** — no v1 settings screen exists. Related v1 code contributes patterns and partial implementations that are referenced but not imported.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `ProfileEditForm.tsx` (185) | **REFINE** | Theme swatch selector (Terminal/1999/XP) is the starting point for Appearance settings; the field form portion stays in the profile spec's identity modal, not here |
| 2 | `PostCard.tsx` (1276) | **KEEP** (reference) | Mute-from-card action (⋯ → Mute @handle) becomes a sibling entry point that feeds the same `muted_accounts` table; the settings mute list is the management surface, not a replacement |
| 3 | `NotificationBell.tsx` (199) | **KEEP** (reference) | Polling logic stays; the bell respects the per-type prefs set here. Bell dropdown links to notification settings ("⚙ Manage notifications") |
| 4 | `styles.css` theme classes | **KEEP** (reference) | `.theme-terminal`, `.theme-1999`, `.theme-xp` class definitions are the CSS foundation; Appearance settings adds `.theme-modern` |

### 2.1 v1 patterns carried forward (not file-level salvage)

| Pattern | Source | How it applies to settings |
|---|---|---|
| Theme swatch selector | `ProfileEditForm.tsx` | Appearance section: 4 theme cards (terminal, 1999, xp-aero, modern) with live preview |
| Mute action via post card | `PostCard.tsx` ⋯ menu | Settings mute list is the management surface; post-card mute remains the quick-action entry point |
| Toast notifications | Event bus `bytescape:toast` | Save confirmations, mute/unmute feedback, export ready notification |
| Modal confirmation pattern | `DeleteConfirmationPopup` (feed spec) | Account deletion uses the same pattern: centered modal, big red DELETE, Cancel default focus |
| Theme CSS variables | `styles.css` | Settings preview panel applies theme classes to show a live sample |

### 2.2 No v1 defects to fix

This is a new screen. The only "defects" are **v1 absences** — features that should have existed but didn't:

1. **No mute list management** — users could mute from a post card but never see or unmute muted accounts. Fixed by Privacy & Safety section.
2. **No notification preferences** — NotificationBell polled for everything or nothing. Fixed by Notifications section.
3. **No centralized theme selection** — theme lived only in the profile edit modal. Fixed by Appearance section.
4. **No accessibility controls** — only `prefers-reduced-motion` CSS media query. Fixed by Accessibility section.
5. **No data export or deletion** — no user-facing data controls. Fixed by Data & Privacy section.

---

## 3. Layout

The settings screen uses a **sidebar-nav + content-area** pattern on desktop, switching to **horizontal tabs** on tablet and **accordion sections** on mobile.

### 3.1 Desktop (>900px) — sidebar nav + content

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR                        [📰 Feed] [👤 @you] [Settings]    [🔔] [☰]   │
├──────────────────┬───────────────────────────────────────────────────────────┤
│ SETTINGS NAV     │  SETTINGS CONTENT                                         │
│ 240px, fixed     │  1fr · max-width 720px, centered                          │
│                  │                                                           │
│ ┌──────────────┐ │  APPEARANCE                                               │
│ │ [av] @handle │ │  ┌──────────────────────────────────────────────────────┐ │
│ │ Display Name │ │  │ Theme                                                  │ │
│ └──────────────┘ │  │ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐        │ │
│                  │  │ │Terminal│ │ 1999   │ │XP Aero │ │ Modern │        │ │
│ ▸ 🎨 Appearance  │  │ │  [✓]   │ │        │ │        │ │        │        │ │
│   Notifications  │  │ └────────┘ └────────┘ └────────┘ └────────┘        │ │
│   Privacy        │  │                                                      │ │
│   Accessibility  │  │ ☑ Apply theme across the app                         │ │
│   Data           │  │                                                      │ │
│                  │  │ Custom CSS                                           │ │
│                  │  │ ┌──────────────────────────────────────────────────┐ │ │
│                  │  │ │ /* your custom CSS */                            │ │ │
│                  │  │ │ .profile-header {                                │ │ │
│                  │  │ │   background: #1a1a2e;                           │ │ │
│                  │  │ │ }                                                │ │ │
│                  │  │ └──────────────────────────────────────────────────┘ │ │
│                  │  │                              [ > SAVE APPEARANCE ]   │ │
│                  │  └──────────────────────────────────────────────────────┘ │
│                  │                                                           │
│                  │  ────────────────────────────────────────────────────────  │
│                  │                                                           │
│                  │  NOTIFICATIONS                                            │
│                  │  ┌──────────────────────────────────────────────────────┐ │
│                  │  │ Notify me when:                                      │ │
│                  │  │                                                      │ │
│                  │  │ Someone likes my post            [toggle ●──]       │ │
│                  │  │ Someone reposts my post          [toggle ●──]       │ │
│                  │  │ Someone follows me               [toggle ──●]       │ │
│                  │  │ Someone replies to my post       [toggle ──●]       │ │
│                  │  │ Someone mentions me              [toggle ──●]       │ │
│                  │  │ Someone posts on my wall         [toggle ──●]       │ │
│                  │  └──────────────────────────────────────────────────────┘ │
└──────────────────┴───────────────────────────────────────────────────────────┘
 grid: 240px · 1fr — sidebar fixed, content scrolls
 sidebar: 1px right border (--border-dim), flat panel
 content: padding 32px, max-width 720px centered within 1fr
```

### 3.2 Tablet (551–900px) — horizontal tabs + content

The sidebar collapses into a horizontal tab bar above the content area. Tabs are horizontally scrollable on overflow.

```
┌──────────────────────────────────────────────────────────────┐
│ TAB BAR                  [📰 Feed] [👤 @you] [⚙]   [🔔] [☰] │
├──────────────────────────────────────────────────────────────┤
│ [Appearance] [Notifications] [Privacy] [Accessibility] [Data]│  ← tabs
├──────────────────────────────────────────────────────────────┤
│ APPEARANCE                                                   │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ Theme                                                    │ │
│ │ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐             │ │
│ │ │Terminal│ │ 1999   │ │XP Aero │ │ Modern │             │ │
│ │ │  [✓]   │ │        │ │        │ │        │             │ │
│ │ └────────┘ └────────┘ └────────┘ └────────┘             │ │
│ │ ☑ Apply theme across the app                             │ │
│ │                                                          │ │
│ │ Custom CSS                                               │ │
│ │ ┌──────────────────────────────────────────────────────┐ │ │
│ │ │ /* your custom CSS */                                │ │ │
│ │ └──────────────────────────────────────────────────────┘ │ │
│ │                                    [ > SAVE APPEARANCE ] │ │
│ └──────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
 tabs: horizontal scroll, pill-style, 1px bottom border active indicator
 content: padding 24px, same max-width 720px
```

### 3.3 Mobile (≤550px) — accordion sections

All categories are stacked as collapsible accordion sections. The currently-active section from the desktop tab state maps to the expanded accordion panel.

```
┌──────────────────────────────┐
│ TAB BAR          [⚙] [🔔]   │
├──────────────────────────────┤
│ ▼ 🎨 Appearance              │  ← expanded
│ ┌──────────────────────────┐ │
│ │ Theme                    │ │
│ │ ┌──────┐ ┌──────┐       │ │
│ │ │Termi-│ │ 1999 │       │ │
│ │ │nal ✓ │ │      │       │ │
│ │ └──────┘ └──────┘       │ │
│ │ ┌──────┐ ┌──────┐       │ │
│ │ │XP    │ │Modern│       │ │
│ │ │Aero  │ │      │       │ │
│ │ └──────┘ └──────┘       │ │
│ │ ☑ App-wide theme        │ │
│ │                          │ │
│ │ Custom CSS               │ │
│ │ ┌──────────────────────┐ │ │
│ │ │ /* custom css */     │ │ │
│ │ └──────────────────────┘ │ │
│ │          [> SAVE]        │ │
│ └──────────────────────────┘ │
│ ▶ Notifications              │  ← collapsed
│ ▶ Privacy & Safety           │
│ ▶ Accessibility              │
│ ▶ Data & Privacy             │
└──────────────────────────────┘
 accordion: only one section open at a time
 expanded section: 200ms max-height transition
 content: padding 16px, no max-width constraint
```

---

## 4. Component Breakdown

### 4.1 Screen shell

**`SettingsScreen` (NEW)**
- Purpose: resolve the authenticated user, hold the active settings category, route between sections, coordinate save operations.
- Props: viewer context from auth (required — unauthenticated redirects to login).
- State: `activeSection: 'appearance' | 'notifications' | 'privacy' | 'accessibility' | 'data'`, `settings: UserSettings` (loaded once, cached), `saveStatus: 'idle' | 'saving' | 'saved' | 'error'`, `error: string | null`.
- Loads via `GET /api/settings` on mount — returns the full settings blob (theme, notification prefs, muted accounts, accessibility prefs). One request, all sections populated.
- Layout: sidebar nav (desktop) / tabs (tablet) / accordion (mobile) + content area. Responsive breakpoint matches the app grid: >900px desktop, 551–900px tablet, ≤550px mobile.
- Stack: React 19 + Vite + TypeScript, useState/useEffect, event bus (`bytescape:toast` for save feedback). No new state library — consistent with the 2026-08-09 stack decision.

**Subcomponents:**

```
SettingsScreen
├── SettingsNav          (sidebar/tabs/accordion header)
├── AppearanceSettings   (theme + app-wide toggle + custom CSS)
├── NotificationSettings (per-type toggles)
├── PrivacySafetySettings
│   ├── MuteListManager  (search + add + view + unmute)
│   └── BlockedListManager (view + unblock)
├── AccessibilitySettings (reduced motion + font size + color-blind placeholder)
└── DataPrivacySettings
    ├── DataExport       (export trigger)
    └── AccountDeletion  (confirmation modal)
```

### 4.2 Settings nav

**`SettingsNav` (NEW)**
- Purpose: category navigation — sidebar on desktop, tab bar on tablet, accordion headers on mobile.
- Props: `activeSection`, `onSelect(section)`, `variant: 'sidebar' | 'tabs' | 'accordion'`, `expandedSection` (accordion only), `onToggleExpand(section)` (accordion only).
- Desktop sidebar: vertical list of category items with icon + label. Active item: 2px left border `--accent` + accent text (matching profile nav pattern). User handle + avatar at top (same as profile nav header).
- Tablet tabs: horizontal pill bar, scrollable. Active tab: `--accent` bg with dark text.
- Mobile accordion: collapsible headers with chevron. One open at a time. 200ms max-height transition.
- Items:
  - 🎨 Appearance
  - 🔔 Notifications
  - 🛡️ Privacy & Safety
  - ♿ Accessibility
  - 📦 Data & Privacy

### 4.3 Appearance settings

**`AppearanceSettings` (NEW)**
- Purpose: theme selection, app-wide toggle, custom CSS editor.
- Props: `theme`, `themeAppWide`, `customCss`, `onSave(settings)`, `saveStatus`.
- State (local): `selectedTheme`, `appWideToggle`, `cssDraft`, `cssDirty`.

**Subcomponents:**

**`ThemeSelector`** — 4 theme cards in a 2×2 grid (desktop) or 2-column (mobile). Each card: theme preview swatch (miniature color sample), theme name, checkmark on selected. Themes:
- **Terminal** (default): green phosphor, `#0a0a0a` bg, `#00ff41` accent — the Bytescape default
- **1999**: lavender/pink, Win98-inspired pastels
- **XP Aero**: blue/glass, translucent panels
- **Modern**: neutral grays, clean sans-serif (new v2 theme)

Clicking a card: applies theme preview in real-time to the settings content area (live preview within the card and a sample post card below the selector). Selection is staged, not saved until the user clicks Save.

**`AppWideToggle`** — checkbox + label: "Apply theme across the app" (default checked, per profile spec decision 2026-08-09 round 3). When unchecked, the theme applies only to the user's profile page; the rest of the app keeps the default Bytescape theme. Tooltip explains the behavior.

**`CustomCssEditor`** — textarea with monospace font (Share Tech Mono), line numbers, syntax highlighting (basic — green comments, cyan selectors, white properties). Sanitization note below the editor: "CSS is sanitized. Only safe properties are allowed — no `position: fixed`, no `z-index > 9999`, no external URLs." Save button disabled when `!cssDirty`. The custom CSS editor **ships in v2.0** (option-B re-draw, profile spec); this spec defines the component, but the feature flag gates it in the initial release.

**`AppearancePreview` (NEW)** — a miniature post card rendered with the selected theme applied. Shows: avatar, handle, sample text, reaction counts. Purpose: live feedback when switching themes without leaving the page. Updates on every theme card click.

**Save behavior:** One `> SAVE APPEARANCE` button at the bottom. Saves `theme`, `theme_app_wide`, and `custom_css` in a single `PATCH /api/settings/appearance` request. On success: toast "Appearance saved ✓", button text changes to "✓ Saved" for 3 seconds, then reverts. On error: toast "Failed to save — try again", button re-enables.

### 4.4 Notification settings

**`NotificationSettings` (NEW)**
- Purpose: per-type notification toggles.
- Props: `preferences: NotificationPreferences`, `onSave(prefs)`, `saveStatus`.
- State (local): `localPrefs` (optimistic copy of preferences).

**Toggle rows** (6 toggles, each a row):

| Toggle | Default | Description |
|---|---|---|
| Likes | on | "Someone likes my post" |
| Reposts | on | "Someone reposts my post" |
| Follows | on | "Someone follows me" |
| Replies | on | "Someone replies to my post" |
| Mentions | on | "Someone mentions me" |
| Wall posts | on | "Someone posts on my wall" |

Each row: label text (left), toggle switch (right). Toggle uses a custom-styled checkbox with `--accent` active state, 44px minimum hit target (WCAG 2.5.5). Labels are descriptive, not just "Likes" — the sentence format ("Someone likes my post") is more accessible and matches the v2 retro tone.

**Save behavior:** Auto-save on toggle change (debounced 500ms). No explicit Save button — each toggle flip fires `PATCH /api/settings/notifications` after the debounce. Visual feedback: toggle briefly pulses `--accent` on save success. On error: toggle reverts to previous state + toast "Failed to update — try again".

**Why auto-save here but explicit save for Appearance?** Appearance has multiple correlated fields (theme + app-wide + CSS) that should save atomically. Notification prefs are independent toggles — auto-save matches the interaction model (flip and forget, like iOS notification settings).

### 4.5 Privacy & Safety settings

**`PrivacySafetySettings` (NEW)**
- Purpose: mute list management (FR-26), blocked accounts list.
- Props: `mutedAccounts: MutedAccount[]`, `blockedAccounts: BlockedAccount[]`, `onMute(did)`, `onUnmute(did)`, `onUnblock(did)`.
- State (local): `searchQuery`, `searchResults`, `searchLoading`, `activeTab: 'muted' | 'blocked'`.

**Subcomponents:**

**`MuteListManager` (NEW)** — FR-26 primary surface. Three sections:

1. **Search + Add** — input field with `> Search users to mute...` placeholder, 300ms debounce. Searches `appview_profiles` by handle/display name. Results appear below as a list: avatar + handle + display name + `🔇 Mute` button. Clicking Mute: confirmation toast "Muted @{handle} — their posts won't appear in your feed", row moves to the muted list below. Duplicate check: if already muted, button shows "Already muted" (disabled).

2. **Muted accounts list** — paginated list (20 per page) of muted accounts. Each row: avatar + handle + display name + "Muted on {date}" + `🔊 Unmute` button. Unmute: confirmation toast "Unmuted @{handle}", row fades out (200ms). Empty state: illustration of a muted speaker + "No muted accounts — muted users' posts won't appear in your feed."

3. **Mute from post card** — the post-card ⋯ menu "Mute @handle" action (PostCard.tsx, feed spec) is a sibling entry point. Muting from the card adds to the same `muted_accounts` table. The settings list is the management surface; the card action is the quick-action shortcut. Both are FR-26.

**`BlockedListManager` (NEW)** — similar to mute list but for blocked accounts. Blocks are stronger than mutes: blocked users cannot view the blocker's profile, send DMs, or appear in search results for the blocker. Each row: avatar + handle + "Blocked on {date}" + `🔓 Unblock` button. Unblock: two-step confirmation ("Unblock @{handle}? They will be able to see your profile and posts again.") — Cancel default focus, matching the DeleteConfirmationPopup pattern.

**Blocked accounts storage:** Extends `muted_accounts` with a `blocked: boolean` column (or a separate `blocked_accounts` table — backend decision). The frontend treats them as distinct lists with distinct enforcement.

### 4.6 Accessibility settings

**`AccessibilitySettings` (NEW)**
- Purpose: reduced motion, font size, color-blind mode (reserved).
- Props: `prefs: AccessibilityPrefs`, `onSave(prefs)`, `saveStatus`.
- State (local): `localPrefs`.

**Controls:**

| Control | Type | Default | Notes |
|---|---|---|---|
| Reduced motion | Toggle | off (respects `prefers-reduced-motion` OS setting) | When on: disables all CSS transitions, animations, skeleton shimmers, and the grid drag animation. Honors the OS preference as a starting value |
| Font size | Segmented control: Small / Default / Large / Extra Large | Default | Maps to `font-size` on `:root`: Small = 13px, Default = 14px, Large = 16px, Extra Large = 18px. Affects body text, not headers (VT323 headers scale proportionally) |
| Color-blind mode | Toggle (disabled) | off | **Post-launch.** Label: "Color-blind mode — coming soon". Greyed out with tooltip: "Adjusts color contrast and adds pattern cues to status indicators." Reserves the UI slot and the data field |

**Save behavior:** Explicit `> SAVE ACCESSIBILITY` button. Saves all three fields atomically via `PATCH /api/settings/accessibility`. Same save-status pattern as Appearance.

### 4.7 Data & Privacy settings

**`DataPrivacySettings` (NEW)**
- Purpose: data export, account deletion.
- Props: `userId`, `onDelete()`.
- State (local): `exportStatus: 'idle' | 'exporting' | 'ready' | 'error'`, `exportUrl`, `deleteModalOpen`, `deleteConfirmStep`.

**Subcomponents:**

**`DataExport`** — "Download your data" section. Button: `> EXPORT MY DATA`. On click: fires `POST /api/settings/export`, shows spinner "Preparing your data...", on success: "Your export is ready — [Download .zip]" link. Export includes: profile data, posts, media URLs (as links, not blobs), follows, muted accounts, notification prefs, widget layouts. Format: JSON files in a zip. Exports are available for 24 hours then deleted.

**`AccountDeletion`** — "Delete your account" section. Warning text: "This action is permanent. Your posts, profile, and data will be deleted. This cannot be undone." Button: `> DELETE MY ACCOUNT` (red, `--danger`). On click: opens **DeleteConfirmationPopup** (same pattern as feed spec's source delete):
- Step 1: "Are you sure you want to delete your account?" — [Cancel] (default focus) + [🔴 DELETE MY ACCOUNT]
- Step 2 (on confirm): "Type your handle to confirm:" — input field with `@handle` placeholder + [Cancel] + [🔴 PERMANENTLY DELETE]. The handle must match exactly (case-insensitive). Button disabled until handle matches.
- On final confirm: `DELETE /api/account`, redirect to logout + landing page with toast "Account deleted".
- Focus trap in modal. Escape = Cancel. Overlay click = nothing (matches the "forces explicit choice" pattern).

**Deletion scope:** Bytescape-hosted accounts (`did:web`): full deletion — PDS records, app DB rows, media. Bluesky-hosted accounts (`did:plc`): Bytescape data only — AT Protocol records on bsky.social PDS survive (the user controls those separately). The confirmation modal text reflects this distinction: Bluesky-hosted users see "Your Bluesky account and posts will not be affected."

---

## 5. States

### 5.1 Loading

On initial mount, `SettingsScreen` shows skeleton placeholders while `GET /api/settings` resolves:

```
┌──────────────────┬──────────────────────────────────────────┐
│ NAV (skeleton)   │  CONTENT (skeleton)                      │
│                  │                                          │
│ ░░░░░░░░░░░░░░░ │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
│ ░░░░░░░░░░░░░░░ │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
│ ░░░░░░░░░░░░░░░ │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
│ ░░░░░░░░░░░░░░░ │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
│ ░░░░░░░░░░░░░░░ │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
└──────────────────┴──────────────────────────────────────────┘
```

- Skeleton: 5 nav items (shimmer bars), content area: 2–3 section placeholders (shimmer bars matching the section header + toggle row shapes).
- Duration: typically <500ms on a healthy connection. If >3s, show a "Still loading..." hint below the skeletons.
- `prefers-reduced-motion`: skeletons are static (no shimmer), just dim rectangles.

### 5.2 Populated

Normal state — all sections render their controls with current values loaded from the API. The nav shows the first section (Appearance) as active. Each section is independently scrollable within the content area (content scrolls, nav is fixed on desktop).

### 5.3 Saving

Per-section save states:

| Section | Save model | Saving indicator |
|---|---|---|
| Appearance | Explicit button | Button shows spinner + "Saving...", controls dimmed (opacity 0.6) |
| Notifications | Auto-save (debounced) | Toggle pulses `--accent` border briefly, no blocking overlay |
| Privacy & Safety | Per-action (mute/unmute/block/unblock) | Action button shows spinner, row gets `opacity: 0.6` |
| Accessibility | Explicit button | Same as Appearance |
| Data & Privacy | Action-based (export/delete) | Button shows spinner + contextual text ("Preparing export...", "Deleting...") |

### 5.4 Saved confirmation

After a successful save:

| Section | Confirmation |
|---|---|
| Appearance | Button text → "✓ Saved" for 3s, then reverts. Toast: "Appearance saved ✓" |
| Notifications | Toggle pulse (brief green glow). Toast: "Notification preferences updated" (debounced — only fires once per 500ms burst) |
| Privacy & Safety | Toast: "Muted @{handle}" / "Unmuted @{handle}" / "Unblocked @{handle}" |
| Accessibility | Button text → "✓ Saved" for 3s. Toast: "Accessibility preferences saved ✓" |
| Data Export | Inline: "Your export is ready — [Download .zip]" replaces the button |
| Account Deletion | Toast on landing page: "Account deleted" |

All toasts use the `bytescape:toast` event bus (consistent with feed and profile specs).

### 5.5 Error

| Error type | Recovery |
|---|---|
| Load failure (GET /api/settings fails) | Full-page error state: "Failed to load settings" + `> RETRY` button. Same pattern as feed/profile error states |
| Save failure (PATCH fails) | Per-section: toast "Failed to save — try again" + controls re-enable. No data loss — local state retains the unsaved values |
| Mute/unmute failure | Toast "Failed to mute @{handle} — try again". Row reverts to previous state |
| Export failure | Inline: "Export failed — try again" + `> RETRY` button |
| Delete failure | Modal: "Failed to delete account — please try again or contact support." Modal stays open, buttons re-enable |

### 5.6 Edge cases

| Case | Behavior |
|---|---|
| Unauthenticated user visits `/settings` | Redirect to `/login?redirect=/settings` |
| User has no muted accounts | Mute list: empty state with illustration + "No muted accounts" message. Search still functional |
| User tries to mute themselves | Search result shows own handle with disabled button: "You can't mute yourself" |
| Custom CSS is empty | Editor shows empty textarea with placeholder: "/* Add your custom CSS here */" |
| Theme not yet set (new account) | Default: Terminal, app-wide toggle checked. First visit shows Terminal selected |
| Very long mute list (>100 accounts) | Paginated at 20 per page, "Load more" button at bottom |
| Account deletion while export is pending | Deletion modal warns: "A data export is in progress. It will be cancelled if you delete your account." |
| Concurrent settings changes (two tabs) | Last-write-wins. No optimistic locking. Tab visibility API pauses auto-save on hidden tabs to reduce conflicts |

---

## 6. Interactions

### 6.1 Theme selection flow

1. User clicks a theme card → card shows checkmark, other cards lose checkmark.
2. `AppearancePreview` below the selector re-renders with the new theme's CSS variables applied.
3. If `theme_app_wide` is checked, the app chrome (tab bar, sidebar) also reflects the theme change as a preview. The preview reverts on navigation away without saving.
4. User clicks `> SAVE APPEARANCE` → `PATCH /api/settings/appearance` with `{ theme, theme_app_wide, custom_css }`.
5. On success: toast + button confirmation. The saved theme applies immediately to the app (the root `<html>` element's class changes).
6. Theme change is **live** — no page reload required. The `bytescape:themeChanged` event fires, and the app shell updates its CSS class.

### 6.2 Notification toggle flow

1. User flips a toggle → local state updates immediately (optimistic).
2. After 500ms debounce (no more toggles within that window), `PATCH /api/settings/notifications` fires with the full preferences object.
3. On success: brief toggle pulse. On error: toggle reverts, toast.
4. The NotificationBell component listens for a `bytescape:notificationPrefsChanged` event and adjusts its polling behavior (skips polling for disabled types, though the server also filters — belt and suspenders).

### 6.3 Mute user flow (from settings)

1. User types a handle or display name in the search field → 300ms debounce → `GET /api/search/users?q={query}` (AppView search, same as sidebar global search).
2. Results render as a list: avatar + handle + display name + `🔇 Mute` button.
3. User clicks Mute → `POST /api/mute` with `{ did }`.
4. On success: result row shows "✓ Muted", button disables. Toast: "Muted @{handle}".
5. The muted account appears in the muted list below (optimistic insert — the row appears immediately without a refetch).
6. Muting from a PostCard ⋯ menu (feed spec) performs the same API call and fires `bytescape:muteListChanged` so the settings screen stays in sync if open.

### 6.4 Unmute user flow

1. User clicks `🔊 Unmute` on a muted account row.
2. No confirmation modal (unlike unblock) — unmuting is low-risk. The row gets a brief "Unmuted ✓" label, then fades out (200ms).
3. `DELETE /api/mute/{did}` fires. On error: row reverts + toast "Failed to unmute — try again".
4. Fires `bytescape:muteListChanged` for cross-tab sync.

### 6.5 Unblock user flow

1. User clicks `🔓 Unblock` on a blocked account row.
2. Confirmation popup (DeleteConfirmationPopup pattern): "Unblock @{handle}? They will be able to see your profile and posts again." — [Cancel] (default focus) + [🔓 UNBLOCK].
3. On confirm: `DELETE /api/block/{did}`, row fades out, toast "Unblocked @{handle}".
4. Focus returns to the blocked accounts list.

### 6.6 Data export flow

1. User clicks `> EXPORT MY DATA`.
2. Button shows spinner + "Preparing your data..."
3. `POST /api/settings/export` returns `{ exportId, status: 'processing' }`.
4. Screen polls `GET /api/settings/export/{exportId}` every 2s until `status: 'ready'`.
5. On ready: button area shows "Your export is ready — [Download .zip]" link. Link points to a signed URL valid for 24 hours.
6. On error: "Export failed — try again" + `> RETRY` button.

### 6.7 Account deletion flow

1. User clicks `> DELETE MY ACCOUNT` (red button).
2. **Step 1 modal** opens (focus-trapped): "Are you sure you want to delete your account? All your posts, profile data, and settings will be permanently deleted."
   - [Cancel] (auto-focused, Enter = Cancel) + [🔴 DELETE MY ACCOUNT]
   - Escape = Cancel. Overlay click = nothing.
3. On DELETE click: modal transitions to **Step 2**: "Type @{handle} to confirm:" + input field + [Cancel] + [🔴 PERMANENTLY DELETE].
   - PERMANENTLY DELETE button disabled until input matches handle (case-insensitive).
   - Bluesky-hosted users see additional text: "Your Bluesky account and posts on bsky.social will not be affected."
4. On PERMANENTLY DELETE: `DELETE /api/account` fires. Button shows spinner.
5. On success: redirect to `/` (logged out), toast "Account deleted".
6. On error: "Failed to delete account — please try again or contact support." Modal stays open.

### 6.8 Keyboard navigation

| Action | Key |
|---|---|
| Navigate settings sections (sidebar) | ↑/↓ arrows move focus between nav items, Enter/Space selects |
| Navigate settings sections (tabs) | ←/→ arrows move between tabs, Enter/Space activates |
| Toggle switches | Space toggles, Tab moves to next control |
| Theme cards | Arrow keys move between cards, Enter/Space selects |
| Open mute search | Focus lands on search input when Privacy section is active |
| Close modal | Escape (Cancel action) |
| Save (any section) | Ctrl+S / Cmd+S triggers the section's save action |

### 6.9 Accessibility

- All form controls have visible `<label>` elements, not just `aria-label`.
- Toggle switches: `role="switch"`, `aria-checked`, `aria-label` for screen readers.
- Theme cards: `role="radio"` within a `role="radiogroup"`, `aria-checked` on selected card.
- Mute search results: `aria-live="polite"` region announces result count ("5 users found").
- Modals: focus trap, `aria-modal="true"`, `aria-labelledby` pointing to the modal title.
- Save confirmations: `aria-live="assertive"` for toast announcements.
- Destructive actions: buttons have `aria-label` with full context ("Mute @handle", not just "Mute").
- Reduced motion: all transitions respect `prefers-reduced-motion: reduce`. The explicit toggle in Accessibility settings overrides the OS preference when set.
- Color contrast: all text meets WCAG AA 4.5:1 on the dark backgrounds. Theme preview cards use the actual theme colors; if a theme has low-contrast text, the card itself shows a warning icon.

---

## 7. Responsive Behavior

### 7.1 Breakpoints

| Breakpoint | Width | Settings layout |
|---|---|---|
| Desktop | >900px | Sidebar nav (240px) + content area (1fr, max 720px centered) |
| Tablet | 551–900px | Horizontal tab bar + content area (full width, padding 24px) |
| Mobile | ≤550px | Accordion sections (full width, padding 16px) |

### 7.2 Desktop specifics (>900px)

- Sidebar is `position: sticky`, top offset matching the tab bar height. Scrolls with the page but stays visible.
- Content area max-width 720px, centered within the available space. Prevents wide monitors from stretching toggle rows to absurd widths.
- Theme cards: 4-column grid (all four visible).
- Mute search results: max-height 300px, scrollable.

### 7.3 Tablet specifics (551–900px)

- Tab bar: horizontal scroll, each tab is a pill-style button. Active tab has `--accent` bg.
- Content area: full width minus padding. Theme cards: 2-column grid.
- Mute search results: max-height 240px, scrollable.
- Nav disappears; the active tab indicates the current section.

### 7.4 Mobile specifics (≤550px)

- Accordion: only one section open at a time. Opening one closes the previous (200ms max-height transition).
- Theme cards: 2-column grid, smaller swatches.
- Mute search results: full-width list, max-height 200px.
- Custom CSS editor: full-width textarea, reduced height (120px vs 200px on desktop).
- Account deletion modal: full-screen overlay on mobile (not centered card — too much content for a small modal).
- Toggle rows: full-width, label wraps if needed. Minimum 44px touch target maintained.
- Bottom nav (MobileNav) stays visible — Settings is accessible via the "More" menu.

### 7.5 `prefers-reduced-motion`

- All transitions (accordion expand, fade-out on unmute/unblock, save button state change, theme preview) use `transition: none` when the OS preference is set or the Accessibility toggle is on.
- Skeleton loading: static dim rectangles instead of shimmer animation.
- Theme preview: instant color swap instead of crossfade.

---

## 8. Open Questions

| # | Question | Default if unresolved | Impact |
|---|---|---|---|
| OQ-1 | **Save model: auto-save vs explicit?** Notifications use auto-save (debounced toggles); Appearance and Accessibility use explicit Save buttons. Is this mixed model the right UX, or should all sections auto-save? | Mixed model (as spec'd) — auto-save makes sense for independent toggles, explicit saves for correlated fields | Low — either model works; mixed matches iOS/Android conventions |
| OQ-2 | **Mute search scope: local AppView only, or include Bluesky directory?** The spec assumes `GET /api/search/users` against the local AppView (`appview_profiles`). Should mute search also query the Bluesky public API for users not yet in the local index? | Local AppView only — consistent with D6/D12 (no `public.api.bsky.app` dependency) | Medium — affects mute reach for new/obscure users |
| OQ-3 | **Blocked accounts storage: extend `muted_accounts` or separate table?** The spec assumes blocks are an extension of mutes (extra `blocked: boolean` flag). A separate `blocked_accounts` table is cleaner but adds schema complexity. | Separate table (`blocked_accounts`) for clean enforcement semantics | Low — backend decision, frontend is agnostic |
| OQ-4 | **Custom CSS sanitization rules: which properties are allowed?** The spec mentions "no `position: fixed`, no `z-index > 9999`, no external URLs" but the full allowlist is a backend concern. Should the frontend pre-validate or trust the server? | Server validates, frontend shows the server's error message | Low — does not affect screen design |
| OQ-5 | **Data export format: JSON files in zip, or single JSON, or CSV?** The spec proposes JSON files in a zip. Is this user-friendly enough, or should we provide a more accessible format? | JSON in zip (machine-readable, importable) | Low — backend concern |
| OQ-6 | **Account deletion: soft-delete with grace period, or hard-delete?** The spec describes immediate permanent deletion. A 30-day grace period (like Bluesky) is safer but adds backend complexity. | Hard-delete (immediate) — simpler, matches the retro ethos of "gone means gone" | Medium — affects backend design and user expectations |
| OQ-7 | **Notification prefs granularity: per-type only, or also per-sender?** The spec has 6 global toggles. Should users be able to mute notifications from specific senders (e.g., "don't notify me about @spammer's likes")? | Per-type only — per-sender is a Phase 2 enhancement | Low — can be added later without breaking the UI |

---

## Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-12 | Settings writes only to app DB, zero PDS writes | Settings are Bytescape-specific preferences with no AT Protocol equivalent. Keeps the screen simple and avoids PDS scope requirements for Bluesky-hosted users |
| 2026-08-12 | Mixed save model: auto-save for toggles, explicit for correlated fields | Matches the interaction model — independent toggles benefit from instant feedback, correlated fields (theme + CSS) need atomic saves |
| 2026-08-12 | Sidebar nav on desktop, tabs on tablet, accordion on mobile | Standard responsive pattern. Sidebar provides persistent context on large screens; tabs are space-efficient on tablet; accordion prevents scroll fatigue on mobile |
| 2026-08-12 | Two-step account deletion with handle confirmation | Prevents accidental deletion. The handle-confirmation step is the industry standard (GitHub, Twitter, Discord all use it) |
| 2026-08-12 | Mute list is management surface, post-card mute is quick action | Both entry points feed the same `muted_accounts` table. Settings is the authoritative list; the card action is the convenience shortcut (FR-26) |
| 2026-08-12 | Auto-save debounced at 500ms for notification toggles | Prevents rapid-fire API calls during multi-toggle adjustments while keeping the response snappy |
| 2026-08-12 | Account deletion modal uses same DeleteConfirmationPopup pattern as feed spec | Consistency — same focus trap, same Cancel-default-focus, same overlay-dismiss-disabled behavior |

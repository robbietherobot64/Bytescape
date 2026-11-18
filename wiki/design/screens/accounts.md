# Screen Spec: Accounts (v2)

**Status:** Draft
**Date:** 2026-08-12
**Route:** `/accounts`
**Satisfies:** FR-27 (Authentication — Bluesky OAuth + Native Signups), FR-14 (Cross-Posting to Bluesky — account linking prerequisite). Contributes to FR-30 (Native PDS-Hosted Signups — account management surface), AD-07 (AtProtoClient trait — dual write-path visibility for the user).

**Sources:**
- v1 implementation: `apps/web/src/screens/SettingsScreen.tsx` (partial — account section buried in general settings; no standalone accounts screen), `src/components/AccountLinkCard.tsx` (minimal OAuth callback display)
- v2 constraints: `wiki/product/prd.md` (FR-14, FR-27, FR-30), `wiki/architecture/database.md` (`users`, `linked_accounts`, `pds_accounts`, `email_verifications`, `invite_codes` tables), `wiki/architecture/decisions.md` (AD-01b dual PDS, AD-03 did:web, AD-07 AtProtoClient, AD-13 invite-gated PDS hosting)

> **Salvage verdict: LOW.** v1 has no standalone accounts screen — account management was a tab inside Settings that showed a Bluesky connect button and an email field. The v2 dual-PDS model (AD-01b) fundamentally changes what "account management" means: Bytescape-hosted users have signing keys, invite codes, and a `did:web` identity; Bluesky-hosted users have a `did:plc` identity and an OAuth token relationship. The screen is designed from scratch.

---

## 1. Overview

The Accounts screen is the user's identity and security management surface. It shows who they are on the AT Protocol network, manages their linked accounts for cross-posting (FR-14), and handles security actions: password change, email verification, data export, and account deletion. This is not a settings screen — it is specifically about *identity and accounts*, separated from feed/profile/appearance preferences.

The screen serves exactly one audience: **the authenticated account owner**. There is no public or visitor view.

### 1.1 Dual PDS rendering (AD-01b)

The same screen renders two user classes with fundamentally different identity cards:

| | Bytescape-hosted (`did:web`) | Bluesky-hosted (`did:plc`) |
|---|---|---|
| **Identity card** | `📦 Bytescape Home` — handle, `did:web:{handle}.bytescape.social`, email, invite code status | `🦋 Bluesky Account` — handle from bsky.social, `did:plc:...`, PDS endpoint (bsky.social) |
| **Signing key** | Visible (public key fingerprint), rotate action | Not applicable — Bluesky manages keys |
| **Password** | Change password form (Argon2, `users.password_hash`) | Not applicable — OAuth-only, no password |
| **Email** | Show + verify status + change email | Show (from Bluesky OAuth profile, read-only) |
| **Invite code** | Show code + remaining uses (AD-13) | Not applicable |
| **Bluesky link** | "Link Bluesky Account" card for cross-posting (FR-14) | Already linked (this *is* their Bluesky identity) |
| **Account deletion** | Cascade warning: PDS repo, imported archives, widget data, app DB | Cascade warning: unlink from Bytescape; Bluesky account itself is untouched |

Frontend consequence: the screen reads `users.user_type` to determine which card set to render. The split is invisible after initial load — the API returns a unified `AccountInfo` shape; the frontend switches on `userType`.

### 1.2 Data sources

| Data | Location | Notes |
|---|---|---|
| Account identity (handle, email, DID, user_type) | App DB: `users` table | Loaded via `GET /api/accounts/me` |
| DID document + PDS endpoint | `appview_did_handle` | Shown in the identity card for Bluesky-hosted users |
| Linked Bluesky account | App DB: `linked_accounts` table | OAuth token stored server-side; frontend sees `provider`, `external_id`, `external_username` |
| Email verification status | App DB: `email_verifications` table + `users.email_verified` boolean | Pending/verified badge |
| Invite code | App DB: `invite_codes` table | Bytescape-hosted only; code, `max_uses`, `use_count` |
| Signing key (public fingerprint) | App DB: `pds_accounts.signing_key_public` | Bytescape-hosted only; displayed as truncated multikey |
| Import sessions | App DB: `import_sessions` table | Count + status shown in cascade warning |

### 1.3 What changed vs. v1 at a glance

1. **No v1 equivalent** — this is a new screen. The closest v1 artifact is a section inside Settings that showed an email field and a Bluesky connect button.
2. **Dual-PDS identity cards** — Bytescape-hosted and Bluesky-hosted users see different primary cards reflecting their identity model (AD-01b).
3. **Bluesky linking is for cross-posting only** (FR-14) — not for identity. A Bluesky-hosted user is already "linked" by definition; only Bytescape-hosted users see the link/unlink flow.
4. **Data export** — GDPR compliance requirement. No v1 equivalent.
5. **Account deletion with cascade** — shows exactly what gets destroyed before confirmation.
6. **Password change** — Bytescape-hosted users only. Argon2 hash change via `PATCH /api/accounts/password`.

---

## 2. v1 Salvage Assessment

Verdicts: **KEEP** (reuse as-is), **REFINE** (keep concept, improve), **REWORK** (keep concept, rebuild), **DROP** (obsolete), **DEFER** (postpone).

Summary: **0 KEEP · 1 REFINE · 1 REWORK · 2 DROP · 0 DEFER** — consistent with the LOW salvage verdict. v1 had minimal account management surface.

| # | File (lines) | Verdict | One-line rationale |
|---|---|---|---|
| 1 | `SettingsScreen.tsx` (account section, ~80 lines) | **REWORK** | Shows email field + Bluesky connect button — right concept, but buried in settings and built for the single-PDS model. The connect button's OAuth flow is the seed for BlueskyLinkCard |
| 2 | `AccountLinkCard.tsx` (45) | **REFINE** | Displays linked account status + disconnect — keep the pattern, rebuild for dual-PDS awareness (show DID, PDS endpoint, token status) |
| 3 | `SettingsScreen.tsx` (non-account sections) | **DROP** | Feed/profile/appearance settings belong on a separate Settings screen, not here |
| 4 | `OAuthCallback.tsx` (60) | **DROP** | OAuth callback handling moves to a shared auth utility (used by both the Accounts screen link flow and the initial login flow) — not a screen component |

### 2.1 v1 defects fixed by this spec

1. **No standalone accounts surface** — account management was a subsection of Settings, invisible to users who didn't scroll past feed preferences. The `/accounts` route gives it prominence.
2. **Single-PDS assumption** — v1's connect button assumed all users are Bytescape-hosted linking to Bluesky. v2 renders the correct identity card based on `user_type`.
3. **No cascade warning on delete** — v1 had no account deletion flow at all.
4. **No data export** — GDPR compliance requirement not addressed in v1.
5. **Password change not surfaced** — v1's password change (if it existed) was not discoverable from the account section.

---

## 3. Layout

The Accounts screen uses a **settings-style single-column card layout** — vertical stack of identity and action cards, centered, max-width 640px. No sidebars, no multi-panel workspace. This is a management surface, not a browsing surface.

### 3.1 Desktop (>900px)

```
┌────────────────────────────────────────────────────────────────────────────┐
│ TAB BAR   [📰 Feed] [👤 @you] [＋…]                          [🔔 3] [☰] │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│                        ┌─ ACCOUNTS ──────────────────────┐                │
│                        │                                  │                │
│                        │ ┌─ IDENTITY ───────────────────┐ │                │
│                        │ │ 📦 Bytescape Home            │ │                │
│                        │ │                              │ │                │
│                        │ │ Handle    @alice             │ │                │
│                        │ │ DID       did:web:alice.     │ │                │
│                        │ │           bytescape.social   │ │                │
│                        │ │ Email     alice@example.com ✓│ │                │
│                        │ │ Joined    Aug 2026           │ │                │
│                        │ │                              │ │                │
│                        │ │ ── Invite Code ──────────── │ │                │
│                        │ │ Code      BYTESCAPE-ALICE-42 │ │                │
│                        │ │ Uses      3 / 10 remaining   │ │                │
│                        │ │                              │ │                │
│                        │ │ ── Signing Key ──────────── │ │                │
│                        │ │ Public    z6Mkf...x6R      │ │                │
│                        │ │ [🔄 Rotate Key]              │ │                │
│                        │ └──────────────────────────────┘ │                │
│                        │                                  │                │
│                        │ ┌─ LINKED ACCOUNTS ───────────┐ │                │
│                        │ │                              │ │                │
│                        │ │ ┌─ 🦋 Bluesky ────────────┐ │ │                │
│                        │ │ │ @alice.bsky.social       │ │ │                │
│                        │ │ │ did:plc:abc123...        │ │ │                │
│                        │ │ │ ✅ Linked for cross-post │ │ │                │
│                        │ │ │ [Unlink]                 │ │ │                │
│                        │ │ └──────────────────────────┘ │ │                │
│                        │ │                              │ │                │
│                        │ │ OR                           │ │                │
│                        │ │                              │ │                │
│                        │ │ ┌─ Link Bluesky Account ───┐ │ │                │
│                        │ │ │ Connect your Bluesky      │ │ │                │
│                        │ │ │ account to cross-post     │ │ │                │
│                        │ │ │ your posts to Bluesky.    │ │ │                │
│                        │ │ │                           │ │ │                │
│                        │ │ │ [🦋 Connect Bluesky]      │ │ │                │
│                        │ │ └──────────────────────────┘ │ │                │
│                        │ └──────────────────────────────┘ │                │
│                        │                                  │                │
│                        │ ┌─ SECURITY ─────────────────┐ │                │
│                        │ │                              │ │                │
│                        │ │ ── Change Password ──────── │ │                │
│                        │ │ Current   [••••••••]        │ │                │
│                        │ │ New       [••••••••]        │ │                │
│                        │ │ Confirm   [••••••••]        │ │                │
│                        │ │ [> UPDATE PASSWORD]         │ │                │
│                        │ │                              │ │                │
│                        │ │ ── Email Verification ───── │ │                │
│                        │ │ alice@example.com  ✅ Verified│ │               │
│                        │ │ [Resend Verification]        │ │                │
│                        │ │                              │ │                │
│                        │ └──────────────────────────────┘ │                │
│                        │                                  │                │
│                        │ ┌─ DATA ─────────────────────┐ │                │
│                        │ │                              │ │                │
│                        │ │ Export all your data from    │ │                │
│                        │ │ Bytescape including posts,   │ │                │
│                        │ │ imported archives, and       │ │                │
│                        │ │ profile configuration.       │ │                │
│                        │ │                              │ │                │
│                        │ │ [📦 EXPORT MY DATA]          │ │                │
│                        │ │                              │ │                │
│                        │ └──────────────────────────────┘ │                │
│                        │                                  │                │
│                        │ ┌─ DANGER ZONE ──────────────┐ │                │
│                        │ │  background: rgba(255,51,   │ │                │
│                        │ │  51, 0.05)                  │ │                │
│                        │ │                              │ │                │
│                        │ │ Delete Account               │ │                │
│                        │ │ Permanently delete your      │ │                │
│                        │ │ account and all associated   │ │                │
│                        │ │ data. This cannot be undone. │ │                │
│                        │ │                              │ │                │
│                        │ │ [🔴 DELETE MY ACCOUNT]       │ │                │
│                        │ └──────────────────────────────┘ │                │
│                        └──────────────────────────────────┘                │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
 max-width: 640px, centered; cards have 1px --border-dim borders, 8px radius
 --bg-secondary background; section headers in VT323 uppercase
```

### 3.2 Bluesky-hosted user variant

The Identity card changes; Bluesky link card is absent (they *are* a Bluesky account); password section is absent (OAuth-only).

```
                        ┌─ IDENTITY ───────────────────┐
                        │ 🦋 Bluesky Account            │
                        │                              │
                        │ Handle    @alice.bsky.social │
                        │ DID       did:plc:abc123...  │
                        │ PDS       bsky.social        │
                        │ Joined    Aug 2026           │
                        └──────────────────────────────┘

                        ┌─ LINKED ACCOUNTS ───────────┐
                        │                              │
                        │ ✅ This is your Bluesky       │
                        │ account. You can cross-post  │
                        │ to Bluesky from Bytescape.   │
                        │                              │
                        │ To disconnect Bytescape from │
                        │ your Bluesky account, delete │
                        │ your Bytescape account below.│
                        └──────────────────────────────┘

                        ┌─ SECURITY ─────────────────┐
                        │                              │
                        │ ── Email ────────────────── │
                        │ alice@example.com            │
                        │ (Managed via Bluesky)        │
                        │                              │
                        └──────────────────────────────┘
```

### 3.3 Tablet (551–900px)

Same single-column layout — the 640px max-width already fits within tablet viewports. No layout changes.

### 3.4 Mobile (≤550px)

Single column, cards fill the viewport width (padding 16px). Card internals compress: field labels stack above values instead of side-by-side.

```
┌──────────────────────────┐
│ ← Back                   │
├──────────────────────────┤
│ ┌─ ACCOUNTS ───────────┐ │
│ │ ┌─ IDENTITY ────────┐ │ │
│ │ │ 📦 Bytescape Home │ │ │
│ │ │                   │ │ │
│ │ │ Handle            │ │ │
│ │ │ @alice            │ │ │
│ │ │                   │ │ │
│ │ │ DID               │ │ │
│ │ │ did:web:alice.    │ │ │
│ │ │ bytescape.social  │ │ │
│ │ │                   │ │ │
│ │ │ Email             │ │ │
│ │ │ alice@example.com │ │ │
│ │ │ ✅ Verified        │ │ │
│ │ │                   │ │ │
│ │ │ Invite Code       │ │ │
│ │ │ BYTESCAPE-ALICE-42│ │ │
│ │ │ 3 / 10 remaining  │ │ │
│ │ │                   │ │ │
│ │ │ Signing Key       │ │ │
│ │ │ z6Mkf...x6R      │ │ │
│ │ │ [🔄 Rotate Key]   │ │ │
│ │ └───────────────────┘ │ │
│ │                       │ │
│ │ ┌─ LINKED ACCOUNTS ─┐ │ │
│ │ │ ...               │ │ │
│ │ └───────────────────┘ │ │
│ │                       │ │
│ │ ┌─ SECURITY ────────┐ │ │
│ │ │ Current           │ │ │
│ │ │ [••••••••]        │ │ │
│ │ │ New               │ │ │
│ │ │ [••••••••]        │ │ │
│ │ │ Confirm           │ │ │
│ │ │ [••••••••]        │ │ │
│ │ │ [> UPDATE PASSWD] │ │ │
│ │ └───────────────────┘ │ │
│ │                       │ │
│ │ ┌─ DATA ────────────┐ │ │
│ │ │ [📦 EXPORT DATA]  │ │ │
│ │ └───────────────────┘ │ │
│ │                       │ │
│ │ ┌─ DANGER ZONE ─────┐ │ │
│ │ │ [🔴 DELETE ACCT]  │ │ │
│ │ └───────────────────┘ │ │
│ └───────────────────────┘ │
└──────────────────────────┘
 labels stack above fields; padding 16px; cards full-width
```

---

## 4. Component Breakdown

### 4.1 Screen shell

**`AccountsScreen`**
- Purpose: load account info, render the card stack, route OAuth callbacks.
- Props: viewer context from auth.
- State: `accountInfo: AccountInfo | null`, `status: loading | ready | error`, `linkStatus: idle | linking | linked | error`, `deleteModalOpen: boolean`.
- Loads via `GET /api/accounts/me` — returns unified shape with `userType`, `handle`, `did`, `email`, `emailVerified`, `linkedAccounts[]`, `inviteCode?`, `signingKeyFingerprint?`, `joinedAt`.
- OAuth callback handling: when the route contains `?provider=bluesky&code=...`, the screen extracts the code, calls `POST /api/accounts/link/bluesky` with it, and shows link status. On success, fires `bytescape:accountLinked` event (the feed composer listens for this to show the cross-post toggle).
- Stack: React 19 + Vite + TypeScript, useState/useEffect, custom DOM event bus (consistent with the 2026-08-09 stack decision carried by profile and feed specs).

### 4.2 Identity card

**`AccountIdentityCard`**
- Purpose: display the user's AT Protocol identity — handle, DID, email, verification status, and type-specific details (invite code, signing key for Bytescape-hosted; PDS endpoint for Bluesky-hosted).
- Props: `accountInfo: AccountInfo`.
- Renders differently based on `accountInfo.userType`:
  - **`bytescape`**: `📦 Bytescape Home` header, handle, `did:web:...` (full, selectable text), email with verification badge, joined date, invite code section (code + uses remaining), signing key section (truncated public key fingerprint + rotate button).
  - **`bluesky`**: `🦋 Bluesky Account` header, handle (from bsky.social), `did:plc:...` (full, selectable text), PDS endpoint (`bsky.social`), joined date. No invite code, no signing key.
- Subcomponents:
  - `IdentityField` — label + value row, label in Share Tech Mono 11px `--text-muted`, value in Share Tech Mono 14px `--text-primary`. On mobile, stacks vertically.
  - `VerificationBadge` — `✅ Verified` (green) or `⚠️ Unverified` (amber) pill, inline with email.

### 4.3 Linked accounts card

**`BlueskyLinkCard`**
- Purpose: show linked Bluesky account status and provide link/unlink actions for cross-posting (FR-14).
- Props: `linkedAccounts: LinkedAccount[]`, `userType: 'bytescape' | 'bluesky'`, `linkStatus`, `onLink`, `onUnlink`.
- Behavior varies by user type:
  - **Bytescape-hosted** with no linked Bluesky: shows the "Link Bluesky Account" prompt — description text + `🦋 Connect Bluesky` button. Button triggers Bluesky OAuth flow (`/api/auth/bluesky/authorize` → redirect to Bluesky → callback to this screen with `?provider=bluesky&code=...`).
  - **Bytescape-hosted** with linked Bluesky: shows the linked account card — `@alice.bsky.social`, `did:plc:...`, `✅ Linked for cross-post` status, `[Unlink]` button.
  - **Bluesky-hosted**: shows informational text — "This is your Bluesky account. You can cross-post to Bluesky from Bytescape." No link/unlink actions (unlinking = deleting the Bytescape account).
- Unlink flow: clicking `[Unlink]` opens an inline confirmation (not a modal — the card expands to show "Unlink @alice.bsky.social? Cross-posting will be disabled." with `[Confirm Unlink]` red + `[Cancel]` buttons). On confirm: `DELETE /api/accounts/link/bluesky` → card reverts to the link prompt state → fires `bytescape:accountLinked` event (with `null`) so the feed composer hides the cross-post toggle.

### 4.4 Security section

**`AccountActionsSection`**
- Purpose: house security-related actions — password change and email verification.
- Props: `accountInfo: AccountInfo`.
- Subcomponents:

**`PasswordChangeForm`** — Bytescape-hosted users only (`userType === 'bytescape'`).
- Three fields: current password, new password, confirm new password. All `type="password"` with show/hide toggle.
- Validation (client-side, mirrored server-side):
  - Current password: required.
  - New password: minimum 8 characters, at least one uppercase, one lowercase, one digit.
  - Confirm: must match new password.
- Submit: `PATCH /api/accounts/password` with `{ currentPassword, newPassword }`. Server verifies current password (Argon2), hashes new password (Argon2), updates `users.password_hash`.
- Success: toast "Password updated" + fields clear. Error: inline error below the form ("Current password incorrect", "Passwords don't match", etc.).
- Bluesky-hosted users: this subcomponent is not rendered. The security section shows only the email field with "(Managed via Bluesky)" note.

**`EmailVerificationStatus`** — both user types.
- Shows email + verification badge.
- Bytescape-hosted: `[Resend Verification]` button if unverified. Calls `POST /api/accounts/verify-email` → sends a 6-digit code to the email. Button shows "Sent!" for 30s after click, then reverts.
- Bluesky-hosted: email is read-only (from Bluesky OAuth profile). No resend action (Bluesky manages verification).

### 4.5 Data section

**`DataExportButton`**
- Purpose: GDPR-compliant data export — downloads all user data as a ZIP.
- Props: none (reads account context from the screen shell).
- Button: `📦 EXPORT MY DATA`. On click: calls `POST /api/accounts/export`. Server assembles a ZIP containing:
  - `profile.json` — all `users` columns, `custom_profile_fields`, `widget_layout`, `widget_themes`, `theme`, `custom_css`
  - `posts.json` — all `appview_posts` where `author_did = user.did`
  - `archives/` — all `archive_posts` where `author_did = user.did` (JSON per batch)
  - `follows.json` — all `appview_follows` where `actor_did = user.did`
  - `likes.json` — all `appview_likes` where `actor_did = user.did`
  - `events.json` — events + RSVPs authored by user
  - `import_sessions.json` — import history
- While the server assembles: button shows a spinner + "Preparing export..." (the server returns a job ID; the frontend polls `GET /api/accounts/export/{jobId}/status` until ready, then redirects to the download URL).
- This is async because archives can be large — the server generates the ZIP in the background and stores it temporarily (1-hour TTL).

### 4.6 Danger zone

**`DeleteAccountModal`**
- Purpose: account deletion with cascade warning. Triggered by `🔴 DELETE MY ACCOUNT` button, rendered as a modal overlay.
- Props: `isOpen`, `onClose`, `accountInfo`.
- Layout: centered modal, 480px max-width, `--bg-secondary` background, `--danger` accent border-top (3px), focus trap.
- Cascade warning content (differs by user type):

  **Bytescape-hosted cascade:**
  ```
  Deleting your account will permanently destroy:

  • Your PDS repository (did:web:alice.bytescape.social)
    — all posts, likes, follows, and records
  • Your imported archives (Facebook, Instagram, Threads)
    — {N} posts across {M} import sessions
  • Your profile customization
    — widget layout, themes, custom CSS
  • Your invite code
    — BYTESCAPE-ALICE-42 (3 uses remaining)
  • Your wall posts and visitor data
  • All notification history

  This action cannot be undone.
  ```

  **Bluesky-hosted cascade:**
  ```
  Deleting your Bytescape account will:

  • Remove your Bytescape profile and customization
  • Delete your imported archives
    — {N} posts across {M} import sessions
  • Unlink your Bluesky account
    — cross-posting will be disabled
  • Delete your wall posts and visitor data

  Your Bluesky account (@alice.bsky.social) will NOT
  be affected. Your posts on Bluesky remain intact.
  ```

- Confirmation: type your handle to confirm (`DELETE /api/accounts` with `{ confirmHandle: "alice" }`). The handle input is case-sensitive. Server verifies the typed handle matches the authenticated user before proceeding.
- After deletion: redirect to `/` with a toast "Account deleted". Session is invalidated server-side.
- Accessibility: focus trap, Escape closes, `aria-modal="true"`, `aria-labelledby` pointing to "Delete Account" heading, destructive action announced to screen readers.

---

## 5. States

| State | Trigger | UI |
|---|---|---|
| **Loading** | Initial `GET /api/accounts/me` in flight | Skeleton cards — 3 card-shaped shimmer blocks matching the identity/linked/security card heights |
| **Ready** | API returns successfully | Full card stack rendered |
| **Error (load)** | API returns 4xx/5xx or network failure | Centered error card: "Failed to load account info." + `> RETRY` button. Match the feed spec's error pattern |
| **Linking (Bluesky)** | User clicks `🦋 Connect Bluesky` → OAuth redirect in progress | Button shows spinner + "Connecting to Bluesky...". The screen remains interactive (user can scroll, navigate away; the OAuth callback completes on return) |
| **Link success** | OAuth callback returns with valid token | BlueskyLinkCard updates to show linked state. Toast "Bluesky account linked!". `bytescape:accountLinked` event fires |
| **Link error** | OAuth callback returns error or token exchange fails | Inline error on the BlueskyLinkCard: "Failed to link Bluesky account: {reason}. Please try again." + `> RETRY` button |
| **Unlink confirmation** | User clicks `[Unlink]` | Card expands inline: "Unlink @alice.bsky.social? Cross-posting will be disabled." + `[Confirm Unlink]` (red) + `[Cancel]` |
| **Unlinking** | Confirm clicked, `DELETE` in flight | `[Confirm Unlink]` shows spinner |
| **Password change success** | `PATCH` returns 200 | Toast "Password updated", fields clear |
| **Password change error** | `PATCH` returns 4xx | Inline error below the form: specific message from server ("Current password incorrect", "Password too weak") |
| **Email verify sent** | Resend button clicked | Button text changes to "✓ Sent!" for 30 seconds, then reverts to "Resend Verification" |
| **Export preparing** | `POST /api/accounts/export` returns job ID | `📦 EXPORT MY DATA` button shows spinner + "Preparing export..." |
| **Export ready** | Poll returns `ready` status | Button changes to "📦 DOWNLOAD EXPORT" with file size. Click downloads the ZIP |
| **Export error** | Poll returns `failed` status | Inline error: "Export failed. Please try again." + button reverts to `📦 EXPORT MY DATA` |
| **Delete modal open** | User clicks `🔴 DELETE MY ACCOUNT` | Modal overlay with cascade warning + handle confirmation input |
| **Delete confirming** | User typed handle, `DELETE` in flight | `[Delete Forever]` button shows spinner. Input and button disabled |
| **Delete error** | `DELETE` returns 4xx (handle mismatch, etc.) | Inline error in modal: "Handle doesn't match. Please type your handle exactly." |

---

## 6. Interactions

### 6.1 Link Bluesky account (FR-14)

```
User clicks [🦋 Connect Bluesky]
  → Frontend: GET /api/auth/bluesky/authorize
  → Server: builds OAuth URL with scopes (AD-01b OAuth scopes)
  → Redirect to Bluesky OAuth consent page
  → User approves on Bluesky
  → Redirect back to /accounts?provider=bluesky&code={code}
  → Frontend: POST /api/accounts/link/bluesky { code }
  → Server: exchanges code for token, stores in linked_accounts
  → Frontend: updates BlueskyLinkCard to linked state
  → Fires bytescape:accountLinked event
```

### 6.2 Unlink Bluesky account

```
User clicks [Unlink] on linked Bluesky card
  → Card expands to inline confirmation
  → User clicks [Confirm Unlink]
  → Frontend: DELETE /api/accounts/link/bluesky
  → Server: removes linked_accounts row, invalidates stored token
  → Frontend: card reverts to link prompt state
  → Fires bytescape:accountLinked event (null)
```

### 6.3 Change password (Bytescape-hosted only)

```
User fills current + new + confirm fields
  → Client-side validation (length, match, complexity)
  → User clicks [> UPDATE PASSWORD]
  → Frontend: PATCH /api/accounts/password { currentPassword, newPassword }
  → Server: verifies current (Argon2), hashes new (Argon2), updates users.password_hash
  → 200: toast "Password updated", fields clear
  → 4xx: inline error message
```

### 6.4 Delete account

```
User clicks [🔴 DELETE MY ACCOUNT]
  → Modal opens with cascade warning
  → User reads the cascade list (shows actual counts: N posts, M imports)
  → User types their handle in the confirmation input
  → [Delete Forever] button enables only when typed handle matches exactly
  → User clicks [Delete Forever]
  → Frontend: DELETE /api/accounts { confirmHandle: "alice" }
  → Server: verifies handle match, deletes PDS repo (Bytescape-hosted) or
    unlinks (Bluesky-hosted), deletes all app DB rows (cascade), invalidates session
  → 200: redirect to /, toast "Account deleted"
  → 4xx: inline error in modal
```

### 6.5 Data export

```
User clicks [📦 EXPORT MY DATA]
  → Frontent: POST /api/accounts/export
  → Server: creates export job, returns { jobId }
  → Button shows spinner + "Preparing export..."
  → Frontend polls: GET /api/accounts/export/{jobId}/status every 2s
  → Server assembles ZIP (profile + posts + archives + follows + likes + events)
  → Poll returns { status: "ready", downloadUrl: "/api/accounts/export/{jobId}/download" }
  → Button changes to [📦 DOWNLOAD EXPORT (12.4 MB)]
  → User clicks → browser downloads ZIP
  → ZIP has 1-hour TTL; after expiry, button reverts to [📦 EXPORT MY DATA]
```

### 6.6 Keyboard

| Key | Action |
|---|---|
| `Tab` | Cycles through interactive elements within each card, then across cards |
| `Enter` / `Space` | Activates buttons and toggles |
| `Escape` | Closes delete modal (if open), collapses unlink confirmation (if open) |
| `Enter` in handle confirmation input | Submits delete (if handle matches) |

### 6.7 Accessibility

- All form inputs have `<label>` elements with `htmlFor` binding.
- Buttons have `aria-label` when the visible text is ambiguous (e.g., the rotate-key button: `aria-label="Rotate signing key"`).
- Destructive actions (`DELETE`, `Unlink`) announce their consequence to screen readers via `aria-live="polite"` region when the confirmation appears.
- Focus trap in `DeleteAccountModal`: Tab cycles within the modal; Escape closes.
- `prefers-reduced-motion`: disables spinner animations, modal fade-in, and card expand transitions. State changes are instant.
- Color is never the only indicator: verification badges use icon + text (`✅ Verified`, `⚠️ Unverified`), not color alone.
- The danger zone section uses `role="region"` with `aria-label="Danger zone — destructive actions"`.

---

## 7. Responsive Behavior

| Breakpoint | Layout | Changes |
|---|---|---|
| **Desktop (>900px)** | Single column, 640px max-width centered | Full card layout with side-by-side label/value fields |
| **Tablet (551–900px)** | Single column, fills available width (padding 24px) | Same as desktop — 640px fits comfortably |
| **Mobile (≤550px)** | Single column, fills viewport (padding 16px) | Labels stack above values (vertical field layout). Card section headers remain sticky-via-scroll. Delete modal becomes full-width bottom sheet (slides up from bottom, 90vh max, rounded top corners) |

No horizontal scrolling at any breakpoint. No multi-column card layout — the content is management-oriented, not browse-oriented.

---

## 8. Open Questions

| # | Question | Default if unresolved | Impact |
|---|---|---|---|
| OQ-1 | **Signing key rotation UX** — should rotating the signing key require re-authentication (re-enter password) or is it a single-click action? Re-auth is more secure but adds friction. The PDS signing key is the root of identity for `did:web` users. | Require re-authentication (current password) before rotation | Security posture; affects the identity card's signing key section |
| OQ-2 | **Export format** — should the data export be JSON-only, or should it also include human-readable HTML/markdown files? GDPR requires "commonly used, machine-readable format" but a human-readable option improves UX. | JSON-only (machine-readable satisfies GDPR; HTML export is a follow-up) | Implementation effort; GDPR compliance is met either way |
| OQ-3 | **Bluesky unlink scope** — when a Bytescape-hosted user unlinks Bluesky, should existing cross-posted content on Bluesky be left intact, or should there be an option to delete cross-posted records? | Leave intact (unlinking is forward-looking; past posts are already on the PDS and firehose) | Data integrity; user expectations |
| OQ-4 | **Email change for Bytescape-hosted users** — should the screen support changing the email address (with re-verification), or is email immutable after signup? | Support change with re-verification (PATCH /api/accounts/email → sends verification code to new email → confirmed on code entry) | Account management completeness; affects the identity card |
| OQ-5 | **Invite code sharing** — should the invite code section include a "Copy invite link" button that generates a shareable URL (e.g., `bytescape.social/invite/BYTESCAPE-ALICE-42`)? | Yes — one-click copy button next to the code | Growth mechanics (AD-13); trivial to implement |
| OQ-6 | **Account deletion grace period** — should deletion be immediate or is there a 30-day grace period (like Bluesky) where the account can be recovered? | Immediate (no grace period) for v2.0 — grace period requires a `deleted_at` soft-delete model and a recovery flow, which is follow-up scope | Implementation complexity; data retention policy |
| OQ-7 | **Multi-account support** — should the screen support linking multiple Bluesky accounts (for managing multiple identities), or is one Bluesky link per Bytescape account the contract? | One Bluesky link per account (simplifies the data model; multiple identities is a Phase 2 feature) | Data model; `linked_accounts` table already supports multiple rows but the UX should enforce one for now |

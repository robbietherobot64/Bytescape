# Frontend Sprint v18 — Domain Publish Panel, First-Run Flow, Profile Publish

**Sprint**: v18 (Go-Live: Hosted Identity)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-08  
**Previous sprint**: v17 (RepostModal fix, feed density, profile images, UX polish — delivered)  
**HEAD at prompt time**: `a12201a`  
**Decision**: Single-user launch first

---

## Sprint Context

This sprint adds the UI for publishing a user's identity to `bytescape.social`. The backend (Track A) is building the server + Tauri commands in parallel. Frontend depends on two new Tauri commands:

- `publish_identity` → pushes DID + profile to server, returns `PublishResult`
- `check_publish_status` → gets current publish status, returns `PublishStatus`

If backend commands aren't ready yet, **stub the invocations** with `invoke<PublishStatus>('check_publish_status')` and let them fail gracefully — the UI should handle errors.

### What already exists

| Component | File | Status |
|-----------|------|--------|
| Identity setup modal | `components/IdentitySetupModal.tsx` | ✅ Username picker + `set_local_username` |
| Settings "Bytescape Social" section | `screens/SettingsScreen.tsx:577-611` | ⚠️ Three "Coming soon" stubs |
| `AuthStatus` type with `web_did` | `types/index.ts:56-63` | ✅ |
| Profile Builder screen | `screens/ProfileBuilderScreen.tsx` | ✅ Drag-and-drop editor |
| Profile screen | `screens/ProfileScreen.tsx` | ✅ Local profile view |

---

## P0: Add `PublishStatus` Type

Add to `src/types/index.ts`:

```typescript
export interface PublishStatus {
    username: string;
    did_published: boolean;
    did_url: string | null;
    profile_published: boolean;
    profile_url: string | null;
    webfinger_active: boolean;
    last_published_at: string | null;
}

export interface PublishResult {
    status: string;         // "published" | "error"
    did: string | null;
    error: string | null;
}
```

### Files to modify

| File | Changes |
|------|---------|
| `src/types/index.ts` | Add `PublishStatus` and `PublishResult` interfaces |

---

## P1: Replace "Coming Soon" Stubs — Domain Publish Panel

### Location

`src/screens/SettingsScreen.tsx`, lines 577-611 — the "Bytescape Social" section.

### Current state

Three rows with "Coming soon" badges (PDS Account, AP Actor, Relay). Replace the **entire section** with a functional Domain Publish panel.

### New UI Design

```
┌─────────────────────────────────────────────┐
│ ☁️ Bytescape Social                          │
│                                             │
│ Publish your identity to bytescape.social   │
│ for cross-platform verification.            │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ Identity: did:web:bytescape.social:alice│ │
│ │ Status: ✅ Published                    │ │
│ │ Last published: Apr 8, 2026 12:00       │ │
│ │                                         │ │
│ │ ✅ WebFinger    acct:alice@bytescape... │ │
│ │ ✅ DID Document /alice/did.json         │ │
│ │ ✅ Profile Page /@alice                 │ │
│ │                                         │ │
│ │ [🔄 Publish Now]  [View Profile →]      │ │
│ └─────────────────────────────────────────┘ │
│                                             │
│ Publish Secret: ••••••••  [Change]          │
└─────────────────────────────────────────────┘
```

### States

1. **No username set** (`!authStatus.local_username`):
   - Show message: "Set a username in Identity Setup to enable publishing"
   - Link/button to open the Identity Setup modal

2. **Username set, not published** (`authStatus.web_did && !publishStatus.did_published`):
   - Show the DID preview: `did:web:bytescape.social:{username}`
   - Show endpoints that will be created (WebFinger, DID doc, profile)
   - Prominent green **"Publish Identity"** button
   - Publish Secret input field (required before first publish)

3. **Published** (`publishStatus.did_published`):
   - Show green checkmarks for each active endpoint
   - Show "last published" timestamp
   - **"Republish"** button (for after profile edits)
   - **"View Profile"** link → opens `https://bytescape.social/@{username}` in browser

4. **Error state**:
   - Red error banner with the error message
   - Retry button

### Implementation

```typescript
// On mount + after publish:
const publishStatus = await invoke<PublishStatus>('check_publish_status');

// On "Publish Now" click:
const result = await invoke<PublishResult>('publish_identity');
```

### Publish Secret flow

The publish secret authenticates the Tauri app to the server. For single-user launch:

1. Show a text input for "Publish Secret" (masked, with show/hide toggle)
2. On entry, store via a new Tauri command `set_publish_secret` (or reuse keychain pattern)
3. The secret is sent as a Bearer token in publish API calls

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/SettingsScreen.tsx` | Replace "Bytescape Social" section with functional Domain Publish panel |

---

## P2: First-Run Publish Flow

### Trigger

After the user completes the Identity Setup Modal (sets username via `set_local_username`), show a follow-up prompt suggesting they publish their identity.

### Location

Extend `src/components/IdentitySetupModal.tsx` — after the username is confirmed:

### New flow

```
Step 1: Choose username (existing flow)
  ↓
Step 2: DID Preview
  "Your identity will be: did:web:bytescape.social:{username}"
  "Your profile will be at: https://bytescape.social/@{username}"
  [Publish Now] [Skip — publish later from Settings]
  ↓ (if Publish Now)
Step 3: Enter Publish Secret
  "Enter the publish secret for bytescape.social:"
  [text input]
  [Publish]
  ↓
Step 4: Success
  "✅ Your identity is live!"
  "WebFinger: acct:{username}@bytescape.social"
  "DID: did:web:bytescape.social:{username}"
  "Profile: https://bytescape.social/@{username}"
  [View Profile] [Done]
```

### Implementation notes

- The modal currently has a single step. Add a `step` state: `'username' | 'preview' | 'secret' | 'publishing' | 'success' | 'error'`
- Steps 2-4 are only shown after username is confirmed
- "Skip" dismisses the modal entirely — user can publish later from Settings
- Error state shows the error and a "Retry" button

### Files to modify

| File | Changes |
|------|---------|
| `src/components/IdentitySetupModal.tsx` | Add multi-step flow with DID preview + publish trigger |

---

## P3: Profile Publish Trigger

### Problem

After the user edits their profile (display name, bio, avatar, links) via the Profile Builder, they need a way to push those changes to the server.

### Requirements

1. **Add "Publish to bytescape.social" button** on the Profile Builder screen or Profile screen
2. Button calls `invoke('publish_identity')` which pushes the latest profile data
3. Show a toast/banner on success: "Profile published to bytescape.social ✅"
4. Show error state on failure

### Location

Find the save/done action in `src/screens/ProfileBuilderScreen.tsx` or `src/screens/ProfileScreen.tsx` and add a publish trigger next to it.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileBuilderScreen.tsx` or `src/screens/ProfileScreen.tsx` | Add "Publish to bytescape.social" button with invoke call |

---

## P4: Kill Remaining "Coming Soon" Stubs

Search the entire `src/` directory for any remaining "Coming soon" text related to PDS, AP Actor, or Relay. Remove or update them:

- PDS Account → "Available after hosted identity is live" 
- Relay → "Available after hosted identity is live"
- Any other Phase 4.5 references → update messaging

### Files to search

```bash
grep -rn "Coming soon" src/
grep -rn "Phase 4.5" src/
```

---

## Verification

Before committing:

```bash
cd bytescape
npx tsc --noEmit                     # 0 errors
pnpm test -- --run                   # all tests pass
```

Document all changes in `progress.md`.

---

## Track Boundaries

- **Track B owns**: `src/` (all React/TypeScript, CSS)
- **Track A owns**: `src-tauri/src/` (Rust app), `server/` (new server crate)
- **Shared files** (require PM coordination): `src/types/index.ts` (add `PublishStatus`/`PublishResult`)
- Check `git log --oneline -5 -- <filepath>` before editing any shared file

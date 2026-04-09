# Frontend Sprint v20 — Hosted Identity UI

**Sprint**: v20 (Hosted Identity)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-09  
**Previous sprint**: v19 delivered profile layout, themes, rich sections, pages, and connections polish  
**Committed HEAD at prompt time**: `6213f59`

---

## Sprint Goal

Ship the first user-facing UI for publishing a Bytescape identity to `bytescape.social`.

This sprint is **not** full PDS migration and **not** full federation hosting. The target is narrower:

1. User chooses a local username
2. User can see what hosted identity will be created
3. User can publish their DID + profile to `bytescape.social`
4. User can see publish status and republish after profile edits

### User feedback driving this sprint

> "We are now moving into a hosted url based identity, such that users will be able to host a profile and did document for cross platform verification. See notes on next phase."

---

## Current committed state

| Component | File | State |
|---|---|---|
| Identity setup modal | `bytescape/src/components/IdentitySetupModal.tsx` | Username picker only. Calls `set_local_username`. |
| Settings identity section | `bytescape/src/screens/SettingsScreen.tsx` | Still shows Phase 4.5 / "Coming soon" stubs for Bytescape Social. |
| Profile screen | `bytescape/src/screens/ProfileScreen.tsx` | v19 profile work shipped: layout, pages, themes, connection browsing. No publish action yet. |
| Shared frontend types | `bytescape/src/types/index.ts` | No hosted publish status/result types yet. |
| Backend profile export | Track A | `export_profile_data` and `save_avatar_image` were committed in `1f6722b`. Reuse them; do not redesign around missing export support. |

### Working tree warning

There are uncommitted profile-related source changes in the repo right now. Their source is undetermined. Do **not** treat them as approved product direction and do **not** build this sprint around them. Work from the committed state above.

---

## P0: Add shared publish types

Add the frontend types needed for the hosted identity flow.

### Required additions

In `bytescape/src/types/index.ts` add:

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
    status: 'published' | 'error';
    did: string | null;
    error: string | null;
}
```

### Shared-file rule

Before editing `bytescape/src/types/index.ts`, run:

```bash
git log --oneline -5 -- bytescape/src/types/index.ts
```

---

## P1: Replace the Settings stub with a real Domain Publish panel

### File

`bytescape/src/screens/SettingsScreen.tsx`

### Replace

The current `☁️ Bytescape Social` block still contains placeholder rows and "Coming soon" badges.

### New behavior

Render a functional publish panel with these states:

1. **No username yet**
   - Explain that hosted identity requires a Bytescape username
   - Button to open or route to the Identity Setup flow

2. **Username set, not yet published**
   - Preview the hosted DID: `did:web:bytescape.social:{username}`
   - Preview endpoints that will exist:
     - `https://bytescape.social/@{username}`
     - `https://bytescape.social/{username}/did.json`
     - WebFinger entry for `acct:{username}@bytescape.social`
   - Collect a masked publish secret
   - `Publish Identity` button

3. **Published**
   - Show status rows for DID doc, profile page, and WebFinger
   - Show last published time
   - `Republish` button
   - `View Profile` action that opens `https://bytescape.social/@{username}`

4. **Error / backend not ready**
   - Clear error banner
   - Retry option
   - If backend commands are missing, fail gracefully instead of crashing

### Command usage

Use these backend commands once Track A provides them:

```typescript
invoke<PublishStatus>('check_publish_status')
invoke('set_publish_secret', { secret })
invoke<PublishResult>('publish_identity')
```

If the commands are not ready yet, keep the UI functional and show an explicit unavailable state.

---

## P2: Extend IdentitySetupModal into a first-run publish flow

### File

`bytescape/src/components/IdentitySetupModal.tsx`

### Current state

Single-step username picker.

### New flow

Add a lightweight multi-step sequence:

1. **Username**
   - Existing username selection flow

2. **Preview**
   - Show the future DID and public profile URL
   - `Publish now` and `Publish later` actions

3. **Secret**
   - Masked publish secret input
   - Save via `set_publish_secret`

4. **Publishing**
   - Loading state while `publish_identity` runs

5. **Success / error**
   - Success: show DID and profile URL with a `Done` action
   - Error: show the failure and a retry path

### Important

This should remain optional. The user must be able to skip publishing and do it later from Settings.

---

## P3: Add a publish action to the profile editor surface

### File

`bytescape/src/screens/ProfileScreen.tsx`

### Requirement

When the user is editing their profile, provide a visible way to push the latest profile state to `bytescape.social`.

### Behavior

1. Add a `Publish to bytescape.social` action in the existing profile action area
2. Only show it when the user has a local username and has either already published or stored a publish secret
3. Call `publish_identity`
4. Show success and error feedback inline or via the existing notification pattern

---

## P4: Remove stale hosted-identity placeholder language

Search `bytescape/src/` for stale text like:

- `Coming soon`
- `Phase 4.5`
- `PDS Account`
- `Relay`

Update hosted-identity messaging so the app reflects the new publish flow. If a feature is still out of scope, phrase it accurately, for example:

- `Desktop relay hosting comes later`
- `Full PDS hosting comes after hosted profile + DID publish`

---

## Verification

Before committing:

```bash
cd bytescape
npx tsc --noEmit
pnpm test -- --run
```

Document the sprint result in `/.github/agents/prompts/progress.md`.

---

## Track boundaries

- Track B owns: `bytescape/src/`
- Shared file: `bytescape/src/types/index.ts`
- Track A owns the new publish commands and server implementation
- Do not modify unrelated profile WIP currently sitting uncommitted in the repo# Frontend Sprint v20 — Domain Publish Panel & First-Run Publish Flow

**Sprint**: v20 (Go-Live: Hosted Identity)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-09  
**Previous sprint**: v19 (profile customization deepening — delivered)  
**HEAD at prompt time**: `b7fca4e`  
**Decision**: Single-user launch first

---

## Sprint Context

This sprint adds the UI for publishing a user's identity to `bytescape.social`. The backend (Track A) is building the server crate + Tauri publish commands in parallel. Frontend depends on these new Tauri commands:

- `publish_identity` → pushes DID + profile to server, returns `PublishResult`
- `check_publish_status` → gets current publish status, returns `PublishStatus`
- `set_publish_secret` → stores the server auth secret in keychain

If backend commands aren't ready yet, **stub the invocations** with `invoke<PublishStatus>('check_publish_status')` and let them fail gracefully — the UI should handle errors and show a clear "Backend not ready" state.

### User feedback driving this sprint

> "We are now moving into a hosted url based identity, such that users will be able to host a profile and did document for cross platform verification."  
> — `user-feedback`

### What already exists

| Component | File | Status |
|-----------|------|--------|
| Identity setup modal | `components/IdentitySetupModal.tsx` | ✅ Username picker + `set_local_username` |
| Settings "Bytescape Social" section | `screens/SettingsScreen.tsx` | ⚠️ "Coming soon" stubs for PDS/AP/Relay |
| `AuthStatus` type with `web_did` | `types/index.ts:56-63` | ✅ |
| Profile screen (overhauled) | `screens/ProfileScreen.tsx` | ✅ 2-col layout, themes, pages (v18) |
| Profile bio sections | `components/ProfileBioSections.tsx` | ✅ Embeds, links, connections (v18) |
| Profile pages system | `components/ProfilePageNav.tsx`, `ProfilePageEditor.tsx`, `hooks/useProfilePages.ts` | ✅ Full CRUD (v18) |
| Profile settings persistence | `save_profile_settings` / `get_profile_settings` commands | ✅ Backend ready (v18a) |

---

## P0: Add Publish Types

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

**Important**: Check `git log --oneline -5 -- src/types/index.ts` before editing — this is a shared file.

### Files to modify

| File | Changes |
|------|---------|
| `src/types/index.ts` | Add `PublishStatus` and `PublishResult` interfaces |

---

## P1: Replace "Coming Soon" Stubs — Domain Publish Panel

### Location

`src/screens/SettingsScreen.tsx` — find the "Bytescape Social" section (currently has three "Coming soon" badges for PDS Account, AP Actor, Relay).

### Replace with functional Domain Publish Panel

**Design:**

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
│ │ Last published: Apr 9, 2026 12:00       │ │
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
   - Button to open the Identity Setup modal

2. **Username set, not published** (`authStatus.web_did && !publishStatus?.did_published`):
   - Show DID preview: `did:web:bytescape.social:{username}`
   - Show endpoints that will be created (WebFinger, DID doc, profile)
   - Publish Secret input field (required before first publish)
   - Prominent **"Publish Identity"** button (disabled until secret is entered)

3. **Published** (`publishStatus?.did_published`):
   - Green checkmarks for each active endpoint
   - "Last published" timestamp
   - **"Republish"** button (for after profile edits)
   - **"View Profile"** link → opens `https://bytescape.social/@{username}` in system browser

4. **Error state**:
   - Red error banner with the error message
   - Retry button

### Implementation

```typescript
// On mount + after publish:
useEffect(() => {
    invoke<PublishStatus>('check_publish_status')
        .then(setPublishStatus)
        .catch(() => setPublishStatus(null)); // not published yet, or command not ready
}, []);

// On "Publish Now" click:
const handlePublish = async () => {
    setPublishing(true);
    try {
        const result = await invoke<PublishResult>('publish_identity');
        if (result.status === 'published') {
            // Refresh status
            const status = await invoke<PublishStatus>('check_publish_status');
            setPublishStatus(status);
        } else {
            setError(result.error || 'Publish failed');
        }
    } catch (e) {
        setError(String(e));
    } finally {
        setPublishing(false);
    }
};
```

### Publish Secret flow

The publish secret authenticates the Tauri app to the server. For single-user launch:

1. Show a text input for "Publish Secret" (masked, with show/hide toggle)
2. On entry, call `invoke('set_publish_secret', { secret })` to store in keychain
3. The secret is sent as a Bearer token in publish API calls by the backend

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/SettingsScreen.tsx` | Replace "Bytescape Social" section with functional Domain Publish panel |

---

## P2: First-Run Publish Flow

### Trigger

After the user completes the Identity Setup Modal (sets username via `set_local_username`), show a follow-up prompt suggesting they publish their identity.

### Extend IdentitySetupModal

Current state: single-step username picker. Add a multi-step flow:

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
  [masked text input]
  [Publish]
  ↓
Step 4: Success/Error
  ✅ "Your identity is live!"
  "WebFinger: acct:{username}@bytescape.social"
  "DID: did:web:bytescape.social:{username}"
  "Profile: https://bytescape.social/@{username}"
  [View Profile] [Done]
```

### Implementation notes

- Add a `step` state: `'username' | 'preview' | 'secret' | 'publishing' | 'success' | 'error'`
- Steps 2-4 only shown after username is confirmed
- "Skip" dismisses the modal — user can publish later from Settings
- Error state shows the error and a "Retry" button
- Loading state during publish shows a spinner

### Files to modify

| File | Changes |
|------|---------|
| `src/components/IdentitySetupModal.tsx` | Add multi-step flow with DID preview + publish trigger |

---

## P3: Profile Publish Trigger

### Problem

After the user edits their profile (display name, bio, themes, pages) in the Profile screen, they need a way to push those changes to the server.

### Requirements

1. Add a **"Publish to bytescape.social"** button in the Profile screen's edit mode actions area
2. Button calls `invoke('publish_identity')` which pushes the latest profile data
3. Show a success toast/banner: "Profile published to bytescape.social ✅"
4. Show error state on failure
5. Only visible when user has previously published (or has a publish secret set)

### Location

In `src/screens/ProfileScreen.tsx`, within the edit mode toolbar or hero action buttons area, alongside the existing save/edit controls.

### Files to modify

| File | Changes |
|------|---------|
| `src/screens/ProfileScreen.tsx` | Add "Publish to bytescape.social" button in edit mode / hero actions |

---

## P4: Clean Up Remaining "Coming Soon" Stubs

Search `src/` for any remaining "Coming soon" or "Phase 4.5" text related to PDS, AP Actor, or Relay. Update messaging:

- PDS Account → "Available after hosted identity is live"
- Relay → "Available after hosted identity is live"
- Any other stale references → update

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

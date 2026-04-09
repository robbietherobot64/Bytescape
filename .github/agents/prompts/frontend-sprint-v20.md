# Frontend Sprint v20 — Domain Publish Panel & First-Run Publish Flow

**Sprint**: v20 (Go-Live: Hosted Identity)  
**Track**: B (Frontend — React/TypeScript)  
**Date**: 2026-04-09  
**Previous sprint**: v19 (profile customization delivered across `3a899e2` → `6213f59`)  
**HEAD at prompt time**: `6213f59`  
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

Profile customization feedback remains important, but it is no longer the sprint driver. Do not reopen the v19 layout/theme/avatar work in this sprint unless it is required to wire the publish flow.

### What already exists

| Component | File | Status |
|-----------|------|--------|
| Identity setup modal | `components/IdentitySetupModal.tsx` | ✅ Username picker + `set_local_username` |
| Settings "Bytescape Social" section | `screens/SettingsScreen.tsx` | ⚠️ "Coming soon" stubs for PDS/AP/Relay |
| `AuthStatus` type with `web_did` | `types/index.ts:56-63` | ✅ |
| Profile screen | `screens/ProfileScreen.tsx` | ✅ v19 delivered: 2-col layout, themes/backgrounds, pages, avatar rendering |
| Profile bio sections | `components/ProfileBioSections.tsx` | ✅ v19 delivered: embeds, auto-links, GIFs, connections search/filter/expand |
| Profile pages system | `components/ProfilePageNav.tsx`, `ProfilePageEditor.tsx`, `hooks/useProfilePages.ts` | ✅ Full CRUD |
| Profile settings persistence | `save_profile_settings` / `get_profile_settings` commands | ✅ Backend ready |
| Avatar storage + profile export | `save_avatar_image` / `export_profile_data` | ✅ Backend ready for publish flow |

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

Current known locations include `SettingsScreen.tsx`, `DataProvider.ts`, and `TauriDataProvider.ts`.

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

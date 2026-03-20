# Identity Architecture Audit — Sprint v7 Post-Mortem (2026-03-19)

## Executive Summary

The identity system has **three critical inconsistencies** that make it unreliable and confusing to users. These must be fixed before further feature development to prevent compounding tech debt.

---

## Issue 1: Mastodon Uses Fake "DIDs" — `mastodon:instance:account_id`

**Severity:** 🔴 Critical — Violates the identity model, confuses users, creates non-interoperable IDs

The Mastodon connector fabricates identifiers like `mastodon:fosstodon.org:109312` and stores them in `did` fields throughout the system. These are **not DIDs** — they don't conform to any DID method spec and can't be resolved by any DID resolver.

**Where it happens:**
- `connectors/mastodon.rs` lines 692, 701, 729, 756-759, 807, 813, 826, 851 — every conversion function uses `format!("mastodon:{}:{}", instance_host, account.id)` 
- `commands/mastodon.rs` line 267 — the auth completion response sends this fake DID to the frontend: `"did": format!("mastodon:{}:{}", instance_host, session.account_id)`
- `commands/feed_aggregation.rs` line 42 — source detection sniffs for `mastodon:` prefix in URIs

**The correct approach (to be implemented):**
Mastodon-sourced content should use the user's **local `master_did` (did:key:z6Mk...)** as `owner_did` for locally-owned content, and the Mastodon `acct:` URI (e.g., `acct:user@fosstodon.org`) as external author identifiers. The `mastodon:instance:id` format should be replaced with a proper URI scheme.

---

## Issue 2: AP Actor DID Derivation Doesn't Match Keychain DID

**Severity:** 🟠 High — Same keypair generates two different "identities"

The keychain (`keychain_encrypted.rs` line ~298) correctly generates W3C `did:key` with multicodec prefix:
```
did:key:z6Mk... (0xed01 prefix + base58btc + z multibase)
```

The AP commands (`commands/ap.rs` line 23) generate a different format from the **same key**:
```
did:key:{raw_bs58} (no 0xed01 prefix, no z multibase)
```

These produce **different strings from the same Ed25519 public key**, meaning the AP actor's `owner_did` doesn't match the keychain's `master_did`.

---

## Issue 3: No Formal Identity Linking — All Linkage Is Implicit

**Severity:** 🟠 High — No data structure tracks which platform accounts belong to which user

The system has:
- `AuthData.did` → Bluesky `did:plc:...`
- `AuthData.master_did` → `did:key:z6Mk...` from Iroh keypair
- `AuthData.local_identity_did` → local identity DID
- `AuthData.mastodon_session_json` → opaque JSON blob (session tokens only)

But there is:
- ❌ No `linked_accounts` field, table, or data structure
- ❌ No record that "Bluesky did:plc:abc and Mastodon @user@fosstodon.org belong to the same person"
- ❌ No Mastodon reference in P2P `also_known_as` exchange (`protocol.rs` line 447-452 — only adds Bluesky DID and handle)
- ❌ No Mastodon awareness in the Settings screen's "Linked Accounts" section (only renders for `source === 'bluesky'`)

The only reason accounts appear linked today is that they share the same encrypted keychain file on disk. There is no cryptographic proof or portable linkage.

---

## Issue 4: Settings Screen Identity UI Is Broken

**Severity:** 🟠 High — User can see the problems but can't fix them

| Problem | Detail |
|---------|--------|
| **Disconnect is placeholder** | `onClick={() => alert('Disconnect & remove Bluesky data — coming soon.')}` — SettingsScreen line 192 |
| **Connect only offers Bluesky App Password** | No Mastodon option, no OAuth — just `handleLinkBluesky()` calling `create_session` |
| **Source label wrong for Mastodon** | `source === 'local' ? '🔒 Local' : '🦋 Bluesky'` — no Mastodon/fediverse case |
| **Mastodon users see no linked accounts** | Linked accounts section only renders when `auth?.source === 'bluesky'` |
| **Displays raw fake DID** | Shows `mastodon:fosstodon.org:109312` directly to user |

Backend commands exist (`mastodon_disconnect`, `logout_bluesky`) but the frontend doesn't wire to them.

---

## Issue 5: `USE_SYSTEM_BROWSER` Flipped Back to `false`

**Severity:** 🟠 High — Re-introduces the "loading page forever" bug

The PM fixed this at commit `a1e5be7` by setting `USE_SYSTEM_BROWSER = true`. The frontend agent's commit `80bdadc` reset it to `false` and routed profile views through the broken WebView again. This directly causes the user's complaint: "Links and profiles don't load in new tabs."

---

## Proposed Identity Architecture (for Sprint v8)

### The "Umbrella DID" Model

```
                        master_did (did:key:z6Mk...)
                     ╱        |            ╲
                    ╱         |             ╲
    Bluesky         P2P          Mastodon       (future)
    did:plc:abc     node:xyz     acct:user@foss  ActivityPub
                                                  https://bs.social/@user
```

1. **`master_did`** = the universal root identity, derived from Iroh Ed25519 keypair
2. **Platform identifiers** are stored as links under the master DID, not as standalone DIDs
3. **Content ownership**: all locally-stored content uses `master_did` as `owner_did`
4. **External content**: uses platform-native identifiers (`did:plc:` for Bluesky, `acct:` URI for Mastodon, AP actor URL for ActivityPub)

### Data Structure: `LinkedAccounts`

Add to `AuthData` in keychain:
```rust
pub linked_accounts: Vec<LinkedAccount>,

#[derive(Serialize, Deserialize)]
pub struct LinkedAccount {
    pub protocol: String,        // "bluesky" | "mastodon" | "activitypub"
    pub identifier: String,      // "did:plc:abc" | "acct:user@fosstodon.org" | "https://bs.social/@user"
    pub display_handle: String,  // "@user.bsky.social" | "@user@fosstodon.org"
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub connected_at: u64,
    pub active: bool,
}
```

### DID Consistency Fix

All code that derives `did:key` from Ed25519 public keys MUST use the same function: `EncryptedKeychain::did_key_from_ed25519_bytes()`. The AP commands module must be updated to call this instead of raw bs58 encoding.

---

## Files Requiring Changes

### Backend (Track A)
| File | Change |
|------|--------|
| `keychain_encrypted.rs` | Add `linked_accounts: Vec<LinkedAccount>` to `AuthData`, add CRUD methods |
| `commands/ap.rs` | Fix `owner_did` derivation to use `did_key_from_ed25519_bytes()` |
| `commands/mastodon.rs` | `mastodon_complete_auth` → store as `LinkedAccount`, use `master_did` as `owner_did` |
| `connectors/mastodon.rs` | Replace all `mastodon:instance:id` with proper URI formats |
| `commands/feed_aggregation.rs` | Update source detection to not rely on `mastodon:` prefix hack |
| `commands/vault.rs` | Add `list_linked_accounts` / `unlink_account` Tauri commands |
| `protocol.rs` + `commands/peers.rs` | Add Mastodon `acct:` to `also_known_as` P2P exchange |

### Frontend (Track B)
| File | Change |
|------|--------|
| `screens/SettingsScreen.tsx` | Complete overhaul of Identity section — render all linked accounts, wire disconnect buttons, add "Connect Mastodon" option |
| `utils/openInApp.ts` | **Set `USE_SYSTEM_BROWSER = true`** (or better: remove the toggle entirely, always use system browser for non-profile URLs) |
| `types/index.ts` | Add `LinkedAccount` TypeScript interface |
| `screens/LoginScreen.tsx` | Ensure Mastodon login stores as `LinkedAccount` via backend |

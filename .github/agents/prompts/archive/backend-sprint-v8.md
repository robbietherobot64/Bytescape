# Backend Agent — Sprint v8 (2026-03-19) — Identity Overhaul + Bug Fixes

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/identity-architecture-audit.md` — **this is the architectural specification for this sprint.**
Read `.github/agents/prompts/user-feedback` — current user feedback.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Backend/Track A — `src-tauri/src/`, `Cargo.toml`) and the Frontend agent (Track B — all of `src/`).

### Performance Standards
- **Research before coding.** Read existing code before changing it.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine attempts, document what you tried in `progress.md` and move on.
- **Terminal discipline:** Kill background processes when done. Close every terminal you open.
- **Do NOT modify files in `src/`** — that is Frontend scope.

---

## 🚨 Codebase State

### Critical Identity Issues (from PM audit)

The identity system has three critical inconsistencies that MUST be fixed this sprint:

1. **Fake DIDs:** Mastodon connector uses `mastodon:instance:account_id` format in `did` fields. These are not real DIDs.
2. **DID derivation mismatch:** `commands/ap.rs` line 23 uses `format!("did:key:{}", bs58::encode(...))` — raw bs58 without multicodec prefix. The keychain uses the correct W3C format with `0xed01` prefix + `z` multibase: `did:key:z6Mk...`. Same keypair → different strings.
3. **No identity linking:** There's no data structure tracking which platform accounts belong to which user. It's all implicit via shared keychain file.

### User Feedback (Critical)
- "It says mastodon:instance_name:hashnumber. What the hell is that? Aren't we supposed to be using DIDs?"
- "Disconnect button doesn't work"
- "Some Mastodon data not fully loading, namely friends list, posts on profile wall, most posts on You feed"

---

## Sprint Theme: "Iron-Clad Identity System"

The identity model must be internally consistent: one `master_did` (did:key:z6Mk...) as the umbrella identity, with platform accounts as linked children — not as fake DIDs.

---

## Phase 1 — Fix the DID System (CRITICAL — Do First)

### P1 — Fix AP Actor DID Derivation ⚡⚡⚡ CRITICAL

**The bug:** `commands/ap.rs` line 23 computes `owner_did` as:
```rust
let owner_did = format!("did:key:{}", bs58::encode(secret_key.public().as_bytes()).into_string());
```

This produces `did:key:4kBH...` (raw bs58, no multicodec prefix).

The keychain's `did_key_from_ed25519_bytes()` produces `did:key:z6Mk...` (correct W3C format with `0xed01` + `z` multibase).

**The fix:** In ALL places where `owner_did` is computed from an Iroh key, use the keychain's method:
```rust
let owner_did = EncryptedKeychain::did_key_from_ed25519_bytes(secret_key.public().as_bytes());
```

**Files to fix:**
- `commands/ap.rs` — `create_ap_actor` (line 23), `get_ap_actor` (line 53), `send_ap_activity` (line 80), `fetch_remote_actor` (line 114)
- Search for ALL occurrences of `bs58::encode(secret_key.public()` in the codebase and replace with the canonical method

**Verify:** After this fix, `create_ap_actor` and `ensure_master_did` should produce the SAME DID string from the same Iroh keypair.

### P2 — Add `LinkedAccount` Data Structure ⚡⚡⚡ CRITICAL

Add formal identity linking to the keychain.

**In `keychain_encrypted.rs`**, add to `AuthData`:
```rust
#[serde(default)]
pub linked_accounts: Vec<LinkedAccount>,
```

**New struct:**
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkedAccount {
    pub protocol: String,        // "bluesky" | "mastodon" | "activitypub"
    pub identifier: String,      // "did:plc:abc" | "acct:user@fosstodon.org"
    pub display_handle: String,  // "@user.bsky.social" | "@user@fosstodon.org"
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub connected_at: u64,       // Unix timestamp
    pub active: bool,
}
```

**Add keychain methods:**
- `add_linked_account(account: LinkedAccount)` — adds or updates (by protocol + identifier)
- `remove_linked_account(protocol: &str, identifier: &str)` — removes a linked account
- `list_linked_accounts() -> Vec<LinkedAccount>` — returns all linked accounts

**Add Tauri commands (register in `lib.rs`):**
- `list_linked_accounts()` → returns `Vec<LinkedAccount>`
- `unlink_account(protocol: String, identifier: String)` → removes account, clears session data

### P3 — Fix Mastodon Auth to Use Proper Identifiers ⚡⚡⚡ CRITICAL

**In `commands/mastodon.rs` `mastodon_complete_auth`** (line ~267):

Replace:
```rust
"did": format!("mastodon:{}:{}", instance_host, session.account_id),
```

With:
```rust
"did": owner_did,  // The user's master_did from keychain
"mastodon_acct": format!("{}@{}", session.acct, instance_host),
```

Also: When Mastodon auth completes, **create a LinkedAccount** and store it:
```rust
let linked = LinkedAccount {
    protocol: "mastodon".to_string(),
    identifier: format!("acct:{}@{}", session.acct, instance_host),
    display_handle: format!("@{}@{}", session.acct, instance_host),
    display_name: session.display_name.clone(),
    avatar_url: session.avatar.clone(),
    connected_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
    active: true,
};
keychain.add_linked_account(linked)?;
```

Do the same for Bluesky auth — when `create_session` or OAuth completes, store a `LinkedAccount` with `protocol: "bluesky"`.

### P4 — Replace Fake Mastodon DIDs in Connector ⚡⚡

**In `connectors/mastodon.rs`**, replace ALL `format!("mastodon:{}:{}", ...)` patterns:

| Current | Replace With | Purpose |
|---------|-------------|---------|
| Post IDs: `mastodon:instance:status_id` | `https://{instance}/statuses/{id}` | Content identifier (resolvable URL) |
| Author DIDs: `mastodon:instance:account_id` | `acct:{username}@{instance}` | W3C WebFinger `acct:` URI |
| Owner DIDs: `mastodon:instance:account_id` | Use `keychain.get_owner_did()` for locally-owned content | Links to master_did |
| Convo IDs: `mastodon:dm:acct` | `mastodon-dm:{acct}@{instance}` | Conversation key (internal) |

**In `commands/feed_aggregation.rs`** line ~42, update the source detection:
```rust
// Old: if post.uri.starts_with("mastodon:") { "mastodon" }
// New: detect by source field or URI scheme
let source = if post.uri.starts_with("https://") && post.uri.contains("/statuses/") {
    "mastodon".to_string()
} else {
    "bluesky".to_string()
};
```

Or better: carry the `source` field through the adapter instead of sniffing URI prefixes.

---

## Phase 2 — Fix Mastodon Data Loading

### P5 — Fix Mastodon Friends List (Following) ⚡⚡

The user reports Mastodon following list doesn't load. Check:
1. Does `mastodon_get_following` actually get called from the frontend's `list_follows`?
2. Does `list_follows` only call the Bluesky API, ignoring Mastodon?
3. The unified feed pipeline merges Mastodon posts into the Bluesky timeline shape — does the friends list do the same?

**Read `commands/import.rs` function `list_follows`** and verify it includes Mastodon followers via `mastodon_get_following`. If not, wire it in.

### P6 — Fix Mastodon Posts on Profile & "You" Feed ⚡⚡

The user reports most Mastodon posts don't appear. Check:
1. Does `get_author_feed` return Mastodon posts for the logged-in user?
2. Does `get_timeline_unified` include Mastodon posts on the "you" tab?
3. Are Mastodon posts being dropped during the `BytescapeItem` normalization?

Debug by checking the `source` field on returned items and verifying Mastodon items survive the pipeline.

### P7 — Add Mastodon to P2P `also_known_as` ⚡

In `protocol.rs` (line ~447) and `commands/peers.rs` (lines ~34-39), add Mastodon handle to the `also_known_as` exchange:
```rust
if let Ok(Some(session_json)) = keychain.get_mastodon_session() {
    if let Ok(session) = serde_json::from_str::<MastodonSession>(&session_json) {
        also_known_as.push(format!("acct:{}@{}", session.acct, session.instance_host));
    }
}
```

---

## Phase 3 — Wire Disconnect (as time allows)

### P8 — Implement `unlink_account` Command ⚡

The `mastodon_disconnect` command exists but only clears the session. Create a proper `unlink_account` that:
1. Removes the `LinkedAccount` from keychain
2. Clears the session tokens (Mastodon session JSON / Bluesky JWTs)
3. Optionally: deletes cached content from that source
4. Returns the updated list of linked accounts

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend scope
- ❌ Do NOT invent new DID-like formats. Use `did:key:z6Mk...` for local identity, `acct:` URI for Mastodon, `did:plc:` for Bluesky
- ❌ Do NOT break existing Bluesky functionality while fixing Mastodon
- ❌ Do NOT leave zombie terminals open

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] AP actor `owner_did` matches keychain `master_did` for the same keypair
- [ ] `LinkedAccount` struct exists in keychain with CRUD methods
- [ ] `list_linked_accounts` and `unlink_account` Tauri commands registered
- [ ] `mastodon_complete_auth` stores proper `acct:` URI, creates `LinkedAccount`, returns `master_did` as DID
- [ ] ALL `mastodon:instance:id` fake DIDs replaced with proper URI formats
### Phase 2 (Data loading — high priority)
- [ ] Mastodon following list loads in Friends screen
- [ ] Mastodon posts appear on profile wall and "You" feed
- [ ] Mastodon handle appears in P2P `also_known_as` exchange
### Phase 3 (as time allows)
- [ ] `unlink_account` properly clears session + removes LinkedAccount
### Build verification
- [ ] `cargo check` — 0 errors, 0 warnings
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages

# Backend Sprint v13 — Group Moderation, AP Group Federation & Critical Perf Fixes (Track A)

**Sprint Goal:** Complete the remaining group management commands (approve/reject/invite/ban/pin), begin AP Group federation so Bytescape groups can interoperate with Lemmy and PieFed communities, and **investigate/fix two critical performance regressions** reported by the user.

**Context:** Sprint v12 delivered the full groups foundation — 10 types, 5 redb tables, 21 Tauri commands, 9 db tests, seed topic tree. Core membership flow works (join/leave for Public/Closed/Private). What's missing:
- **Moderation ops:** approve_member, reject_member, invite_member, ban_member, set_member_role, pin_group_post (all specified in ROADMAP B4.7/B4.8/B4.10 but deferred)
- **AP Group federation:** Federate local groups as AP `Group` actors so Lemmy/PieFed instances can follow and interact (ROADMAP A2.7)
- **🔥 CRITICAL — Memory Leak (User feedback 2026-03-28):** Filtering feed by video → 32+ GB memory, only Instagram videos show, never load. Videos overall still stuttery.
- **🔥 CRITICAL — CPU Spike on Launch (User feedback 2026-03-28):** CPU spikes to ~100% on app launch, causing lag.

**HEAD:** `25dc29f` on `main`. **Build:** 0 tsc errors, 250 frontend tests, 216 Rust tests pass, cargo check clean.

> **📋 Technical Debt Audit (2026-04-02):** A comprehensive audit surfaced 432 issues (86 HIGH, 207 MEDIUM, 139 LOW) across 162 files. See `.github/docs/technical-debt-audit.md`. This sprint absorbs the **security quick-wins** (SEC-1 through SEC-7 + SEC-13) as a new P0.5 section. Database performance, connector correctness, and architecture refactoring are tracked in ROADMAP Open Issues and deferred to sprint v14+.

---

## P0: Critical Performance Investigations (🔥🔥 User-Reported)

**Priority:** 🔥🔥 Highest — these are user-observed production issues that make the app feel broken.

### 0.1 Memory Leak on Video Filter

**Symptoms:** User filters feed to "Video" → only Instagram archive videos show up (not Bluesky/Mastodon video embeds) → videos attempt to load but never complete → memory usage climbs to 32+ GB.

**Investigation path:**
1. Check `get_unified_feed` / `get_timeline_unified` — does the `Video` content filter correctly include videos from all sources, or only vault-imported Instagram?
2. Check the video HLS conversion pipeline — are FFmpeg child processes being spawned and not killed? Are temporary conversion files accumulating?
3. Check `video_cache` / `hls_cache` — are converted video files being cleaned up? Is there a cache eviction policy?
4. Check blob fetching for video content — is `get_media_blob` loading entire video files into memory without streaming?
5. Profile memory with `MALLOC_CONF` or `heaptrack` to find the actual allocation source.

**Fix requirements:**
- Video filter must show videos from ALL sources (Bluesky, Mastodon, Instagram, vault), not just Instagram archives
- Video loading must be streaming, not full-file-in-memory
- FFmpeg processes must have timeouts and cleanup
- Video cache must have size limits and eviction
- Memory must stay under 2 GB under normal usage

### 0.2 CPU Spike on Launch

**Symptoms:** CPU hits ~100% when app first launches, causing UI lag for several seconds.

**Investigation path:**
1. Check `seed_default_topics()` — does it run on EVERY launch or only on first run? If it's idempotent with an early exit, this isn't the issue. If it re-creates topics every time, fix it.
2. Check the blob consistency check (10s delay startup task) — does it scan ALL blobs on every launch?
3. Check Iroh node initialization — is the DHT bootstrap taking excessive CPU?
4. Check Bluesky OAuth session restoration — is it doing network calls during setup?
5. Add timing logs to each setup phase to identify the bottleneck.

**Fix requirements:**
- `seed_default_topics()` must check if topics exist and return early
- Blob consistency check should be throttled or deferred further
- App should be interactive within 2 seconds of launch
- Add `tracing::info!` timing spans to all startup phases

---

## P0.5: Security Quick-Wins from Technical Debt Audit (🔥🔥 Security-Critical)

**Priority:** 🔥🔥 Highest alongside P0 — these are exploitable vulnerabilities that must be addressed before any public release.

> **Source:** `.github/docs/technical-debt-audit.md` (Audit items 3.1–3.11, 4.3, 4.6). Full security tracker in ROADMAP → Security Hardening (SEC-1 through SEC-13).

### 0.5.1 SEC-6: XSS in OAuth Error Page *(Audit 3.11, LOW effort)*

In `auth.rs`, the OAuth error callback renders `error_description` directly into HTML without escaping. A crafted Bluesky PDS could inject arbitrary HTML/JS.

**Fix:** HTML-escape all interpolated values in the OAuth error response body. Use a simple function:
```rust
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
```

### 0.5.2 SEC-7: Non-Atomic Keychain Writes *(Audit 3.7, LOW effort)*

In `keychain.rs`, `save()` uses `fs::write()` directly. A crash mid-write corrupts the keychain file, causing **total credential loss**.

**Fix:** Write to a temp file, then atomic rename:
```rust
let tmp = path.with_extension("tmp");
fs::write(&tmp, &data)?;
fs::rename(&tmp, &path)?;
```

### 0.5.3 SEC-4: SSRF via JWT `aud` Claim *(Audit 4.3, LOW effort)*

In `connectors/bluesky.rs`, the JWT `aud` claim is used to construct the PDS API endpoint. A crafted JWT can redirect requests to `http://169.254.169.254/` (cloud metadata) or internal services.

**Fix:** After extracting the PDS URL from the JWT, validate it:
1. Must be HTTPS
2. Must not resolve to a private/loopback IP (127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16)
3. Must end with a recognized domain pattern (e.g., `*.bsky.network`, `*.atproto.com`, or user-configured allowlist)

### 0.5.4 SEC-5: HTTP Instance URLs Accepted *(Audit 4.6, LOW effort)*

In `connectors/mastodon.rs`, the Mastodon instance URL is not validated for HTTPS — auth tokens can be sent in plaintext.

**Fix:** Reject any instance URL that doesn't start with `https://`. Add a single check at the top of all Mastodon connection functions.

### 0.5.5 SEC-13: JWT Refresh Race Condition *(Audit 3.1, MEDIUM effort)*

In `auth.rs`, concurrent Tauri commands can each see an expired token, both trigger `refresh_session()`, and the second invalidates the first's new token — causing cascading 401 failures.

**Fix:** Wrap the check→refresh→save cycle in a `tokio::sync::Mutex`. Commands should await the mutex before checking/refreshing:
```rust
// In AppState:
pub session_refresh_lock: tokio::sync::Mutex<()>,
```

### 0.5.6 SEC-1: Master Key as Plaintext File *(Audit 3.3, MEDIUM effort)*

`.master_key` stored as base64 on disk — any user-level process can read and decrypt the entire keychain.

**Fix:** Use the `keyring` crate (or `secret-service` on Linux) to store the master key in the OS credential store. Fall back to the file-based approach with restrictive permissions (`0o600`) if no OS keyring is available. Log a warning if falling back.

### 0.5.7 SEC-2: Hardcoded Recovery Code *(Audit 3.2, LOW effort)*

Auto-initialized identities use `"bytescape-auto-init"` as the recovery passphrase — anyone who obtains the escrow bundle can decrypt the Iroh secret key.

**Fix:** Generate a random 24-character alphanumeric recovery code on identity creation:
```rust
use rand::Rng;
let code: String = rand::thread_rng()
    .sample_iter(&rand::distributions::Alphanumeric)
    .take(24)
    .map(char::from)
    .collect();
```
Display the code to the user once during setup and prompt them to save it.

### 0.5.8 SEC-3: Secrets Never Zeroized *(Audit 3.4, MEDIUM effort)*

`master_key`, JWT tokens, and other secrets are stored as plain `String` — key material persists in freed heap memory and can be read from core dumps or swap.

**Fix:** Add `zeroize` crate to `Cargo.toml`. Replace `String` fields holding secrets with `zeroize::Zeroizing<String>` or create a wrapper type. Derive `ZeroizeOnDrop` on key-holding structs. Priority fields:
- `Keychain.master_key`
- `BlueskySession.access_jwt` / `refresh_jwt`
- `ApActor.secret_key_hex`

### 0.5.9 `clear_all_data` Misses 8 Tables *(Audit 2.4, LOW effort, DATA PRIVACY)*

`clear_all_data()` only clears items, messages, contacts, and blobs — but leaves behind Groups, Guestbook, AP Actors, Collab Docs, and 4 other tables. The user's "Clear all data" expectation is full wipe.

**Fix:** Add all redb table definitions to the clear loop in `db.rs`. Every table defined in `const` at the top of `db.rs` must be included.

### 0.5.10 AP Private Key Leak Risk *(Audit 6.1, LOW effort)*

`ApActor` derives `Serialize` with `secret_key_hex` field — any accidental serialization (logging, API response, error message) leaks the private key.

**Fix:** Add `#[serde(skip)]` to `secret_key_hex` field, or split `ApActor` into `ApActorPublic` (serializable) and `ApActorWithKey` (internal only).

### 0.5 Summary

| Item | Effort | Risk | File |
|------|--------|------|------|
| SEC-6 XSS escape | LOW | HIGH | `auth.rs` |
| SEC-7 Atomic writes | LOW | HIGH | `keychain.rs` |
| SEC-4 SSRF URL validation | LOW | HIGH | `connectors/bluesky.rs` |
| SEC-5 Enforce HTTPS | LOW | MEDIUM | `connectors/mastodon.rs` |
| SEC-13 Refresh mutex | MEDIUM | HIGH | `auth.rs` |
| SEC-1 OS keyring | MEDIUM | HIGH | `keychain.rs` |
| SEC-2 Random recovery code | LOW | MEDIUM | `lib.rs` / identity init |
| SEC-3 Zeroize secrets | MEDIUM | MEDIUM | `keychain.rs`, `auth.rs`, `activitypub/` |
| clear_all_data gaps | LOW | HIGH (privacy) | `db.rs` |
| AP key leak | LOW | HIGH | `activitypub/types.rs` |

**Tests for P0.5:**
- `test_html_escape_special_chars` — verify `<script>` is escaped
- `test_keychain_atomic_write_survives_crash` — write to temp path, verify rename
- `test_pds_url_rejects_private_ip` — verify 127.0.0.1, 10.x, 169.254.x rejected
- `test_mastodon_rejects_http_url` — verify `http://` URL fails
- `test_session_refresh_under_concurrency` — spawn 10 concurrent refresh attempts, verify only 1 refresh occurs
- `test_clear_all_data_clears_all_tables` — verify every defined table is empty after clear

---

## P1: Moderation Commands (🔥 Critical)

**Priority:** 🔥 Critical — Closed groups can't function without member approval. Private groups can't function without invites.

**Task:** Add the remaining membership management commands to `commands/groups.rs`.

### 1.1 approve_member

```rust
#[tauri::command]
pub async fn approve_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String>
```

- Only Owner or Moderator can approve.
- Target member must have `status: Pending`.
- Sets `status` to `Active`. Increments `member_count`.
- Returns error if member is not Pending.

### 1.2 reject_member

```rust
#[tauri::command]
pub async fn reject_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String>
```

- Only Owner or Moderator can reject.
- Deletes the Pending `GroupMember` record entirely.

### 1.3 invite_member

```rust
#[tauri::command]
pub async fn invite_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String>
```

- Only Owner or Moderator can invite.
- Creates a `GroupMember` with `status: Invited`, `role: Member`.
- If member already exists (any status), return error.
- For Private groups, this is the ONLY way to add members.

### 1.4 accept_invite

```rust
#[tauri::command]
pub async fn accept_invite(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<(), String>
```

- Current user must have `status: Invited` for this group.
- Sets `status` to `Active`. Increments `member_count`.

### 1.5 ban_member

```rust
#[tauri::command]
pub async fn ban_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String>
```

- Only Owner can ban (not moderators — moderators can reject pending but not ban active).
- Cannot ban the Owner.
- Sets `status` to `Banned`. Decrements `member_count` if was Active.
- Banned members cannot re-join (enforce in `join_group`).

### 1.6 set_member_role

```rust
#[tauri::command]
pub async fn set_member_role(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
    role: MemberRole,
) -> Result<(), String>
```

- Only Owner can change roles.
- Cannot change Own role (must use a transfer_ownership flow if needed later).
- Member must be Active.

### 1.7 pin_group_post / unpin_group_post

```rust
#[tauri::command]
pub async fn pin_group_post(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String>

#[tauri::command]
pub async fn unpin_group_post(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String>
```

- Only Owner or Moderator.
- Updates `GroupPost.pinned` to true/false.
- `get_group_feed` should return pinned posts first.

### 1.8 remove_group_post

```rust
#[tauri::command]
pub async fn remove_group_post(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String>
```

- Owner or Moderator can remove any post.
- Regular members can remove their own posts.
- Deletes the `GroupPost` record (not the underlying BytescapeItem).

### 1.9 get_pending_members

```rust
#[tauri::command]
pub async fn get_pending_members(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<Vec<GroupMember>, String>
```

- Returns all members with `status: Pending` for a group.
- Only Owner or Moderator can call.

### 1.10 Tests

Add tests to `db/groups.rs`:
- `test_approve_member_flow` — create Pending, approve, verify Active + count
- `test_reject_member_removes_record` — create Pending, reject, verify deleted
- `test_invite_accept_flow` — invite, accept, verify Active + count
- `test_ban_prevents_rejoin` — ban Active member, try join_group, verify error
- `test_pin_unpin_post` — pin post, verify pinned=true, unpin, verify pinned=false
- `test_role_change` — set_member_role Owner→Mod, verify

Register all new commands in `lib.rs` invoke_handler.

**Key files:**
- `src-tauri/src/commands/groups.rs`
- `src-tauri/src/db/groups.rs`
- `src-tauri/src/lib.rs` (command registration)

---

## P2: AP Group Actor Foundation (High)

**Priority:** High — enables federation with Lemmy/PieFed in P3.

**Task:** Create AP `Group` actor representation for local groups.

### 2.1 GroupActor struct

Create `src-tauri/src/activitypub/group_actor.rs`:

```rust
use crate::models::groups::LocalGroup;
use crate::activitypub::types::*;

/// Generate an AP Group actor JSON-LD document for a local group.
pub fn group_to_ap_actor(group: &LocalGroup, base_url: &str) -> serde_json::Value {
    // Returns a JSON-LD document with:
    // - @context: ActivityStreams + security
    // - type: "Group"
    // - id: "{base_url}/group/{group_id}"
    // - preferredUsername: group.slug
    // - name: group.name
    // - summary: group.description
    // - inbox: "{base_url}/group/{group_id}/inbox"
    // - outbox: "{base_url}/group/{group_id}/outbox"
    // - followers: "{base_url}/group/{group_id}/followers"
    // - icon: group.icon_blob (if set)
    // - image: group.banner_blob (if set)
    // - publicKey: group's signing key
    // - sensitive: false
    // - manuallyApprovesFollowers: group.visibility != Public
    // - discoverable: group.visibility != Private
}
```

### 2.2 Group Collections

```rust
/// Generate the Group's outbox Collection (recent posts).
pub fn group_outbox(group_id: &str, posts: &[GroupPost], base_url: &str) -> serde_json::Value

/// Generate the Group's followers Collection.
pub fn group_followers(group_id: &str, follower_dids: &[String], base_url: &str) -> serde_json::Value
```

### 2.3 Post-to-Group AP Activities

```rust
/// Wrap a BytescapeItem posted to a group as a Create/Note activity
/// with `audience` pointing to the group actor.
pub fn create_note_for_group(item: &BytescapeItem, group_actor_id: &str) -> serde_json::Value

/// Wrap a group post in an Announce activity (the group relays to followers).
/// This is how Lemmy distributes community posts.
pub fn announce_to_followers(
    group_actor_id: &str,
    original_activity: &serde_json::Value,
    follower_inboxes: &[String],
) -> serde_json::Value
```

### 2.4 Group Keypair

Each group needs its own ed25519 keypair for signing AP activities:
- On `create_group`, generate a keypair and store it in the encrypted keychain under key `group_key:{group_id}`.
- Add `get_group_signing_key(group_id)` helper to retrieve.

### 2.5 Tests

- `test_group_to_ap_actor_public` — verify Public group has `manuallyApprovesFollowers: false`, `discoverable: true`
- `test_group_to_ap_actor_closed` — verify `manuallyApprovesFollowers: true`
- `test_group_to_ap_actor_private` — verify `discoverable: false`
- `test_create_note_for_group` — verify `audience` field set
- `test_announce_wrapping` — verify Announce activity structure

**New file:** `src-tauri/src/activitypub/group_actor.rs`
**Modified:** `src-tauri/src/activitypub/mod.rs` (add `pub mod group_actor;`)

---

## P3: AP Group Inbox Handlers (High)

**Priority:** High — enables receiving Follow requests from Lemmy/PieFed.

**Task:** Add Group-specific inbox handlers so remote Lemmy/PieFed instances can follow local groups and receive post relays.

### 3.1 Follow → Accept/Reject

When the AP inbox receives a `Follow` activity where the `object` is a Group actor:
1. Look up the local group by AP actor ID.
2. If Public → auto-accept: create `GroupMember` with `Active`, send `Accept(Follow)`.
3. If Closed → create `GroupMember` with `Pending`, queue for mod approval. On approve, send `Accept(Follow)`.
4. If Private → send `Reject(Follow)` (must use `invite_member` flow).

### 3.2 Create/Note → Group Inbox

When a remote actor sends `Create(Note)` to a Group inbox:
1. Verify the sender is an Active member (for Closed/Private groups).
2. Store the note as a `BytescapeItem` (source: ActivityPub).
3. Create a `GroupPost` linking it.
4. Wrap in `Announce` and deliver to all group followers.

### 3.3 Undo(Follow)

When a remote actor sends `Undo(Follow)`:
1. Remove the `GroupMember` record.
2. Decrement `member_count`.

### 3.4 Implementation

Add to `src-tauri/src/activitypub/inbox.rs`:
- `handle_group_follow(activity, db) -> Result<Activity, String>`
- `handle_group_create(activity, db) -> Result<(), String>`
- `handle_group_undo_follow(activity, db) -> Result<(), String>`

Route detection: if `activity.object` matches a local group actor URL pattern, dispatch to group handlers.

### 3.5 Tests

- `test_follow_public_group_auto_accepts`
- `test_follow_closed_group_creates_pending`
- `test_follow_private_group_rejects`
- `test_create_note_to_group_relays_announce`
- `test_undo_follow_removes_member`

---

## P4: Lemmy Compatibility Layer (Medium)

**Priority:** Medium — ensures interop with the most popular federated forum software.

**Task:** Handle Lemmy-specific AP quirks.

### 4.1 Lemmy Post Format

Lemmy communities send posts as `Create/Page` (not `Create/Note`). The `Page` object includes:
- `name` — post title
- `content` — post body (HTML)
- `url` — optional link
- `mediaType: "text/html"`
- `audience` — community actor URL

Add a converter: `lemmy_page_to_bytescape_item(page: &serde_json::Value) -> BytescapeItem`.

### 4.2 Lemmy Community Discovery

When browsing a remote Lemmy instance:
- Fetch `https://{instance}/.well-known/nodeinfo` → get Lemmy version
- Fetch `https://{instance}/api/v3/community/list` → get community list
- Map each to a `GroupSummary` for display in ForumScreen

Add command:
```rust
#[tauri::command]
pub async fn discover_lemmy_communities(
    state: State<'_, AppState>,
    instance_url: String,
) -> Result<Vec<GroupSummary>, String>
```

### 4.3 Follow Remote Community

```rust
#[tauri::command]
pub async fn follow_remote_community(
    state: State<'_, AppState>,
    community_actor_url: String,
) -> Result<(), String>
```

- Send `Follow` activity to the community's inbox.
- On `Accept(Follow)`, create a local placeholder group entry (with source: ActivityPub).

---

## Constraints

- **Track boundary:** Only modify files in `src-tauri/src/`. Do NOT touch `src/` (frontend files).
- **Build verification:** Run `cargo check` + `cargo test` before EVERY commit. Zero errors.
- **Command registration:** Register all new commands in `lib.rs` invoke_handler.
- **Commit format:** `feat(track-a): description` or `fix(track-a): description`.
- **No TODOs in code.** Implement fully.
- **Update `progress.md`** with deliverables when done.

---

## What This Sprint Does NOT Include (Future)

- **Outbound AP delivery infrastructure** (needs relay/server deployment)
- **WebFinger for groups** (needs public HTTP endpoint)
- **PieFed-specific compatibility** (PieFed uses standard AP — Lemmy is the quirky one)
- **Events & Calendar** (sprint v14+)
- **RSS connector** (sprint v14+)

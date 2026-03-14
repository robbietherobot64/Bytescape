# Backend Agent — Sprint Kickoff (2026-03-15)

## PM Sprint Review Summary

A0 is **fully closed**. All 7 GAPs resolved, critical 3-root-cause OAuth bug fixed, post-GAP fixes shipped. Excellent work. You are now unblocked for the most critical path item in the entire project: **A1.1 ActivityPub Actor & Inbox/Outbox**.

### A0 Remediation — Verified ✅
| Deliverable | Status |
|-------------|--------|
| GAP-1 Persistent OAuth Sessions | ✅ Verified — FileSessionStore with ChaCha20-Poly1305 |
| GAP-2 Dynamic Callback Port | ✅ Verified — `127.0.0.1:0` fallback |
| GAP-3 OAuth Tests | ✅ Verified — 30 test functions (OAuthManager, callback parsing, XRPC error paths) |
| GAP-4 Write Operations | ✅ Verified — `is_oauth_session()` branching |
| GAP-5 DMs for OAuth | ✅ Verified — chat proxy + `transition:chat.bsky` scope |
| GAP-6 Feeds for OAuth | ✅ Verified — PDS URL vs AppView routing fixed |
| GAP-7 Transitive Deps | ✅ Verified — p256 transitive, urlencoding in Cargo.toml |
| Critical Bug Fix | ✅ Verified — DPoP auth header, PDS URL, chat proxy path |
| Post-GAP fixes | ✅ Verified — "You" tab, proxy header isolation, parallel import, buddy handles, unified buddy list |
| Master password | ✅ Verified — random 32-byte key persisted to `$APP_DATA/.master_key` |

---

## Priority 0 — Branch State (REQUIRED FIRST)

You are working on **`main`** at commit `fe890b4`. This is correct — stay on `main`.

⚠️ **Do NOT merge from `track-b` or `track-c`** — those branches have frontend/content work that the PM will merge separately. Your job is to keep `main` as the clean backend trunk.

---

## Priority 1 — A1.1: ActivityPub Actor & Inbox/Outbox 🌐

**This is the most critical item in the project.** A1.1 unblocks 7 items across all three tracks:
- Track B: B2.6 (Dual-Format Serving), B9.1 (AP Group Threading), B9.3 (WASM Web Portal), B10.5 (URL Deep Linking)
- Track C: C5.2 (AP destination wiring)
- Track A: A1.2+ (all subsequent AP work)

### What to Build

Create a minimal but functional ActivityPub actor for Bytescape users. This is the **local-first AP implementation** — it doesn't need a public server yet. Focus on the data model and protocol logic that will work both locally and when `bytescape.social` goes live.

#### 1. Module Structure
```
src-tauri/src/activitypub/
├── mod.rs          // Module declarations
├── actor.rs        // AP Actor type, WebFinger, nodeinfo
├── inbox.rs        // Inbox POST handler (signature verification, activity dispatch)
├── outbox.rs       // Outbox GET (collection of user's activities)
├── activities.rs   // Activity types (Create, Follow, Accept, Reject, Like, Announce, Delete)
├── signatures.rs   // HTTP Signatures (sign outbound, verify inbound)
└── types.rs        // ActivityStreams JSON-LD types (Note, Article, Person, etc.)
```

#### 2. AP Actor Record
```rust
pub struct ApActor {
    pub id: String,                    // https://bytescape.social/@{username}
    pub inbox: String,                 // {id}/inbox
    pub outbox: String,                // {id}/outbox
    pub followers: String,             // {id}/followers
    pub following: String,             // {id}/following
    pub preferred_username: String,
    pub name: Option<String>,
    pub summary: Option<String>,       // Bio
    pub public_key: ApPublicKey,       // RSA or ed25519 for HTTP signatures
    pub icon: Option<ApMediaLink>,     // Avatar
    pub image: Option<ApMediaLink>,    // Header
    pub manually_approves_followers: bool,
}
```

Store AP actor configuration in a new redb table: `AP_ACTORS: TableDefinition<&str, &[u8]>` (keyed by DID).

#### 3. Source Enum Update
Add `ActivityPub` variant to `Source` enum in `connectors/mod.rs`:
```rust
pub enum Source {
    Bluesky, Mastodon, Facebook, Instagram, Google, Twitter, Manual, ActivityPub,
}
```
**Also update** `src/types/index.ts` to add `'ActivityPub'` to the `Source` type union.

#### 4. HTTP Signatures
- **Outbound**: Sign all POST requests to remote inboxes with the actor's private key using `(request-target) host date digest` headers.
- **Inbound**: Verify signatures on incoming inbox POSTs by fetching the sender's public key from their actor record.
- Use ed25519 keys (from the existing Iroh node keypair) rather than RSA where possible. Some servers only support RSA — research compatibility.
- Consider the `http-signature-normalization` crate or hand-roll with existing crypto primitives.

#### 5. WebFinger
Implement `/.well-known/webfinger?resource=acct:{username}@bytescape.social` response generation. This doesn't need to be served yet — just generate the correct JSON-RD document that will be served when hosting goes live.

#### 6. Tauri Commands
```rust
#[tauri::command]
pub async fn create_ap_actor(username: String, state: State<'_, AppState>) -> Result<ApActor, String>

#[tauri::command]
pub async fn get_ap_actor(state: State<'_, AppState>) -> Result<Option<ApActor>, String>

#[tauri::command]
pub async fn send_ap_activity(activity_json: String, inbox_url: String, state: State<'_, AppState>) -> Result<(), String>

#[tauri::command]
pub async fn fetch_remote_actor(actor_url: String, state: State<'_, AppState>) -> Result<ApActor, String>
```

Register in `lib.rs` invoke_handler.

### Reference Material
- Study [Forte](https://codeberg.org/fortified/forte) (Rust AP server) for HTTP signature implementation patterns
- Study [Streams](https://codeberg.org/streams/streams) for Nomadic Identity AP patterns
- [ActivityPub spec](https://www.w3.org/TR/activitypub/) — focus on Sections 5 (Collections), 6 (Client to Server), 7 (Server to Server)
- [FEP index](https://codeberg.org/fediverse/fep) for enhancement proposals

### Definition of Done
- [ ] `activitypub/` module compiles with `cargo check`
- [ ] AP Actor can be created and stored in redb
- [ ] HTTP signature generation produces valid signatures (unit test against known test vectors)
- [ ] `ActivityPub` variant added to `Source` enum (Rust + TypeScript)
- [ ] WebFinger response generates correct JSON-RD
- [ ] Activity types serialize to valid ActivityStreams JSON-LD
- [ ] `pnpm tauri dev` builds without errors
- [ ] `pnpm test` passes
- [ ] Committed with `feat(track-a): A1.1 AP Actor & Inbox/Outbox`

---

## Priority 2 — A1.2 Research (Read Only)

While working on A1.1, research what A1.2 (Basic Mastodon/Lemmy Support) will require. Don't implement — just note what endpoints, activity types, and authentication patterns you'll need. This informs the next sprint.

---

## Anti-Patterns
- Do NOT modify frontend components (`src/`) — Track B owns those
- Do NOT modify `ComposeBox.tsx` or content components — Track C owns those
- Do NOT serve HTTP endpoints yet — `bytescape.social` is not live. Build the AP logic as local Tauri commands
- Do NOT add RSA keys if ed25519 works with target servers. Test interop first.
- Do NOT hallucinate crate imports — check Cargo.toml before using

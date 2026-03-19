# Backend Agent — Sprint v7 (2026-03-19) — ActivityPub Pivot

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` — **NOTE: this file is stale/outdated.** Rely on this prompt for current priorities.
Read `.github/agents/prompts/systems-advisory-2026-03-15.md` for the systems architect's recommendations.

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

## 🚨 Codebase State (as of commit `a1e5be7`)

### What was fixed by the PM before this sprint
1. **Source filter: frontend now passes `sourceFilter` to backend** — The `all` tab in `useUnifiedFeed.ts` now sends `sourceFilter` to `get_timeline_unified`. Your sprint v6 backend work is now fully wired.
2. **System browser re-enabled** — External URLs use system browser. The in-app WebView is disabled.

### Sprint v6 results (your previous work)
- ✅ Source filter backend: `source_filter` param on `get_timeline_unified`, composite cache keys
- ✅ `get_or_create_bsky_convo` exposed as Tauri command
- ✅ WebView window label fixed + diagnostic logging + fallback
- ✅ Profile sections persistence: `save_profile_sections` / `get_profile_sections`
- ✅ Facebook import verification: quality confirmed
- ✅ 30 new Rust unit tests

**Build:** cargo ✅ (0 warnings), tsc ✅, 264/264 tests ✅

---

## Sprint Theme: "ActivityPub A1.2 — Mastodon/Lemmy Client"

### Strategic Context

The user wants to move forward with ActivityPub implementation. The app already has Bluesky DM chat, and we want chat to work with ActivityPub too. Rather than polish Bluesky-only chat and redo it later, we're building the AP transport layer now so the Frontend agent can make the chat UI protocol-agnostic from the start.

### What Already Exists (A1.1 — DONE)

You have a solid AP foundation in `src-tauri/src/activitypub/`:

| File | What's there |
|------|-------------|
| `actor.rs` | `ApActor` struct with ed25519 key pair from Iroh, `to_activitystreams()` serialization |
| `types.rs` | `ApObject`, `ApActivity`, `ApCollection`, `ApPublicKey`, `ApMediaLink`, `ApTag`, `ApSource` |
| `activities.rs` | Constructors: `create_note`, `follow`, `accept`, `reject`, `like`, `announce`, `delete`, `undo`, `update`, `bytescape_item_to_note` |
| `inbox.rs` | `process_inbox()` — HTTP signature verification, dispatches Follow/Undo/Like/Announce/Create/Update/Delete/Accept/Reject |
| `outbox.rs` | `deliver_activity()` — signed HTTP POST to remote inbox; `fetch_remote_actor()` — signed/unsigned GET |
| `signatures.rs` | ed25519 HTTP signing for POST and GET |
| `webfinger.rs` | `resolve_webfinger()` — looks up `acct:user@domain` resources |
| `commands/ap.rs` | Tauri commands: `create_ap_actor`, `get_ap_actor`, `send_ap_activity`, `fetch_remote_actor`, `resolve_webfinger` |

All 5 AP commands are registered in `lib.rs` lines 545-549.

### What's Missing (A1.2 — THIS SPRINT)

The A1.1 infra provides the signing, delivery, and inbox processing primitives. What's missing is the **Mastodon/Lemmy C2S client** — the ability to:
1. Authenticate with a remote Mastodon instance (OAuth2)
2. Read a user's home feed from their instance
3. Send/receive DMs (AP `Create(Note)` addressed only to recipient — not `as:Public`)
4. Follow/Unfollow remote actors

---

## Phase 1 — AP Client Foundation (CRITICAL)

### P1 — Mastodon OAuth2 Client ⚡⚡⚡ CRITICAL

Implement Mastodon OAuth2 registration and authentication.

**Background:** Mastodon uses standard OAuth2. The flow is:
1. **App registration:** `POST /api/v1/apps` on the target instance to register Bytescape (returns `client_id` + `client_secret`)
2. **Authorization:** Open `https://{instance}/oauth/authorize?client_id=...&redirect_uri=...&scope=read+write+push&response_type=code` in browser
3. **Token exchange:** `POST /oauth/token` with the auth code to get `access_token`

**Implementation:**
1. Create `src-tauri/src/connectors/mastodon.rs` (or extend the existing connector file if there is one)
2. Store the Mastodon session in `AppState` (similar to `bsky_session`) — struct `MastodonSession { instance: String, access_token: String, account_id: String }`
3. Create Tauri commands:
   - `mastodon_register_app(instance: String)` → registers Bytescape app on the instance, returns OAuth URL to open in browser
   - `mastodon_complete_auth(instance: String, auth_code: String)` → exchanges code for token, stores session
   - `mastodon_get_session()` → returns current session info (or null)
4. Store credentials securely via the existing keychain module (`src-tauri/src/keychain.rs`)

**Reference the Mastodon API docs:**
- App registration: `POST /api/v1/apps` with `client_name`, `redirect_uris`, `scopes`, `website`
- Token: `POST /oauth/token` with `grant_type=authorization_code`, `code`, `client_id`, `client_secret`, `redirect_uri`
- Verify credentials: `GET /api/v1/accounts/verify_credentials` with `Authorization: Bearer {token}`

### P2 — Read Mastodon Home Feed ⚡⚡⚡ CRITICAL

Once authenticated, fetch the user's home timeline and normalize to `BytescapeItem`.

**Implementation:**
1. Add `mastodon_get_timeline(limit: Option<i32>)` command
2. Call `GET /api/v1/timelines/home?limit={limit}` with the stored access token
3. Parse Mastodon `Status` objects and normalize to `BytescapeItem`:
   - `id` → status.id (prefixed with instance to be globally unique, e.g. `mastodon.social:123456`)
   - `content` → strip HTML from `status.content` (Mastodon returns HTML)
   - `source` → `Source::Mastodon`
   - `original_timestamp` → parse `status.created_at` (ISO 8601)
   - `author_name` → `status.account.display_name`
   - `author_handle` → `status.account.acct` (includes @instance for remote accounts)
   - `author_avatar` → `status.account.avatar`
   - `media_paths` → map `status.media_attachments[].url`
   - `item_type` → infer from media: images → `Gallery`, video → `Gallery`, text only → `Status`
4. Cache results in `FeedCache` (same pattern as Bluesky timeline)

**HTML stripping:** Mastodon content is HTML. Use a simple regex to strip tags or add the `ammonia` crate for safe HTML sanitization. Keep `<br>` as newlines, `<a>` href as link text.

### P3 — AP Direct Messages (Send & Receive) ⚡⚡⚡ CRITICAL

This is the key piece that enables protocol-agnostic chat. DMs in ActivityPub are just `Create(Note)` activities addressed to a single recipient (not to `as:Public`).

**Sending DMs:**
1. Create `send_ap_dm(recipient_actor_url: String, text: String)` Tauri command
2. Construct a `Create(Note)` activity where:
   - `to: [recipient_actor_url]` (NOT `as:Public` — this makes it a DM)
   - `cc: []` (empty — private)
   - The `Note` content is the message text
3. Resolve the recipient's inbox URL via `fetch_remote_actor()`
4. Deliver via the existing `deliver_activity()` signed POST
5. Save the sent message locally as a `VaultMessage` with `source: "activitypub"`

**Receiving DMs:**
1. Modify `inbox.rs` `handle_create()` to detect DMs:
   - A DM is a `Create(Note)` where `to` contains only specific actor URLs (not `as:Public`)
   - When detected, save as `VaultMessage` with `source: "activitypub"`, `convo_id: "{sender_actor_url}"`, `sender_did: "{sender_actor_url}"`
2. Emit a Tauri event: `app.emit("new-ap-dm", ...)` so the frontend can show it in real-time

**Important:** The existing `VaultMessage` storage in `db/chat.rs` is already protocol-agnostic — it keys by `{owner_did}:{source}:{id}`. Just pass `"activitypub"` as the source. No schema changes needed.

### P4 — Unified Send Message Command ⚡⚡

Create a protocol-agnostic message-sending command that the frontend can call without knowing the protocol details:

```rust
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    protocol: String,      // "bluesky" | "activitypub" | "p2p"
    convo_id: String,       // Bluesky convo ID, AP actor URL, or P2P node ID
    text: String,
) -> Result<serde_json::Value, String>
```

This dispatches to:
- `"bluesky"` → existing `send_bsky_dm` logic
- `"activitypub"` → new `send_ap_dm` logic from P3
- `"p2p"` → existing P2P message sending

Return a serialized `VaultMessage` so the frontend can immediately display it.

**Do NOT remove `send_bsky_dm`** — keep it for backwards compatibility but have the unified command delegate to it internally.

---

## Phase 2 — AP Social Graph

### P5 — Follow/Unfollow Remote Actors ⚡⚡

1. Create `mastodon_follow(account_id: String)` — calls `POST /api/v1/accounts/{id}/follow` (Mastodon C2S API)
2. Create `mastodon_unfollow(account_id: String)` — calls `POST /api/v1/accounts/{id}/unfollow`
3. Also create the S2S equivalents using the existing AP activity constructors:
   - `send_ap_follow(target_actor_url: String)` — constructs Follow activity, delivers to target inbox
   - `send_ap_unfollow(target_actor_url: String)` — constructs Undo(Follow), delivers to target inbox
4. Update `handle_follow` in `inbox.rs` to auto-accept follow requests (for now) and store the follower in the DB

### P6 — Expose `resolve_profile` Command ⚡⚡

The Frontend agent is building a native profile screen. They need a unified profile resolver.

**Check first:** Search `lib.rs` and `commands/` for existing profile-fetching commands.

**Create:**
```rust
#[tauri::command]
pub async fn resolve_profile(
    state: State<'_, AppState>,
    handle: String,
    protocol: Option<String>,  // "bluesky" | "activitypub" — auto-detect if None
) -> Result<serde_json::Value, String>
```

**Auto-detection logic:**
- If handle contains `@` and looks like `user@instance.social` → treat as Mastodon/AP → resolve via WebFinger then fetch actor
- If handle starts with `did:` or looks like a Bluesky handle → resolve via `app.bsky.actor.getProfile`
- Return a normalized JSON with: `did`/`actor_url`, `handle`, `display_name`, `description`, `avatar`, `banner`, `followers_count`, `following_count`, `posts_count`, `protocol: "bluesky" | "activitypub"`

---

## Phase 3 — Hardening (as time allows)

### P7 — Harden `get_or_create_bsky_convo` ⚡

1. **Add 10-second timeout** — return error instead of hanging
2. **Handle expired sessions** — retry once with `refresh_session()`
3. **Handle missing chat scope** — return clear error: "Re-login required for chat"
4. **Add logging** — log member_did and result for debugging

### P8 — Rate Limit Handling ⚡

Add proper rate limit handling for both Bluesky and Mastodon API calls:
1. Check `RateLimit-Remaining` / `X-RateLimit-Remaining` response headers
2. If rate-limited (429), wait and retry once
3. Log rate limit events

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend scope
- ❌ Do NOT spawn multiple build processes or leave zombie terminals
- ❌ Do NOT make multi-feature commits
- ❌ Do NOT guess at Mastodon API behavior — **read the Mastodon API docs**: `https://docs.joinmastodon.org/api/`
- ❌ Do NOT break the existing Bluesky chat — `send_bsky_dm` and `import_vault_messages` must continue to work

## Definition of Done
### Phase 1 (Critical — must complete)
- [ ] Mastodon OAuth2 flow works: register app, authorize, exchange token, store session
- [ ] `mastodon_get_timeline` fetches home feed and normalizes to `BytescapeItem` with `Source::Mastodon`
- [ ] `send_ap_dm` sends a DM via signed ActivityPub delivery
- [ ] `handle_create` in inbox.rs detects DMs and saves as VaultMessage with `source: "activitypub"`
- [ ] `send_message` unified command dispatches to Bluesky/AP/P2P based on protocol param
### Phase 2 (Social graph — high priority)
- [ ] Follow/Unfollow remote Mastodon actors (C2S and S2S)
- [ ] `resolve_profile` works for both Bluesky handles and `user@instance.social` handles
### Phase 3 (Hardening — as time allows)
- [ ] `get_or_create_bsky_convo` has timeout, session refresh, missing-scope handling
- [ ] Rate limit handling on Bluesky + Mastodon API calls
### Build verification
- [ ] `cargo check` — 0 errors, 0 warnings
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages

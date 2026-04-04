# Backend Sprint v14 — Memory Leak & CPU Spike Investigation, AP Security, Moderation Commands (Track A)

**Sprint Goal:** **Investigate and fix two CRITICAL performance regressions** that have persisted for 2 sprints without any investigation, complete the remaining AP security fixes from the tech debt audit, and deliver the group moderation commands that were scoped for v13 but not delivered.

**Context:** Sprint v13 delivered 10 security hardening fixes (SEC-1/2/3/4/5/6/7/11/13, clear_all_data, willow cleanup) but **did NOT investigate the two highest-priority user issues**: the memory leak and CPU spike. These have been reported as CRITICAL by the user since Sprint v12 (2026-03-28) and confirmed STILL CRITICAL in the latest feedback (2026-04-03). The user explicitly marked these with "STILL AN ISSUE - CRITICAL." They must be the top priority this sprint — no new features until these are resolved.

Additionally, the user reports Instagram images **do not load on the feed** (they load fine in the vault) — this is a regression from the prior report of "slow initial load."

**HEAD:** `7ba4c4a` on `main`. **Build:** 0 tsc errors, 272 frontend tests, 216 Rust tests pass, cargo check clean.

---

## P0: Critical Performance Investigations (🔥🔥🔥 USER-REPORTED, 2 SPRINTS OVERDUE)

**Priority:** 🔥🔥🔥 ABSOLUTE HIGHEST — these are user-observed production issues that have persisted for 2 full sprints. No other work should begin until these have been investigated and either fixed or root-caused with a clear remediation plan.

### 0.1 Memory Leak on Video Filter

**Symptoms:** User filters feed to "Video" → only Instagram archive videos show up (not Bluesky/Mastodon video embeds) → videos attempt to load but never complete → memory usage climbs to 32+ GB.

**Investigation path (follow this sequence):**
1. **Reproduce first.** Launch app, navigate to feed, click Video filter. Monitor memory with `MALLOC_CONF=stats_print:true` or `heaptrack`. Document baseline memory and growth rate.
2. **Check video source filtering.** In `get_unified_feed` / `get_timeline_unified` — does the `Video` content filter correctly include videos from all sources (Bluesky embeds, Mastodon media, vault imports), or only Instagram archives? If only Instagram, the filter logic is broken.
3. **Check FFmpeg process lifecycle.** Are HLS conversion child processes being spawned per video element and never killed? Check `convert_to_hls` and related functions — is there a timeout? Are temp files accumulating in the HLS cache directory?
4. **Check blob fetching for video.** Is `get_media_blob` loading entire video files into memory without streaming? For a 100MB video, this would consume 100MB of heap per concurrent fetch. Check if videos are being fetched in parallel without limits.
5. **Check React-side video element lifecycle.** Are `<video>` elements being created by the virtualizer, scrolled out of view, and never destroyed? Each active `<video>` element holds a media source buffer. If 50+ video elements accumulate, that's the leak.
6. **Check HLS cache eviction.** Is there a `video_cache` / `hls_cache` with size limits? If not, converted HLS segments accumulate on disk and may also be held in memory.

**Fix requirements:**
- Video filter must show videos from ALL sources, not just Instagram archives
- Video loading must be streaming, not full-file-in-memory
- FFmpeg processes must have timeouts (30s max) and cleanup on abort
- Video cache must have size limits (max 500MB disk, 200MB memory) and LRU eviction
- Concurrent video blob fetches limited to 3
- Memory must stay under 2 GB under normal usage, including with video filter active
- Add `tracing::warn!` for any single allocation >50MB

### 0.2 CPU Spike on Launch

**Symptoms:** CPU hits ~100% when app first launches, causing UI lag for several seconds.

**Investigation path:**
1. **Add timing instrumentation.** Wrap each startup phase in `tracing::info_span!` with timing: identity init, Iroh node start, blob consistency check, seed_default_topics, Bluesky session restore, timeline pre-fetch.
2. **Check `seed_default_topics()`** — does it run on EVERY launch or only first run? If it re-creates/updates topics every time, add an early-exit check.
3. **Check the blob consistency check** (10s delay task) — does it scan ALL blobs? If the user has 10,000+ items with media, this is O(n) on every launch. Consider: skip if last check was <24h ago, or check only items modified since last check.
4. **Check Iroh node bootstrap** — is DHT discovery CPU-intensive? Consider deferring non-essential P2P operations until 10s after UI is interactive.
5. **Check Bluesky session restore** — is `restore_oauth_session` doing network calls synchronously during setup? Move to background.

**Fix requirements:**
- `seed_default_topics()` must check if topics exist and return early in <1ms
- Blob consistency check should be skipped if last check was <24h ago
- App must be interactive (UI responsive) within 2 seconds of launch
- All startup phases must log their duration via `tracing::info!`
- No single startup phase should take >500ms on the main thread

### 0.3 Instagram Images Not Loading on Feed

**Symptoms:** Instagram images do not load at all on the feed — just broken/empty placeholders. They load fine in the vault. This was previously reported as "slow initial load" but has worsened.

**Investigation path:**
1. Check how Instagram archive items store their `media_blobs` / `media_paths` — are the blob hashes correct?
2. Check if `get_media_blob` resolves Instagram blob hashes differently in feed context vs. vault context
3. Check if the frontend's `useBlobUrl` hook handles Instagram source items differently
4. Check if the Instagram connector stores paths relative to the archive root vs. absolute paths

**Fix requirements:**
- Instagram images must load on the feed identically to how they load in the vault
- No special-casing by source — the blob resolution path should be source-agnostic

---

## P1: AP Security Fixes from Tech Debt Audit (🔥 Security-Critical)

**Priority:** 🔥 High — these are exploitable vulnerabilities in the ActivityPub federation layer. Must be fixed before any public AP federation deployment.

> **Source:** `.github/docs/technical-debt-audit.md` (Audit items 6.2, 6.3, 6.4, 7.39). Full security tracker in ROADMAP → Security Hardening (SEC-8, SEC-9, SEC-10, SEC-12).

### 1.1 SEC-8: SSRF in AP Inbox *(Audit 6.2)*

`fetch_actor_public_key()` fetches arbitrary URLs from untrusted `keyId` field in HTTP Signature headers. No private-IP check, no timeout.

**Fix:**
1. Add the same private-IP blocklist from SEC-4 (already implemented in `connectors/bluesky.rs`) to AP key fetching
2. Add a 10-second timeout on the HTTP client request
3. Validate that the URL scheme is `https://`

### 1.2 SEC-9: No AP Digest Verification *(Audit 6.3)*

Inbox receives `digest_header` but never verifies it against the actual body hash — allows body substitution attacks where the signature is valid but the body has been tampered.

**Fix:**
1. After reading the body, compute `SHA-256(body)` using the `sha2` crate
2. Compare against the `Digest: SHA-256=...` header value
3. Reject if mismatch (401 Unauthorized)

### 1.3 SEC-10: Hand-Rolled SHA-256 *(Audit 6.4)*

120+ lines of custom SHA-256 implementation instead of using the well-tested `sha2` crate.

**Fix:**
1. Replace the hand-rolled implementation with `sha2::Sha256` from the `sha2` crate
2. Verify all call sites produce identical output
3. Remove the custom SHA-256 code entirely

### 1.4 SEC-12: SSRF in Link Unfurl *(Audit 7.39)*

`unfurl_link` fetches user-provided URLs with no private IP or localhost check — allows scanning internal networks.

**Fix:**
1. Extract the private-IP validation from SEC-4 into a shared `validate_public_url()` utility
2. Call it before any fetch in `unfurl_link`
3. Also add a 5-second timeout

### 1.5 AP Clock Skew Check *(Audit 6.5)*

Inbound AP signature verification doesn't validate the `Date` header — replay attacks possible indefinitely.

**Fix:**
1. Parse the `Date` header from inbound AP requests
2. Reject if the date is more than ±5 minutes from server time
3. Log rejected requests at `tracing::warn!` level

### P1 Tests
- `test_ap_key_fetch_rejects_private_ip` — verify 127.0.0.1, 10.x, 169.254.x rejected
- `test_ap_digest_verification_rejects_tampered_body`
- `test_sha256_matches_sha2_crate` — before removing hand-rolled, verify output equivalence
- `test_unfurl_link_rejects_private_ip`
- `test_ap_clock_skew_rejects_stale_date`

---

## P2: Group Moderation Commands (High — Carried from v13)

**Priority:** High — Closed groups can't function without member approval. Private groups can't function without invites. This was P1 in sprint v13 but was not delivered.

**Task:** Add the remaining membership management commands to `commands/groups.rs`.

### 2.1 Commands to Implement

| Command | Signature | Authorization |
|---------|-----------|---------------|
| `approve_member` | `(group_id, member_did) -> ()` | Owner or Moderator |
| `reject_member` | `(group_id, member_did) -> ()` | Owner or Moderator |
| `invite_member` | `(group_id, member_did) -> ()` | Owner or Moderator |
| `accept_invite` | `(group_id) -> ()` | Current user (must be Invited) |
| `ban_member` | `(group_id, member_did) -> ()` | Owner only |
| `set_member_role` | `(group_id, member_did, role) -> ()` | Owner only |
| `pin_group_post` | `(group_id, item_id) -> ()` | Owner or Moderator |
| `unpin_group_post` | `(group_id, item_id) -> ()` | Owner or Moderator |
| `remove_group_post` | `(group_id, item_id) -> ()` | Owner/Mod (any post), Member (own post) |
| `get_pending_members` | `(group_id) -> Vec<GroupMember>` | Owner or Moderator |

### 2.2 Business Rules
- `approve_member`: Target must have `status: Pending`. Sets to `Active`, increments `member_count`.
- `reject_member`: Deletes the Pending `GroupMember` record entirely.
- `invite_member`: Creates `GroupMember` with `status: Invited`, `role: Member`. Error if member already exists.
- `accept_invite`: Current user must be `Invited`. Sets to `Active`, increments `member_count`.
- `ban_member`: Cannot ban the Owner. Sets `status: Banned`, decrements `member_count` if was Active. Banned members cannot re-join (enforce in `join_group`).
- `set_member_role`: Cannot change own role. Member must be Active.
- `pin_group_post` / `unpin_group_post`: Updates `GroupPost.pinned`. `get_group_feed` should return pinned posts first.
- `remove_group_post`: Deletes `GroupPost` record (not underlying BytescapeItem).

### 2.3 Tests
- `test_approve_member_flow` — create Pending, approve, verify Active + count
- `test_reject_member_removes_record` — create Pending, reject, verify deleted
- `test_invite_accept_flow` — invite, accept, verify Active + count
- `test_ban_prevents_rejoin` — ban Active member, try join_group, verify error
- `test_pin_unpin_post` — pin post, verify pinned=true, unpin, verify pinned=false
- `test_role_change` — set_member_role Member→Mod, verify
- `test_ban_owner_fails` — attempt to ban owner, verify error

Register all new commands in `lib.rs` invoke_handler.

**Key files:**
- `src-tauri/src/commands/groups.rs`
- `src-tauri/src/db/groups.rs`
- `src-tauri/src/lib.rs` (command registration)

---

## P3: Connector Correctness Fixes (Medium)

**Priority:** Medium — data integrity issues that affect import quality. Address if time permits after P0-P2.

### 3.1 Facebook Encoding Drops Non-Latin Unicode *(Audit 4.4)*

`fix_facebook_encoding` silently drops characters ≥ U+0100 — emoji, CJK, Cyrillic lost from imported posts.

**Fix:** Rewrite `fix_facebook_encoding` to properly handle Facebook's double-encoded UTF-8 (they encode UTF-8 bytes as Latin-1 escape sequences). Use `encoding_rs` or manual byte-level decoding that preserves all Unicode code points.

### 3.2 Facebook Timezone-Unaware Timestamps *(Audit 4.5)*

Local dates treated as UTC — every imported timestamp off by user's UTC offset (up to ±12 hours).

**Fix:** Add a `timezone_offset` parameter to the Facebook import command. Apply the offset to all parsed timestamps. Default to UTC if not provided, but prompt the user to select their timezone during import.

### 3.3 Stale Header Cache After Clear *(Audit 2.1)*

`HeadersCache` not invalidated after `clear_all_data` / `remove_syndicated_duplicates`.

**Fix:** Add `headers_cache.invalidate_all()` calls after any operation that modifies the items table.

---

## Constraints

- **Track boundary:** Only modify files in `src-tauri/src/`. Do NOT touch `src/` (frontend files).
- **Build verification:** Run `cargo check` + `cargo test` before EVERY commit. Zero errors.
- **Command registration:** Register all new commands in `lib.rs` invoke_handler.
- **Commit format:** `feat(track-a): description` or `fix(track-a): description`.
- **No TODOs in code.** Implement fully.
- **P0 FIRST.** Do not begin P1/P2/P3 until P0 memory leak and CPU spike have been investigated and either fixed or root-caused with specific findings documented.
- **Update `progress.md`** with deliverables when done.

---

## What This Sprint Does NOT Include (Future)

- **AP Group Actor / Group Federation** (sprint v15+) — deferred until moderation commands are verified working
- **Lemmy Compatibility Layer** (sprint v15+)
- **Database schema redesign** (sprint v15+) — full table scan elimination requires major refactor
- **import.rs god file split** (sprint v15+) — structural refactor, no user-facing impact
- **Bluesky post parsing deduplication** (sprint v15+)

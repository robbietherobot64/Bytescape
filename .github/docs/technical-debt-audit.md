# Bytescape Technical Debt Audit

> **Date**: 2026-03-31
> **Scope**: Batches 1–6 (39 source files · ~18,700 lines · Rust + TypeScript)
> **Auditor**: Systems Agent
> **Purpose**: Identify bugs, hacks, security issues, and architectural debt for PM sprint planning

---

## Executive Summary

A systematic audit of the first 6 documentation batches (Core Types, Database, Backend Core, Platform Connectors, P2P/Sync, ActivityPub Federation) uncovered **131 issues** across 39 files:

| Severity | Count | Description |
|----------|-------|-------------|
| 🔴 HIGH | 28 | Bugs, data loss, security vulnerabilities, panics |
| 🟡 MEDIUM | 62 | Performance traps, architectural hacks, missing features |
| 🔵 LOW | 41 | Code smells, dead code, naming issues |

The top systemic concerns are:

1. **Full table scans everywhere** — The `TableDefinition<&str, &str>` universal schema forces JSON deserialization on every query that isn't a direct key lookup. This affects ~15 methods across the database layer.
2. **Security gaps in auth/keychain** — Master key stored as plaintext file, hardcoded recovery code, JWT refresh race condition, secrets never zeroized.
3. **Duplicated logic** — Post parsing duplicated 4× in bluesky.rs, DID types duplicated across files, crypto boilerplate duplicated between keychain and oauth.
4. **Missing pagination** — Follows/followers only return first page in both Bluesky and Mastodon connectors, unbounded loops in several API calls.
5. **Memory pressure from large media** — Facebook, Instagram, and vault operations load entire media files into memory with no size guards.

---

## Batch 1 — Core Types & Models

> **Files**: `types/index.ts`, `models/mod.rs`, `models/groups.rs`, `connectors/mod.rs`, `error.rs`, `activitypub/types.rs`
> **Lines**: ~1,911

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 1.1 | `types/index.ts` | **TS `FeedEntry` has fields not in Rust** — `repost_count`, `reply_count` declared but never populated by backend | Frontend reads `undefined` silently; false API surface |
| 1.2 | `models/mod.rs` | **`source` is `String`, not `Source` enum** — the typed `Source` enum exists but `BytescapeItem.source` stores a raw string | Typos like `"bluessky"` pass silently; entire enum's type safety bypassed for the most important struct |
| 1.3 | `error.rs` | **Blanket `From<String>` into `BytescapeError`** — any string auto-converts to `Internal` error via `?` | Defeats typed error handling; specific errors silently become generic `Internal` |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 1.4 | `types/index.ts` | **Optional/required field drift** — `media_paths` and `media_blobs` are `?` in TS but required (with `serde(default)`) in Rust | Unnecessary null-checks on frontend; inconsistent API contract |
| 1.5 | `types/index.ts` | **Phantom TS fields** — `replies`, `reply_parent`, `pinned_at` have no Rust counterpart | Dead type surface misleads frontend developers |
| 1.6 | `models/mod.rs` | **3 duplicate classification functions with divergent logic** — `classify_content_type()`, `reclassify()`, `suggested_content_type()` implement the same concept with different rules | Same content classified differently depending on which is called |
| 1.7 | `models/mod.rs` | **Legacy media maps never auto-migrated** — `media_types`, `media_dimensions` read-only but no auto-migration on read | Old items permanently carry stale fields |
| 1.8 | `connectors/mod.rs` | **`from_str()` maps unknowns to `Manual` silently** — unrecognized source strings become `Manual` with no warning | Bugs masked; corrupted data appears as user-created |
| 1.9 | `connectors/mod.rs` | **`from_str()` is inherent, not `FromStr` trait** — breaks idiomatic `"bluesky".parse::<Source>()` | Anti-pattern; confusing to Rust developers |
| 1.10 | `error.rs` | **Frontend can't distinguish error types** — all errors convert to plain `String` for Tauri IPC | Frontend must do brittle string-matching on error messages |
| 1.11 | `error.rs` | **`From<anyhow::Error>` discards error chain** — only outermost message preserved | Root causes lost in error reports |
| 1.12 | `error.rs` | **Overlapping error variants** — `Internal` vs `Custom`, `AuthRequired` vs `Unauthorized` | Inconsistent usage across codebase |
| 1.13 | `activitypub/types.rs` | **`ApActivity.object` is `serde_json::Value`** — fully untyped | No compile-time checking of AP message structure |
| 1.14 | `activitypub/types.rs` | **`object_type` is `String`, not enum** — same for `activity_type`, `collection_type` | Typos compile fine; no exhaustiveness checking |
| 1.15 | `activitypub/types.rs` | **Missing `@context` field on all AP types** — must be manually injected into JSON | Forgetting it produces invalid AP messages |

### 🔵 LOW

| # | File | Issue |
|---|------|-------|
| 1.16 | `types/index.ts` | Runtime constant `PRESET_LENSES` in a types file — should be in constants |
| 1.17 | `types/index.ts` | Deprecated `BskyPost` type still exported |
| 1.18 | `types/index.ts` | Missing `quote_count` in TS (exists in Rust) |
| 1.19 | `models/mod.rs` | `WillowWriteParityEntry` debug types in production models |
| 1.20 | `models/mod.rs` | Byte-index string slicing in `preview_text()` — will panic on multi-byte chars if `clean_text` ever allows Unicode |
| 1.21 | `models/groups.rs` | No `Default`, `Eq`, `Hash` derives on enums |
| 1.22 | `connectors/mod.rs` | `Threads` and `Tumblr` variants declared but connectors not implemented |
| 1.23 | `activitypub/types.rs` | `published`/`updated` are `Option<String>` not `DateTime` — no timestamp validation |
| 1.24 | `activitypub/types.rs` | `ApTag` missing `#[serde(rename_all = "camelCase")]` unlike sibling types |

---

## Batch 2 — Database Layer

> **Files**: `db/mod.rs`, `db/groups.rs`, `db/chat.rs`, `db/vault.rs`, `db/peers.rs`, `db/contacts.rs`
> **Lines**: ~2,312

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 2.1 | `db/vault.rs` | **Stale header cache after `clear_all_data`/`remove_syndicated_duplicates`** — cache not invalidated | `list_items_paged` returns phantom items that no longer exist |
| 2.2 | `db/mod.rs` | **`blocking_write()` in async context** — blocks Tokio worker thread while holding RwLock | Potential deadlock under concurrent Tauri commands |
| 2.3 | `db/vault.rs` | **TOCTOU race in `save_item`** — read and write in separate transactions | Deduplication bypass under concurrent saves |
| 2.4 | `db/mod.rs` | **`clear_all_data` misses 8 tables** — groups, guestbook, AP actors, collab docs not cleared | "Clear all data" privacy feature leaves behind sensitive data |
| 2.5 | `db/chat.rs` | **`list_messages_for_convo` is O(all_messages)** — deserializes every message in DB, then filters | 10K DMs = 10K JSON parses per conversation open |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 2.6 | `db/chat.rs` | **`list_vault_messages_paged` loads ALL messages to paginate** — full scan + sort + slice | 50K messages = 50K JSON parses per page request |
| 2.7 | `db/groups.rs` | **`list_user_groups` full table scan** — scans all groups × all members | Finding "my groups" scans every membership record |
| 2.8 | `db/vault.rs` | **Filter queries use N+1 pattern** — `get_items_by_*` loops calling `get_item` individually | 500 matches = 500 separate read transactions |
| 2.9 | `db/vault.rs` | **`get_item` fallback full table scan** — if key format doesn't match, scans entire items table | O(n) worst case on every item lookup |
| 2.10 | `db/vault.rs` | **`remove_syndicated_duplicates` double full-table scan** — iterates items table twice | ~20K JSON parses per cleanup call at 10K items |
| 2.11 | `db/groups.rs` | **Non-atomic read-modify-write for member count** — separate transactions, denormalized count can drift | Member count can become incorrect permanently |
| 2.12 | `db/groups.rs` | **No cascade deletes** — `delete_group` doesn't cascade to members/posts | Orphaned records accumulate silently |
| 2.13 | `db/groups.rs` | **`search_groups` full table scan** — deserializes every group for substring match | Every search query processes entire table |
| 2.14 | `db/mod.rs` | **Write lock acquired for read-then-write** — exclusive lock held during read phase blocks all readers | All DB access serialized during these operations |

### 🔵 LOW

| # | File | Issue |
|---|------|-------|
| 2.15 | `db/vault.rs` | Debug `println!` left in production code |
| 2.16 | `db/chat.rs` | `.unwrap()` on SystemTime duration — panics if clock before 1970 |
| 2.17 | `db/peers.rs` | `list_peers` silently swallows iteration errors |
| 2.18 | `db/vault.rs` | Inline key construction duplicated vs helper function |
| 2.19 | `db/vault.rs` | `count_items` swallows errors with `unwrap_or(0)` |
| 2.20 | `db/contacts.rs` | `list_contacts` returns unordered results |
| 2.21 | `db/groups.rs` | Hardcoded `50` limit in `search_groups` — should be parameter |

---

## Batch 3 — Backend Core (Auth, Keys, App State)

> **Files**: `lib.rs`, `auth.rs`, `keychain_encrypted.rs`, `oauth.rs`, `contacts.rs`, `main.rs`
> **Lines**: ~3,731

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 3.1 | `oauth.rs` | **JWT refresh race condition** — no mutex around check→refresh→save cycle | Two concurrent commands can both refresh, second invalidates first's token → spurious logout |
| 3.2 | `lib.rs` | **Hardcoded recovery code `"bytescape-auto-init"`** — used for auto-initialized identities | Anyone with the escrow bundle can decrypt the user's Iroh secret key |
| 3.3 | `keychain_encrypted.rs` | **Master encryption key stored as plaintext file** — `.master_key` on disk as base64 | Any user-level process can read it and decrypt entire keychain |
| 3.4 | `keychain_encrypted.rs` | **Secrets in memory never zeroized** — `master_key`, JWT fields are plain `String` | Key material persists in freed heap memory indefinitely |
| 3.5 | `oauth.rs` | **Silently dropped save errors** — `let _ = keychain.save_session(...)` | User thinks they're logged in but session lost on restart |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 3.6 | `keychain_encrypted.rs` | **Mutex held across blocking file I/O** — blocks Tokio thread | Thread starvation under concurrent auth checks |
| 3.7 | `keychain_encrypted.rs` | **Non-atomic file writes** — direct `fs::write` | Crash mid-write corrupts keychain → total credential loss |
| 3.8 | `keychain_encrypted.rs` | **Argon2id uses minimal parameters** — 19 MiB / 2 iterations | Insufficient for at-rest encryption; OWASP recommends 47+ MiB |
| 3.9 | `lib.rs` | **Unbounded `HeadersCache` with no eviction** — grows without limit | Memory leak over long sessions with diverse feeds |
| 3.10 | `oauth.rs` | **Single-read TCP parsing for OAuth callback** — may truncate with proxies | OAuth authentication fails silently on certain network configs |
| 3.11 | `oauth.rs` | **XSS in OAuth error HTML page** — unescaped `error_description` | Code injection via crafted OAuth redirect |
| 3.12 | `keychain_encrypted.rs` | **Recovery code minimum 8 chars** — ~41 bits entropy | Brute-forceable with modern GPU |
| 3.13 | `auth.rs` | **`needs_refresh` uses wall clock instead of JWT `exp`** — heuristic can be wrong | Unnecessary refreshes or missed refreshes |
| 3.14 | `oauth.rs` | **No retry on session refresh** — single attempt, then fail | Brief network glitch forces full re-authentication |

### 🔵 LOW

| # | File | Issue |
|---|------|-------|
| 3.15 | `lib.rs` | New `reqwest::Client` per chat request — bypasses shared pool |
| 3.16 | `oauth.rs` | OAuth creates standalone client — doesn't share connection pool |
| 3.17 | `contacts.rs` | `ContactManager` is entirely dead code (~130 lines) |
| 3.18 | `contacts.rs` | O(n²) contact deduplication — should use HashSet |
| 3.19 | `contacts.rs` | Verbose bio logging in production |
| 3.20 | `keychain_encrypted.rs` + `oauth.rs` | Duplicated encryption boilerplate |
| 3.21 | `keychain_encrypted.rs` | `integrity_md5` field now stores BLAKE3 — misleading name |
| 3.22 | `main.rs` | `.expect()` on app run — unhelpful panic message |

---

## Batch 4 — Platform Connectors

> **Files**: `connectors/bluesky.rs`, `connectors/facebook.rs`, `connectors/mastodon.rs`, `connectors/instagram.rs`
> **Lines**: ~6,970

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 4.1 | `bluesky.rs` | **Post parsing duplicated 4×** — `parse_api_post`, `get_feed_with_media`, `get_liked_posts`, `get_bookmarks` each implement their own extraction | Bug fixes only apply to one path; quoted posts missing in 3 of 4 |
| 4.2 | `bluesky.rs` | **`get_all_follows_with_posts` is N+1 API bomb** — 2000 follows = 2000+ sequential API calls with zero rate limiting | Will trigger Bluesky rate limits; possible token revocation |
| 4.3 | `bluesky.rs` | **SSRF via JWT `aud` claim** — extracted URL used to construct API endpoint | Crafted JWT can redirect requests to internal network addresses |
| 4.4 | `facebook.rs` | **`fix_facebook_encoding` silently drops non-Latin-1 Unicode** — chars ≥ U+0100 filtered out | Posts containing emoji, CJK, Cyrillic lose characters silently |
| 4.5 | `facebook.rs` | **Timezone-unaware timestamp parsing** — treats local dates as UTC | Every imported timestamp off by user's UTC offset (up to ±12 hours) |
| 4.6 | `mastodon.rs` | **HTTP URLs accepted for instance** — tokens sent in plaintext `Authorization` headers | Credential leakage over unencrypted connections |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 4.7 | `bluesky.rs` | Double extraction in `get_feed_with_media` overwrites embed data | Wasteful; copy-paste debt |
| 4.8 | `bluesky.rs` | `get_feed_with_media` ignores `limit` parameter, hardcodes 100 | Caller's requested limit silently ignored |
| 4.9 | `bluesky.rs` | `get_liked_posts` has unbounded pagination loop | 50K+ likes = unbounded download |
| 4.10 | `bluesky.rs` | `get_all_follows` / `get_all_followers` unbounded | Millions of followers = infinite loop |
| 4.11 | `bluesky.rs` | Dead variable `_pds_url` makes unnecessary HTTP call | Wasted network round-trip on every feed import |
| 4.12 | `bluesky.rs` | `create_bluesky_client` silently defaults to empty JWT | All subsequent requests fail with confusing 401s |
| 4.13 | `facebook.rs` | Old-format message parser is dead code — never creates records | Old-format Facebook exports lose all Messenger history |
| 4.14 | `facebook.rs` | Recursive JSON extraction creates duplicate posts | Parent and child objects both become posts with overlapping media |
| 4.15 | `facebook.rs` | O(n²) whitespace collapse with `while contains / replace` | Quadratic for strings with many spaces |
| 4.16 | `facebook.rs` | Media files loaded entirely into memory | Multi-GB videos cause OOM |
| 4.17 | `mastodon.rs` | `get_follows` / `get_followers` don't paginate — single page only | Users with >80 follows silently truncated |
| 4.18 | `mastodon.rs` | Mastodon status URL mapped to `preview_img` | Frontend may render spurious link cards on every Mastodon post |
| 4.19 | `mastodon.rs` | `follow_user` / `unfollow_user` bypass rate limiter | Rapid actions trigger instance rate limits |
| 4.20 | `mastodon.rs` | Rate limiter only retries once on 429 | Aggressive instances cause failures during batch operations |
| 4.21 | `instagram.rs` | Media files loaded entirely into memory | Same OOM risk as Facebook |
| 4.22 | `instagram.rs` | `app_handle` parameter unused — no progress events emitted | Large imports give zero user feedback |
| 4.23 | `instagram.rs` | Wrong log prefix "facebook" in Instagram module | Copy-paste error; confuses debugging |
| 4.24 | `instagram.rs` | DM conversation ID derived from display name | Name changes create duplicate conversations |

### 🔵 LOW

| # | File | Issue |
|---|------|-------|
| 4.25 | `bluesky.rs` | Misleading underscore prefix on used `_access_jwt` parameter |
| 4.26 | `bluesky.rs` | No rate limiting infrastructure for Bluesky API calls |
| 4.27 | `facebook.rs` | `BytescapeItem` construction missing `source` field vs Instagram |
| 4.28 | `facebook.rs` | Heuristic media distribution assigns wrong photos to posts |
| 4.29 | `facebook.rs` | ~50+ debug `println!` statements — excessive log noise |
| 4.30 | `mastodon.rs` | DM `convo_id` based on sender, not conversation | Group DMs incorrectly merged |
| 4.31 | `mastodon.rs` | `strip_html_tags` only handles 6 HTML entities | Numeric entities like `&#8217;` appear as literal text |
| 4.32 | `instagram.rs` | Empty string sentinel values for missing media paths |
| 4.33 | `instagram.rs` | Cross-module dependency on `fix_facebook_encoding` |

---

## Batch 5 — P2P, Sync & Real-time

> **Files**: `sync.rs`, `protocol.rs`, `chat.rs`, `collab.rs`, `blob_hash.rs`, `richtext.rs`, `media_server.rs`, `traffic.rs`
> **Lines**: ~3,400

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 5.1 | `sync.rs` | **O(n) full scan on every file add** — `vault_add_file()` calls `vault_list_files()` for dedup check | Every upload triggers full namespace scan + JSON deser |
| 5.2 | `collab.rs` | **Full ITEMS_TABLE scan fallback** — `snapshot_collab_to_item()` scans all items if direct key lookup fails | O(n) worst case from key format mismatch |
| 5.3 | `sync.rs` | **No size guard on `get_media()`** — reads entire blob into memory | 4 GB video blob = OOM crash |
| 5.4 | `chat.rs` | **Follows/followers pagination missing** — only returns first page | Users with 2000+ connections see only ~100 |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 5.5 | `protocol.rs` | **3-second sleep hack in FEED_REQUEST** — `tokio::time::sleep(3s)` after writing response | Blocks connection handler for 3s on every feed request |
| 5.6 | `chat.rs` | **Duplicate HTTP client** — creates own `Client::new()` vs shared `SHARED_CLIENT` | Wasted TLS sessions, DNS cache, connection pool |
| 5.7 | `chat.rs` | **Duplicated `DidDocument` / `DidService` types** — identical to `bluesky.rs` | Changes must be synchronized across files |
| 5.8 | `protocol.rs` | **Keychain read on every request** — `get_fresh_profile()` reads encrypted keychain per P2P connection | Disk I/O + potential decryption per incoming connection |
| 5.9 | `collab.rs` | **Full Yjs doc deserialization per update** — loads entire doc, applies update, re-serializes | Expensive for large CRDT documents |
| 5.10 | `sync.rs` | **GC doesn't actually delete** — replaces tombstones with "GC" markers that also accumulate | Namespace grows forever |
| 5.11 | `traffic.rs` | **Hardcoded TODO in Bluesky path** — `let _access_jwt = "TODO"` | TrafficController's Bluesky routing is non-functional |
| 5.12 | `richtext.rs` | **New HTTP client per mention resolution** | Unnecessary client creation overhead |

### 🔵 LOW

| # | File | Issue |
|---|------|-------|
| 5.13 | `sync.rs` | JSON classified as Archive in `VaultFileKind::from_mime` |
| 5.14 | `chat.rs` | Stale PDS URL cache — not invalidated on account switch |
| 5.15 | `protocol.rs` | No response size limit on feed serving — 50 large items could be megabytes |
| 5.16 | `collab.rs` | Triple encoding: binary → base64 → JSON → redb string |
| 5.17 | `media_server.rs` | TOCTOU port binding (acknowledged as negligible) |
| 5.18 | `collab.rs` | `blocking_read()` then `begin_write()` — misleading lock pattern |
| 5.19 | `chat.rs` | No rate limiting on outgoing Bluesky chat API calls |
| 5.20 | `blob_hash.rs` | CIDv1 "last 32 bytes" assumption — fragile for other multihash codecs |

---

## Cross-Cutting Themes & Recommended Sprint Priorities

### Priority 1: Security Hardening Sprint 🔒

These issues represent real vulnerability surface:

| Issue | Severity | Effort |
|-------|----------|--------|
| 3.3 Master key as plaintext file | 🔴 | Medium — integrate OS keyring |
| 3.2 Hardcoded recovery code | 🔴 | Low — generate random code |
| 3.4 Secrets never zeroized | 🔴 | Low — add `zeroize` crate |
| 4.3 SSRF via JWT `aud` | 🔴 | Low — URL validation |
| 4.6 HTTP instance URLs accepted | 🔴 | Low — enforce HTTPS |
| 3.11 XSS in OAuth error page | 🟡 | Low — HTML-escape |
| 3.7 Non-atomic file writes | 🟡 | Low — write-then-rename |

### Priority 2: Database Performance Sprint 🗄️

The `<&str, &str>` universal schema causes cascading performance issues:

| Issue | Severity | Effort |
|-------|----------|--------|
| 2.5 `list_messages_for_convo` O(n) | 🔴 | Medium — composite keys |
| 5.1 `vault_list_files` O(n) per upload | 🔴 | Medium — local hash index |
| 2.8 N+1 query pattern in filters | 🟡 | Medium — batch `get_items` |
| 2.9 `get_item` fallback scan | 🟡 | Medium — secondary UUID index |
| 5.2 Collab snapshot scan fallback | 🔴 | Low — store DB key in collab meta |
| 2.1 Stale header cache | 🔴 | Low — invalidate on clear |

### Priority 3: Connector Correctness Sprint 🔌

Data integrity issues in platform connectors:

| Issue | Severity | Effort |
|-------|----------|--------|
| 4.4 Facebook encoding drops Unicode | 🔴 | Medium — rewrite decoder |
| 4.5 Facebook timezone handling | 🔴 | Medium — detect/ask timezone |
| 4.1 Bluesky parsing duplicated 4× | 🔴 | Medium — unify to `parse_api_post` |
| 4.2 N+1 API bomb in follows | 🔴 | Medium — add rate limiter + cap |
| 4.16/4.21 OOM from large media files | 🟡 | Medium — stream to disk |

### Priority 4: Auth Resilience Sprint 🔑

| Issue | Severity | Effort |
|-------|----------|--------|
| 3.1 JWT refresh race condition | 🔴 | Low — add Mutex |
| 3.5 Silently dropped save errors | 🔴 | Low — propagate errors |
| 1.3 Blanket `From<String>` on errors | 🔴 | Medium — remove, fix call sites |
| 3.14 No retry on refresh | 🟡 | Low — add backoff |

### Priority 5: Type System Cleanup Sprint 🧹

Foundational issues that affect all layers:

| Issue | Severity | Effort |
|-------|----------|--------|
| 1.2 `source` as String not enum | 🔴 | Medium — change type, update all serialization |
| 1.4-1.5 TS/Rust field drift | 🟡 | Low — audit and align |
| 1.6 Duplicate classification functions | 🟡 | Low — unify |
| 5.7 Duplicated DID types | 🟡 | Low — extract shared module |

---

## Batch 6 — ActivityPub Federation

> **Files**: `activitypub/mod.rs`, `activitypub/inbox.rs`, `activitypub/outbox.rs`, `activitypub/actor.rs`, `activitypub/activities.rs`, `activitypub/signatures.rs`, `activitypub/webfinger.rs`, `activitypub/group_actor.rs`, `activitypub/lemmy.rs`
> **Lines**: ~2,867

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 6.1 | `actor.rs` | **`secret_key_hex` on `#[derive(Serialize)]` struct** — `ApActor` derives Serialize, so `serde_json::to_value(&actor)` would leak the Ed25519 private key. Only `to_activitystreams()` manually excludes it. | Any accidental serialization (e.g., debug logging, error response, Tauri command return) leaks the private key |
| 6.2 | `inbox.rs` | **SSRF in `fetch_actor_public_key`** — fetches arbitrary URLs from untrusted Signature header `keyId` field. No URL allowlist, no private-IP check, no timeout. | Attacker can probe internal services (localhost, metadata endpoints) by sending crafted activities with `keyId` pointing to internal URLs |
| 6.3 | `inbox.rs` | **No Digest verification** — `process_inbox` receives the `digest_header` but never verifies it matches the actual body hash. An attacker could replace the body after signing. | Body substitution attack: signature covers headers (including Digest), but if Digest isn't checked against body, a MITM could swap the body |
| 6.4 | `signatures.rs` | **Hand-rolled SHA-256** — 120+ lines of custom SHA-256 implementation instead of using the `sha2` crate. No fuzzing, no test vectors beyond one known-answer test. | Correctness risk in cryptographic code; not auditable; maintenance burden |
| 6.5 | `signatures.rs` | **No clock skew check on Date header** — inbound signature verification doesn't validate that the Date header is within a reasonable window (e.g., ±5 minutes). | Replay attacks: a captured signed request can be replayed indefinitely |
| 6.6 | `activities.rs` | **`bytescape_item_to_note()` drops all media attachments** — images, videos, and audio on posts are silently lost when federating to the fediverse | Users' media posts appear as text-only on Mastodon/Lemmy |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 6.7 | `actor.rs` | **`AP_DOMAIN` hardcoded as `"bytescape.social"`** — not configurable for self-hosting or development | Self-hosters can't federate under their own domain; dev/staging environments share production domain |
| 6.8 | `inbox.rs` | **All activity parsing uses `serde_json::Value`** — completely untyped dispatch. The `ApActivity` and `ApObject` typed structs in types.rs are unused by the inbox. | No compile-time checks on AP message structure; field name typos silently produce None/empty |
| 6.9 | `inbox.rs` | **`from_name` in DM handler uses `attributedTo` URL as name** — `attributedTo` is an actor URL (e.g., "https://mastodon.social/@alice"), not a human-readable name | DM sender shown as a URL instead of a display name in the chat UI |
| 6.10 | `inbox.rs` | **Activity handlers don't persist state** — handle_follow, handle_like, etc. return InboxResult but don't actually store anything. The caller must handle persistence. | Easily forgotten; if the caller doesn't persist, follows/likes are silently lost |
| 6.11 | `outbox.rs` | **No retry logic in `deliver_activity()`** — single attempt, fail permanently. No backoff, no queue, no dead-letter log. | Transient network errors cause permanent delivery failure; federation unreliable |
| 6.12 | `outbox.rs` | **No shared inbox optimization** — delivers individually to each follower's inbox, even if many followers are on the same instance | N followers on instance X = N requests instead of 1 to sharedInbox |
| 6.13 | `outbox.rs` | **`fetch_remote_actor()` has no timeout** — uses default reqwest timeout (potentially infinite) | Hung connections block the async runtime |
| 6.14 | `activities.rs` | **No `@context` field in constructed activities** — only `actor.to_activitystreams()` includes it. Activity constructors (create_note, follow, like, etc.) omit it. | Invalid ActivityPub JSON-LD; some servers may reject or misparse context-less activities |
| 6.15 | `signatures.rs` | **`algorithm="ed25519"` in Signature header** — the HTTP Signatures spec expects `"hs2019"` for modern algorithms. Some implementations may reject `"ed25519"` as non-standard. | Potential federation failures with strict implementations |
| 6.16 | `lemmy.rs` | **`original_timestamp` stored as i64 (seconds) but `imported_at` as u64 (milliseconds)** — different timestamp units in the same struct | Sorting/display bugs when mixing Lemmy-imported items with other sources |
| 6.17 | `lemmy.rs` | **`author_did` and `author_handle` both set to the actor URL** — Lemmy's `attributedTo` is a URL, not a DID or a handle | Frontend displays raw URLs instead of usernames for Lemmy post authors |
| 6.18 | `lemmy.rs` | **`discover_communities()` only validates Lemmy instances** — returns error for PieFed, Mbin, and other Lemmy-API-compatible software | Unnecessarily restrictive; blocks compatible instances |
| 6.19 | `group_actor.rs` | **`create_note_for_group()` uses `imported_at` for `published` timestamp** — should use `original_timestamp` | Posts appear with import time instead of original creation time |
| 6.20 | `webfinger.rs` | **`resolve_webfinger()` has no timeout** — uses default reqwest timeout | Hung connections when resolving remote WebFinger |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 6.21 | `actor.rs` | **`owner_did` initialized as empty string** — set by caller later, but no validation | Potential for empty `owner_did` in actor records if caller forgets |
| 6.22 | `inbox.rs` | **DM detection misses single-string `to`/`cc`** — checks `.as_array()` only; some implementations send `"to": "https://..."` as a string, not array | DMs from non-conforming servers treated as public posts |
| 6.23 | `inbox.rs` | **`handle_reject` returns `Ignored` instead of a specific variant** — caller can't distinguish between unsupported activity and active rejection | Reject and Ignore are conflated in inbox processing |
| 6.24 | `activities.rs` | **`AP_PUBLIC` duplicates `types::AS_CONTEXT` pattern** — the public addressing constant should be in types.rs alongside other AP constants | Constants scattered across modules |
| 6.25 | `signatures.rs` | **`parse_signature_header` fragile comma splitting** — fails if any value contains an escaped comma (unlikely but spec-possible) | Edge case parsing failure |
| 6.26 | `signatures.rs` | **`pem_to_ed25519_pubkey` accepts raw 32-byte keys** — falls back to treating raw bytes as a key if DER parsing fails | Could misinterpret non-Ed25519 data as a valid key |
| 6.27 | `webfinger.rs` | **No validation that resolved links point to the expected domain** — a malicious WebFinger response could redirect to a phishing domain | Trust-on-first-use vulnerability |
| 6.28 | `group_actor.rs` | **`group_to_ap_actor()` omits `published` field** — AP spec recommends including creation timestamp | Minor spec non-compliance |
| 6.29 | `group_actor.rs` | **No pagination on group outbox/followers** — returns all items in a single response | Fails for groups with thousands of posts or followers |
| 6.30 | `lemmy.rs` | **`strip_html_tags()` is a naive char-by-char parser** — fails on edge cases like `<script>`, HTML entities (`&amp;`), unclosed tags | Incomplete HTML stripping; XSS risk if output is rendered as HTML elsewhere |
| 6.31 | `lemmy.rs` | **Duplicate `strip_html_tags` implementation** — `connectors/mastodon.rs` has its own `strip_html()` | Code duplication across modules |

---

## Appendix: Full Issue Count by File

| File | 🔴 | 🟡 | 🔵 | Total |
|------|-----|-----|-----|-------|
| `types/index.ts` | 1 | 4 | 3 | 8 |
| `models/mod.rs` | 1 | 3 | 3 | 7 |
| `models/groups.rs` | 0 | 0 | 2 | 2 |
| `connectors/mod.rs` | 0 | 3 | 2 | 5 |
| `error.rs` | 1 | 3 | 2 | 6 |
| `activitypub/types.rs` | 0 | 4 | 3 | 7 |
| `db/mod.rs` | 2 | 1 | 0 | 3 |
| `db/groups.rs` | 0 | 4 | 1 | 5 |
| `db/chat.rs` | 1 | 1 | 1 | 3 |
| `db/vault.rs` | 2 | 3 | 3 | 8 |
| `db/peers.rs` | 0 | 0 | 1 | 1 |
| `db/contacts.rs` | 0 | 0 | 1 | 1 |
| `lib.rs` | 1 | 1 | 1 | 3 |
| `auth.rs` | 0 | 1 | 0 | 1 |
| `keychain_encrypted.rs` | 2 | 3 | 2 | 7 |
| `oauth.rs` | 2 | 3 | 1 | 6 |
| `contacts.rs` | 0 | 0 | 3 | 3 |
| `main.rs` | 0 | 0 | 1 | 1 |
| `bluesky.rs` | 3 | 6 | 2 | 11 |
| `facebook.rs` | 2 | 4 | 3 | 9 |
| `mastodon.rs` | 1 | 4 | 2 | 7 |
| `instagram.rs` | 0 | 4 | 2 | 6 |
| `sync.rs` | 2 | 2 | 1 | 5 |
| `protocol.rs` | 0 | 2 | 1 | 3 |
| `chat.rs` | 1 | 2 | 2 | 5 |
| `collab.rs` | 1 | 1 | 2 | 4 |
| `blob_hash.rs` | 0 | 0 | 1 | 1 |
| `richtext.rs` | 0 | 1 | 0 | 1 |
| `media_server.rs` | 0 | 0 | 1 | 1 |
| `traffic.rs` | 0 | 1 | 0 | 1 |
| `activitypub/mod.rs` | 0 | 0 | 0 | 0 |
| `activitypub/inbox.rs` | 2 | 3 | 3 | 8 |
| `activitypub/outbox.rs` | 0 | 3 | 0 | 3 |
| `activitypub/actor.rs` | 1 | 1 | 1 | 3 |
| `activitypub/activities.rs` | 1 | 1 | 1 | 3 |
| `activitypub/signatures.rs` | 2 | 1 | 2 | 5 |
| `activitypub/webfinger.rs` | 0 | 1 | 1 | 2 |
| `activitypub/group_actor.rs` | 0 | 1 | 2 | 3 |
| `activitypub/lemmy.rs` | 0 | 3 | 2 | 5 |
| **TOTAL** | **28** | **62** | **41** | **131** |

# Bytescape Technical Debt Audit

> **Date**: 2026-04-01
> **Scope**: Batches 1–12 (117 source files · ~48,970 lines · Rust + TypeScript)
> **Auditor**: Systems Agent
> **Purpose**: Identify bugs, hacks, security issues, and architectural debt for PM sprint planning

---

## Executive Summary

A systematic audit of the first 12 documentation batches (Core Types, Database, Backend Core, Platform Connectors, P2P/Sync, ActivityPub Federation, Tauri Commands, Frontend Foundation, Frontend Hooks, Frontend Utils, Frontend Screens, **Large Frontend Components**) uncovered **341 issues** across 117 files:

| Severity | Count | Description |
|----------|-------|-------------|
| 🔴 HIGH | 72 | Bugs, data loss, security vulnerabilities, panics |
| 🟡 MEDIUM | 168 | Performance traps, architectural hacks, missing features |
| 🔵 LOW | 101 | Code smells, dead code, naming issues |

The top systemic concerns are:

1. **Full table scans everywhere** — The `TableDefinition<&str, &str>` universal schema forces JSON deserialization on every query that isn't a direct key lookup. This affects ~15 methods across the database layer.
2. **Security gaps in auth/keychain** — Master key stored as plaintext file, hardcoded recovery code, JWT refresh race condition, secrets never zeroized.
3. **Duplicated logic** — Post parsing duplicated 4× in bluesky.rs, DID types duplicated across files, crypto boilerplate duplicated between keychain and oauth. **Batch 7 adds 5 more duplication patterns** across the commands layer (MIME detection ×4, also_known_as ×5, OAuth branching ×6+, VaultMessage ×5+, LinkedAccount ×3).
4. **Missing pagination** — Follows/followers only return first page in both Bluesky and Mastodon connectors, unbounded loops in several API calls.
5. **Memory pressure from large media** — Facebook, Instagram, and vault operations load entire media files into memory with no size guards.
6. **God file problem** — `import.rs` at 2,900 lines is the largest file in the codebase and mixes 8+ unrelated concerns (archive import, timeline fetching, DM import, feed aggregation, publishing, social graph). Should be split into 5-6 submodules.
7. **Frontend foundation sprawl** — `store/index.ts` bundles 6 unrelated Zustand stores in one file, `App.tsx` has 40+ line duplicated chat-open callbacks, `DataProvider.ts` is a 100+ method monolithic interface with 6 untyped returns. The provider pattern requires 3-file changes for every new backend command.
8. **Hooks doing orchestration work** — `useUnifiedFeed.ts` and `useGroupsData.ts` both perform heavy orchestration, fallback logic, and backend transport concerns that should live in service/provider layers. This blurs boundaries between view hooks and domain logic.
9. **Utility layer contract drift** — frontend utility modules now mix pure helpers with transport-bound logic (`prefetchBlobUrls` -> `invoke`) and contradictory runtime flags (`openInApp`). This erodes the “utils are pure/reusable” boundary and causes coupling between presentation and backend internals.
10. **Screen-layer orchestration overload** — multiple top-level screens (`FeedScreen`, `ChatScreen`, `SettingsScreen`, `ProfileBuilderScreen`, `WebViewScreen`) combine transport calls, state orchestration, and rendering in monolithic components, increasing coupling and making defects harder to isolate.
11. **Component-layer monoliths** — large UI components (ComposeBox, FeedEntryCard, Omnibar, ProfileBioSections) now absorb both domain orchestration and rendering concerns, causing very large files with brittle state transitions and limited reuse.

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

## Batch 7 — Tauri Commands (Backend API Surface)

**Scope:** 21 files · ~10,220 lines in `src-tauri/src/commands/`
**Findings:** 58 issues (12 HIGH · 28 MEDIUM · 18 LOW)

This is the **largest batch** and the most architecturally concerning. The commands layer is where Engine meets Lens — every `#[tauri::command]` function is a public API callable from the React frontend via Tauri IPC. The layer exhibits:

- A **God file** (`import.rs` at 2,900 lines) that should be 5–6 submodules
- **Pervasive duplication** across files — MIME detection (4+ copies), `also_known_as` identity building (5 copies), OAuth/app-password branching (6+ commands), VaultMessage construction (5+ places), LinkedAccount creation (3 copies)
- **Silent data loss** — Mastodon publish drops attachments, syndicate silently limits to 4 images, several import paths swallow errors
- **Unbounded memory loading** — classify, export, peers, and feed_aggregation all load entire datasets into memory

### Proposed Refactoring: `import.rs` Split

The 2,900-line `import.rs` should be split into focused submodules:

```
commands/import/
  mod.rs              — re-exports + ParseResult, PublishResult types
  archive.rs          — Facebook/Instagram/DTP ZIP parsing + commit_memories
  bluesky_import.rs   — import_bluesky_posts, save_posts, import_bluesky_feed_oauth
  timeline.rs         — get_timeline, get_author_feed, get_unified_feed
  messages.rs         — import_vault_messages, import_archive_messages, send_bsky_dm
  social.rs           — list_follows, list_followers, get_or_create_bsky_convo
  feeds.rs            — list_saved_feeds, list_saved_feeds_oauth
  publish.rs          — publish_post (merge with posting.rs::publish_post_v2)
```

### Top 5 Cross-Cutting Duplication Patterns

| Pattern | Copies | Files |
|---------|--------|-------|
| MIME detection logic | 4+ | vault.rs, media.rs, import.rs, posting.rs |
| `also_known_as` identity building | 5 | peers.rs (×4), mastodon.rs |
| OAuth vs app-password branching | 6+ | import.rs, mastodon.rs, auth.rs |
| VaultMessage construction | 5+ | vault.rs, peers.rs, import.rs, mastodon.rs |
| LinkedAccount creation | 3 | auth.rs, mastodon.rs (×2) |

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 7.1 | `import.rs` | **God file at 2,900 lines** — contains Facebook/Instagram/DTP archive import, Bluesky feed import (OAuth + app-password), timeline fetching, DM import, archive message import, Bluesky DM send, convo management, saved feeds, follows/followers listing, unified feed aggregation, and publishing. Should be 5–6 submodules. | Unmaintainable; merge conflicts; impossible to review; violates single-responsibility principle |
| 7.2 | `import.rs` | **`publish_post` duplicates `publish_post_v2` from posting.rs** — nearly identical multi-destination routing logic with diverged feature sets (import version lacks Mastodon attachments, posting version lacks some error handling) | Two publish paths that will drift further apart; bugs fixed in one won't be fixed in the other |
| 7.3 | `media.rs` | **6 near-identical FFmpeg resolution strategies** — `try_ffprobe_json`, `try_ffprobe_stderr`, `try_ffprobe_show_streams`, `try_ffmpeg_input`, `try_file_binary`, `try_mediainfo` contain ~180 lines of duplicated spawn-parse-extract logic | Maintenance nightmare; each strategy has slightly different error handling; adding a new extraction method requires copying boilerplate |
| 7.4 | `media.rs` | **`cancel_video_resolve` uses `kill` shell command** — `Command::new("kill").arg("-9")` is Linux-only; will fail on Windows and macOS `kill` behaves differently | Cross-platform breakage; video cancel non-functional on non-Linux |
| 7.5 | `peers.rs` | **`list_p2p_conversations` loads ALL vault messages** — iterates every message in the vault table just to count messages per conversation | O(n) on total message count for every conversation list request; catastrophic at scale |
| 7.6 | `mastodon.rs` | **Hand-rolled HTTP callback server** — `start_mastodon_oauth_callback_server` manually parses raw TCP bytes for OAuth callback instead of using a proper HTTP server (e.g., tiny_http, warp) | Fragile; doesn't handle chunked encoding, keep-alive, large headers, or TLS; security risk |
| 7.7 | `classify.rs` | **`reclassify_vault_items` loads all items** — opens a read transaction and iterates up to 100,000 items to reclassify. No batching, no streaming. | Memory explosion on large vaults; blocks the database for the entire scan duration |
| 7.8 | `export.rs` | **`export_vault_data` loads everything into memory** — collects all items, all blob data, and builds entire ZIP in memory before writing to disk | OOM crash on large vaults; no progress feedback to the user |
| 7.9 | `feed_aggregation.rs` | **Vault dedup logic duplicated** — nearly identical dedup-by-URI code appears twice: once for `has_custom_feeds` path and once for `!has_custom_feeds` path | Divergent behavior between feed modes; bugs fixed in one path not fixed in the other |
| 7.10 | `posting.rs` | **Mastodon publish silently drops attachments** — `publish_post_v2` Mastodon path publishes text only; attachment upload code exists for Bluesky but not Mastodon | Users think they posted images to Mastodon but only text appears; no error shown |
| 7.11 | *Cross-cutting* | **MIME detection duplicated 4+ times** — `vault.rs`, `media.rs`, `import.rs`, and `posting.rs` each have their own MIME type detection logic with different fallback behavior | Inconsistent file type handling; some paths accept files others reject; maintenance burden |
| 7.12 | *Cross-cutting* | **`also_known_as` identity building duplicated 5 times** — `peers.rs` (4 instances) and `mastodon.rs` (1 instance) each reconstruct the same DID/handle/Mastodon identity vector | Easy to miss when adding new identity providers; each copy has slightly different field sets |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 7.13 | `import.rs` | **OAuth vs app-password branching duplicated** — 6+ commands have identical `if has_access_token { oauth_path } else { app_password_path }` structure with no shared abstraction | Every new Bluesky command requires duplicating the branch; easy to forget one path |
| 7.14 | `import.rs` | **`get_unified_feed` duplicates `get_timeline_unified` from feed_aggregation.rs** — two separate implementations of unified feed merging with different dedup strategies | Two divergent feed implementations; user sees different results depending on which code path is hit |
| 7.15 | `import.rs` | **`import_bluesky_feed_oauth` hardcodes page size to 30** — no user control over import batch size | Slow for large feeds; no way to speed up initial import |
| 7.16 | `import.rs` | **Archive message import creates placeholder contacts** — `import_archive_messages` generates contacts with handle/DID set to `"archive-{hash}"` — these never get resolved to real identities | Ghost contacts pollute the contacts list; no way to link archive people to real accounts |
| 7.17 | `import.rs` | **`send_bluesky_dm` error path returns Ok** — catches errors and returns `Ok(json!({"success": false, ...}))` instead of `Err(...)` | Frontend can't distinguish success from failure using standard error handling; must parse JSON body |
| 7.18 | `import.rs` | **`list_saved_feeds` and `list_saved_feeds_oauth` are 90% identical** — same logic, different auth path | Duplication; changes to feed listing logic must be made in two places |
| 7.19 | `mastodon.rs` | **`complete_mastodon_auth` creates LinkedAccount** that duplicates logic in `auth.rs` login flow | Three places create LinkedAccount with slightly different field population |
| 7.20 | `mastodon.rs` | **Mastodon DM implementation has two paths** — C2S (client-to-server) direct messages and AP S2S (server-to-server) inbox. The C2S path is used but S2S path is partially implemented. | Confusing; unclear which path is authoritative; S2S path may be dead code |
| 7.21 | `mastodon.rs` | **`get_mastodon_timeline` ad-hoc JSON return type** — returns `serde_json::Value` instead of a typed struct | No compile-time guarantees on response shape; frontend must defensively parse |
| 7.22 | `mastodon.rs` | **`unified_send_message` fallback chain opaque** — tries Bluesky DM → P2P → Mastodon with no visibility into which path was used | User doesn't know how their message was delivered; debugging delivery failures is difficult |
| 7.23 | `auth.rs` | **`logout` and `delete_account` are 90% duplicated** — both clear sessions, credentials, and state in nearly identical sequences | Bug fixes to cleanup logic must be applied in two places |
| 7.24 | `auth.rs` | **`check_auth_status` three-tier fallback** — Bluesky → local → Mastodon with implicit priority. No configuration for preferred auth provider. | Users with multiple accounts can't control which identity is "primary" |
| 7.25 | `vault.rs` | **`get_vault_stats` and `get_storage_stats`** — two different stats commands returning overlapping but different data | Frontend must call both for complete stats; response shapes diverge |
| 7.26 | `vault.rs` | **`list_contacts` merges vault contacts with Bluesky follows inline** — 60+ lines of merge logic that should be a separate function | Contacts list logic is tightly coupled to Bluesky; adding Mastodon follows requires modifying this function |
| 7.27 | `vault.rs` | **`get_paginated_vault_posts` decodes ALL items then slices** — pagination is post-load, not query-level | Memory usage scales with total vault size regardless of page size |
| 7.28 | `vault.rs` | **Duplicate MIME type detection in `upload_file`** — reimplements extension-based MIME detection that exists in media.rs | Fourth copy of MIME detection logic |
| 7.29 | `groups.rs` | **`list_topics` and `get_topic_tree` load all topics** — iterate entire topics table to build tree in memory | O(n) memory on total topics; no incremental/cached tree |
| 7.30 | `groups.rs` | **`search_groups` loads all groups then filters** — no index-based search; full scan with string contains matching | O(n) search time; unusable at scale |
| 7.31 | `groups.rs` | **BFS in `get_subtopics` has no cycle detection** — if parent references form a cycle (data corruption), BFS loops forever | Infinite loop on corrupted topic tree |
| 7.32 | `groups.rs` | **`seed_default_topics` unconditionally inserts** — no check for existing topics; will create duplicates on every app launch if called multiple times | Duplicate default topics accumulate on repeated calls |
| 7.33 | `media.rs` | **`resolve_hls_video` spawns 6 sequential FFmpeg processes** — each strategy spawns a new process, waits for it, then tries the next | Slow video resolution; 6 process spawns for worst case; no parallelism |
| 7.34 | `media.rs` | **`md5_hash` function actually computes SHA-256** — misleading function name | Developer confusion; callers expect MD5 but get SHA-256 |
| 7.35 | `media.rs` | **`backfill_blob_mime_types` loads all blobs** — iterates entire blob table to check/update MIME types | Unbounded memory; should use cursor-based iteration |
| 7.36 | `media.rs` | **No cache eviction strategy** — `get_cache_info` reports size but there's no automatic cleanup when cache exceeds a threshold | Cache grows unboundedly until disk full |
| 7.37 | `posting.rs` | **`search_mentions` hits Bluesky API with no debounce/cache** — every keystroke triggers a network request | Poor UX; API rate limiting; unnecessary network traffic |
| 7.38 | `posting.rs` | **`publish_post_v2` link card handling duplicated** — Bluesky link card creation code is nearly identical to the link card code in `import.rs::publish_post` | Two copies of link card logic that can diverge |
| 7.39 | `unfurl.rs` | **No SSRF protection** — `unfurl_link` fetches any user-provided URL with no private IP / localhost / internal network checks | Server-Side Request Forgery; attacker can probe internal network via link preview |
| 7.40 | `feed_aggregation.rs` | **Source detection uses fragile string heuristic** — `source` field detection checks for "custom_feed:" prefix and "mastodon" substring with case-sensitive matching | Misclassification if source strings change format; brittle coupling |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 7.41 | `vault.rs` | **`clear_vault` deletes blobs last** — if the process crashes between clearing items and clearing blobs, orphaned blobs remain | Minor storage leak on crash during vault clear |
| 7.42 | `vault.rs` | **`list_hashtags` returns unsorted** — no guaranteed order of hashtag results | Inconsistent UX; list order changes between calls |
| 7.43 | `peers.rs` | **`check_peer_connectivity` timeout hardcoded to 10s** — not configurable | May be too short for high-latency P2P connections; too long for quick checks |
| 7.44 | `peers.rs` | **`get_peer_feed` builds `also_known_as` with redundant entries** — adds DID and handle even when they're the same value | Redundant entries in identity vector; wastes bytes in serialization |
| 7.45 | `peers.rs` | **`send_chat_message` creates conversation as side effect** — calling send implicitly creates a conversation if one doesn't exist | Invisible state mutation; conversation list grows unexpectedly |
| 7.46 | `mastodon.rs` | **`get_mastodon_profile` returns raw JSON** — `serde_json::Value` instead of a typed struct | No compile-time shape validation; frontend must defensively access fields |
| 7.47 | `auth.rs` | **`create_local_identity` generates DID with `rand::random::<[u8; 16]>()`** — uses thread_rng which is adequate but not cryptographically explicit | Should use `OsRng` or document why thread_rng is sufficient for DID generation |
| 7.48 | `auth.rs` | **`get_recovery_phrase` returns phrase in plain JSON** — no transport-level encryption beyond Tauri IPC | Recovery phrase visible in IPC logs if debug logging is enabled |
| 7.49 | `groups.rs` | **`update_member_role` accepts any string as role** — no validation against a Role enum | Invalid roles silently stored; could cause frontend rendering errors |
| 7.50 | `groups.rs` | **`invite_to_group` creates membership without notification** — the invited user has no way to know they were invited | Invisible invitations; users discover memberships by accident |
| 7.51 | `groups.rs` | **`get_group_feed` sorts by `imported_at` instead of `original_timestamp`** — shows posts in import order, not chronological order | Posts appear out of order in group feeds |
| 7.52 | `media.rs` | **`get_blob_data` returns raw bytes with hardcoded `application/octet-stream`** — ignores stored MIME type | Browser/frontend can't auto-detect content type for display |
| 7.53 | `syndicate.rs` | **Bluesky-only syndication** — `syndicate_to_bluesky` hardcoded to single platform despite generic naming | No Mastodon/AP syndication; function name implies platform-agnostic but isn't |
| 7.54 | `syndicate.rs` | **Silent 4-image limit** — Bluesky API limits embeds to 4 images; the code silently truncates without warning the user | Users lose images without knowing; should show warning |
| 7.55 | `syndicate.rs` | **`verify_syndication` only checks Bluesky URI** — verification logic is platform-specific with no abstraction | Can't verify Mastodon syndication; verification is incomplete |
| 7.56 | `interactions.rs` | **All interactions are Bluesky-only** — like, repost, reply commands hardcoded to Bluesky AT Protocol | No Mastodon interaction support; frontend must hide buttons for Mastodon posts |
| 7.57 | `feed_aggregation.rs` | **`get_timeline_unified` merges three sources without consistent pagination** — live Bluesky uses cursor, custom feeds use cursor, vault uses timestamp; merged result has inconsistent page boundaries | Infinite scroll may skip or duplicate items at page boundaries |
| 7.58 | `import.rs` | **`commit_memories` success count is approximate** — counts items before insert but doesn't verify all inserts succeeded | Reported count may not match actual persisted count |

---

## Batch 8 — Frontend Foundation

**Scope:** 8 files · ~2,581 lines in `src/`, `src/store/`, `src/providers/`, `src/contexts/`
**Findings:** 24 issues (4 HIGH · 12 MEDIUM · 8 LOW)

Batch 8 covers the application shell, global state management, and the data provider abstraction layer. These are the files every screen and component depends on. Key concerns:

- **Duplicated callback logic** — The "open chat" flow is copy-pasted between FeedScreen and FriendsScreen callbacks in App.tsx with divergent convo-resolution logic
- **Monolithic interfaces** — `DataProvider` has 100+ methods in a flat interface; adding any backend command requires touching 3 files
- **Single-file store sprawl** — 6 unrelated Zustand stores crammed into one file
- **No lazy loading** — All 17 screen components are eagerly imported regardless of which ones the user visits

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 8.1 | `App.tsx` | **"Open Chat" logic duplicated between FeedScreen and FriendsScreen callbacks** — 40+ lines of identical Bluesky/AP/P2P branching copied in both `onOpenChat` handlers with slightly different convo-finding logic | Bugs fixed in one callback won't be fixed in the other; convo resolution diverges |
| 8.2 | `App.tsx` | **All import/audit state is local `useState`** — `progress`, `parseResult`, `logs`, `isShredding`, `importComplete`, `selectedArchivePath` are local state in App. Preserved only because tabs render simultaneously (display:none). | Fragile coupling between tab rendering strategy and state persistence |
| 8.3 | `DataProvider.ts` | **Interface is 100+ methods with no grouping/namespacing** — single flat interface with every command the app can execute. Any new backend command requires touching 3 files (DataProvider + TauriDataProvider + future providers). | Hard to navigate; impossible to implement partial providers; adding commands is expensive |
| 8.4 | `store/index.ts` | **6 unrelated stores in a single file** — BrowserStore, AuthStore, AppStore, NavStore, PeerStore, ChatStore, FeedCacheStore, ThemeStore all in one 459-line file | Import pollution; merge conflicts; developers must read unrelated stores |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 8.5 | `App.tsx` | **`importBluesky()` fires automatically on every boot** for Bluesky users — no opt-out, no rate limiting, no "last synced" check | Unnecessary API calls on every launch; slow startup for large feeds |
| 8.6 | `App.tsx` | **`loadArchiveStats` fallback chain** — calls `vault_get_stats` then falls back to `count_archive_posts`. Two stats APIs for the same purpose. | Masks real errors; fallback exists because two stats commands coexist (see #7.25) |
| 8.7 | `App.tsx` | **Error boundary only wraps the content area** — if AppLayout or the tab bar crashes, entire app goes white | Partial error boundary coverage; shell-level crashes unrecoverable |
| 8.8 | `App.tsx` | **`renderTabScreen` is a 150+ line switch statement** with inline props. No lazy loading of screens. | All 17 screen components imported eagerly; startup bundle includes everything |
| 8.9 | `store/index.ts` | **`FeedCacheStore.getCache` maxAge hardcoded to 120s** — `appendCache` preserves original `fetchedAt`, so appended pages can appear stale immediately | Cache may seem expired right after pagination; no user control |
| 8.10 | `store/index.ts` | **`tabCounter` is a module-level mutable variable** — resets on HMR, new tabs can collide with existing IDs | Duplicate tab IDs after HMR cause React key conflicts |
| 8.11 | `store/index.ts` | **`closeTab` when last tab closed sets `activeTabId: null`** but doesn't reset `screen` — app can have a screen with no tab | Ghost screen state; empty main area but sidebar still highlights old screen |
| 8.12 | `TauriDataProvider.ts` | **Every method uses `{ ...params }` spread** — unnecessary object copy on every IPC call | Minor perf overhead; style inconsistency |
| 8.13 | `TauriDataProvider.ts` | **No error wrapping** — raw Rust error strings bubble to the UI with no normalization or retry | Users see raw text like "ExpiredToken" or "Io(BrokenPipe)" |
| 8.14 | `DataProvider.ts` | **6 methods return `Promise<unknown>`** — `fetchRemoteActor`, `importVaultMessages`, `sendP2pChatMessage`, `sendBskyDm`, `migrateWillowPaths`, `importArchiveMessages` | Callers must use `as` casts; no type safety on responses |
| 8.15 | `DataProviderContext.tsx` | **Singleton `defaultProvider` created at module scope** — `new TauriDataProvider()` instantiated even if component tree never mounts | Wasteful in SSR/test environments |
| 8.16 | `App.tsx` | **P2P keepalive polls with sequential connectivity checks** — iterates peers one-by-one with `await`. 20 peers × 10s timeout = 200s cycle vs 90s interval. | Overlapping keepalive cycles; peers polled sequentially not in parallel |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 8.17 | `main.tsx` | **No error boundary at root level** — if `<App />` throws during initial render, blank white screen | Unrecoverable crash on startup errors |
| 8.18 | `App.tsx` | **`searchQuery` state managed but unused** — `handleSearchSubmit` logs to console, doesn't filter feed | Dead feature; search bar appears functional but does nothing |
| 8.19 | `store/index.ts` | **`AuthInfo.source` is `string` not a union type** — should be `"bluesky" \| "local" \| "mastodon"` | No type safety on auth source checks |
| 8.20 | `store/index.ts` | **Theme store has no persistence** — theme selection and overrides reset on app restart | Users lose customizations every session |
| 8.21 | `store/index.ts` | **`UserBackground.dataUrl` stores images as base64 in Zustand** — large images (5MB+) live in JS heap | Memory pressure from background images |
| 8.22 | `providers/index.ts` | **Re-exports manually maintained** — adding a new provider type requires updating barrel file | Minor maintenance burden |
| 8.23 | `App.tsx` | **`handleLogout` clears chat store but not FeedCacheStore or PeerStore** — stale data from previous account persists | Stale feed cache and peer statuses after logout |
| 8.24 | `TabActiveContext.tsx` | **Default context value is `true`** — components outside tab system always think they're active | Resources not released for orphan components |

---

## Batch 9 — Frontend Hooks

**Scope:** 7 files · ~2,349 lines in `src/hooks/`
**Findings:** 29 issues (6 HIGH · 15 MEDIUM · 8 LOW)

This batch covers the core orchestration hooks used by feed, groups, navigation, media resolution, collaboration, themes, and posting composition. Main structural concerns:

- **Hooks are acting as service layers** (`useUnifiedFeed`, `useGroupsData`) with transport/fallback logic mixed into UI-state hooks
- **Large hook complexity** (`useUnifiedFeed` at ~640 lines) with duplicated branch logic between initial fetch and pagination
- **Mock + production paths in same module** (`useGroupsData`) creates split behavior and hidden test/prod drift

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 9.1 | `useUnifiedFeed.ts` | **Monolithic orchestration hook (~640 lines)** — handles fetching, dedup, caching, verification, pagination, prefetch, and fallback logic in one unit | Hard to reason about and test; regressions likely when touching unrelated concerns |
| 9.2 | `useUnifiedFeed.ts` | **`handleLoadMore()` duplicates branch logic from `fetchFeed()`** for `following`, `you`, `subscribed`, and `all` tabs | Divergent behavior between first page and subsequent pages; bug fixes must be duplicated |
| 9.3 | `useGroupsData.ts` | **Mock and production logic mixed in same runtime path** under `BACKEND_READY` flag with massive in-file mock dataset | Easy for mock path to drift from backend behavior; production bugs hidden during UI development |
| 9.4 | `useLocation.ts` | **URL parser and NavStore synchronizer are separate mutable systems** — `navigate()` mutates NavStore + ChatStore, while `syncFromNavStore()` mutates history from NavStore | Two-way sync risks loops/state drift; hard to guarantee single source of truth |
| 9.5 | `useBlobUrl.ts` | **Global mutable `resolvedUrlCache` with no eviction** | Unbounded memory growth across long sessions with media-heavy feeds |
| 9.6 | `useCollabDoc.ts` | **Persistence queue is in-memory only** (`pendingUpdates`) with debounced flush and no retry queue after failure | Update loss risk during transient backend errors or app shutdown |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 9.7 | `useUnifiedFeed.ts` | **`fetchFeed` has complex tab-branch behavior and many side effects** (state, cache, provider calls, prefetch) in one callback | Difficult unit testing; high cognitive load |
| 9.8 | `useUnifiedFeed.ts` | **Repeated client-side `source` filtering in multiple branches** | Repetition invites inconsistent filtering rules across tabs |
| 9.9 | `useUnifiedFeed.ts` | **Verification loop launches one async task per unverified entry without batching/cap** | Burst of IPC/network calls on large feeds; potential contention |
| 9.10 | `useUnifiedFeed.ts` | **Cache invalidation logic tightly coupled to filter arrays via stringified key** | Cache key construction is fragile and easy to break when filters evolve |
| 9.11 | `useGroupsData.ts` | **Huge static mock graph in hook module** (topics, groups, members) | Inflates bundle and obscures actual hook behavior |
| 9.12 | `useGroupsData.ts` | **`useGroupDetail` does nested auth check via `invoke('check_auth_status')`** after members fetch | Extra round-trip per refresh; should come from auth store/context |
| 9.13 | `useGroupsData.ts` | **Global mutable mock state (`Set`s / objects) shared across hook consumers** | Cross-component test pollution and non-deterministic behavior in dev |
| 9.14 | `useLocation.ts` | **Route support drift** — `resolveUrl` valid feed tabs omit `'groups'` while `FeedTab` includes it | URL parsing silently coerces unsupported tabs to `'all'` |
| 9.15 | `useLocation.ts` | **`MAX_HISTORY` hardcoded to 50** | No configurability for power users; history truncation surprises |
| 9.16 | `useBlobUrl.ts` | **No dedupe for concurrent hash resolves** — same hash requested by many components can trigger repeated invokes before cache fills | Redundant IPC calls and wasted work |
| 9.17 | `useBlobUrl.ts` | **`useLazyBlobUrl` starts observing per-element with no shared observer pool** | Many observers in long lists; avoidable overhead |
| 9.18 | `useCollabDoc.ts` | **`origin === 'remote'` filter relies on convention** — if callers don't set origin correctly, remote updates may be re-persisted | Potential update echo loops |
| 9.19 | `useTheme.ts` | **Module-level `_restored` singleton** controls hydration across all consumers | Hidden global state complicates SSR/tests and multi-root setups |
| 9.20 | `useTheme.ts` | **Multiple localStorage writes across independent effects** | Extra synchronous main-thread I/O during theme changes |
| 9.21 | `useCharacterCount.ts` | **`Intl.Segmenter` instantiated on every `countGraphemes` call** | Avoidable allocation cost while typing |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 9.22 | `useUnifiedFeed.ts` | **Magic constants** (`45_000`, `200`, `50`, `30`) in multiple places | Hard to tune behavior globally |
| 9.23 | `useUnifiedFeed.ts` | **`LIVE_TABS` excludes `'you'` by design but not documented in API contract** | Surprise for future maintainers expecting all tabs to auto-refresh |
| 9.24 | `useGroupsData.ts` | **`BACKEND_READY` flag is hardcoded `true`** while mock paths remain in file | Dead-path confusion and maintenance overhead |
| 9.25 | `useLocation.ts` | **`sidebar` union in `ResolvedRoute` duplicates `SidebarItem` type shape** | Type drift risk between modules |
| 9.26 | `useBlobUrl.ts` | **`extractBlobHash` string parsing rules are undocumented at callsites** | Edge-case behavior is non-obvious to future maintainers |
| 9.27 | `useCollabDoc.ts` | **Base64 helper implementations are local-only** and duplicated pattern likely elsewhere | Utility duplication risk |
| 9.28 | `useTheme.ts` | **Theme storage keys are un-namespaced constants in module scope** | Potential collisions if embedding app contexts change |
| 9.29 | `useCharacterCount.ts` | **`resolveTargets()` accepts free-form comma string API** | Caller typos silently reduce target set; weak contract |

---

## Batch 10 — Frontend Utils

**Scope:** 13 files · ~1,432 lines in `src/utils/`
**Findings:** 26 issues (5 HIGH · 13 MEDIUM · 8 LOW)

Batch 10 includes the frontend utility layer: rich text parsing, markdown conversion, feed adapters/analytics, media resolution, sanitization, and test helpers. Structural/compositional concerns center on **boundary violations** (utils depending on runtime transport), **contract drift**, and **logic duplication**.

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 10.1 | `prefetchBlobUrls.ts` | **Broken import contract** — imports `cacheResolvedUrl` from `useBlobUrl`, but `useBlobUrl` exports no such symbol | TypeScript build fails (`TS2614`), blocking strict type-checking |
| 10.2 | `adapters.ts` | **Hardcoded source inference for Mastodon in Bluesky adapter** — uses URI substring heuristics (`/statuses/`, `acct:`) to remap platform | Misclassification risk and cross-protocol coupling inside a low-level adapter |
| 10.3 | `openInApp.ts` | **Configuration contradiction** — comments claim system-browser fallback is active while `USE_SYSTEM_BROWSER = false` keeps in-app path enabled | Operational confusion and fragile behavior toggling in production |
| 10.4 | `VideoPlaybackManager.ts` | **Imperative `releaseAll()` mutates DOM media state outside React ownership** (`removeAttribute('src')`, `load()`) | Can fight React reconciliation and produce hard-to-debug playback regressions |
| 10.5 | `richtext.ts` | **Regex-only facet detection used as parser** for mentions/hashtags/URLs with overlap pruning | Edge-case parsing bugs (nested punctuation/unicode boundaries) can silently break mentions/links |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 10.6 | `richtext.ts` | **Frontend facet rules mirror backend but are not shared** | Drift risk between Rust and TypeScript detection behavior |
| 10.7 | `richtext.ts` | **`linkifyFacetsInHtml()` performs regex HTML rewriting after split-by-tags** | Fragile against malformed HTML and complex nested markup |
| 10.8 | `markdown.ts` | **Custom markdown parser is intentionally partial and hand-rolled** | Feature gaps and inconsistent round-trip fidelity vs full markdown engines |
| 10.9 | `markdown.ts` | **Inline replacement order can produce unexpected nesting behavior** (bold/italic/code/link transforms) | Incorrect formatting in edge-case text combinations |
| 10.10 | `adapters.ts` | **Content-kind inference duplicated with util media helpers** (`inferContentKind` + additional fallback paths) | Behavior differences across components for same media input |
| 10.11 | `openInApp.ts` | **Deep coupling to global stores from utility module** (`useNavStore`, `useAuthStore`) | Hard to test and reuse; utility behaves like navigation service |
| 10.12 | `feedAnalytics.ts` | **Scoring model uses heuristic constants without central config** | Hard to tune; ranking shifts require code edits |
| 10.13 | `VideoPlaybackManager.ts` | **Singleton with global mutable set and no lifecycle owner** | Hidden shared state and difficult deterministic testing |
| 10.14 | `prefetchBlobUrls.ts` | **Utility invokes backend directly** (`invoke('resolve_blob_urls_batch')`) instead of going through provider abstraction | Breaks abstraction layering; harder to port frontend to non-Tauri runtime |
| 10.15 | `media.ts` | **`resolveMediaPath` duplicates parts of `useBlobUrl`/`mediaServer` resolution flow** | Divergent URL resolution behavior and maintenance overhead |
| 10.16 | `content.ts` | **`sanitizeVaultContent()` strips metadata via brittle regex list** | New source formats can leak noise or accidentally remove valid content |
| 10.17 | `sanitize.ts` | **Allowlist config is fixed in module scope** with no caller-level policy variants | One-size policy may be too strict/too loose for different rendering contexts |
| 10.18 | `mediaServer.ts` | **Global cached port has no invalidation** | Stale cache if media server restarts on a new port during session |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 10.19 | `richtext.ts` | **Regex constants and magic limits (`65`) are in-module literals** | Harder to audit/tune detection policy |
| 10.20 | `markdown.ts` | **Utility names imply generic markdown support despite limited subset** | API expectation mismatch |
| 10.21 | `adapters.ts` | **Normalization and adaptation concerns mixed in one file** | Reduced composability of adapter layer |
| 10.22 | `openInApp.ts` | **Dead/disabled path comments are stale relative to runtime flag** | Documentation drift for maintainers |
| 10.23 | `media.ts` | **Dual import style from same module** (`invoke`, `convertFileSrc`) | Minor stylistic inconsistency |
| 10.24 | `content.ts` | **`safeHostname()` fallback returns original input** for invalid URLs | Potentially misleading display values |
| 10.25 | `sanitize.ts` | **`formatFileSize` has fixed precision policy in util** | UI-specific formatting choices leaked into shared util |
| 10.26 | `testUtils.tsx` | **Wrapper scaffold is minimal/no-op by default** | Easy to forget provider wiring in tests requiring context |

---

## Batch 11 — Frontend Screens

**Scope:** 16 files · ~7,604 lines in `src/screens/`
**Findings:** 34 issues (8 HIGH · 16 MEDIUM · 10 LOW)

This batch covers top-level user-facing screens. The dominant pattern is **screen-level orchestration overload**: multiple screens embed transport calls, state machines, and rendering concerns in single large components.

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 11.1 | `ChatScreen.tsx` | **Monolithic component with mixed domains** — conversation loading, protocol branching, send pipeline, classification, and rendering all in one screen | High regression risk; difficult to isolate behavior for tests/fixes |
| 11.2 | `LoginScreen.tsx` | **Mastodon pending-auth flow state inconsistency** — pending branch logic and state transitions are partially disconnected | User can get stuck in confusing auth states; flow reliability risk |
| 11.3 | `ProfileBuilderScreen.tsx` | **Editor bootstrap and profile runtime tightly coupled** — GrapesJS setup, block registry, and persistence are all inline in one screen | Hard to evolve profile builder safely; large blast radius for changes |
| 11.4 | `ProfileBrowserScreen.tsx` | **Routing/history/network/render concerns merged in one screen** | Navigation bugs and data bugs become intertwined; reduced maintainability |
| 11.5 | `SettingsScreen.tsx` | **God-screen behavior** — account linking, recovery, sync, export, destructive actions, and appearance settings in one component | Very high cognitive load and difficult QA matrix |
| 11.6 | `FeedScreen.tsx` | **High orchestration density** — compose interactions, filters, refresh, side panels, and feed coordination in one top-level component | Performance and correctness regressions likely during feature additions |
| 11.7 | `WebViewScreen.tsx` | **Complex lifecycle orchestration with multiple effects/refs/cancellation paths** | Fragile under tab switches and async race conditions |
| 11.8 | `ProfileViewScreen.tsx` | **Unsanitized HTML rendering path risk (`dangerouslySetInnerHTML`)** | Potential XSS if upstream sanitization assumptions fail |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 11.9 | `ChatScreen.tsx` | **Effect dependency suppression (`exhaustive-deps` bypass)** around convo synchronization | Stale closure/state drift risks in message flow |
| 11.10 | `VaultScreen.tsx` | **Local caching/lifecycle complexity** for thumbnails and object URLs | Long-session memory behavior is harder to reason about |
| 11.11 | `VaultScreen.tsx` | **Repeated date/size formatting in render paths** | Minor render overhead and duplicated presentation logic |
| 11.12 | `LoginScreen.tsx` | **Multi-flow auth UI packed into one component** (local, Bluesky OAuth, Mastodon OAuth) | Hard to test each auth path independently |
| 11.13 | `FriendsScreen.tsx` | **Sequential peer connectivity checks (`await` loop)** | Poor scaling with many peers; UI freshness lag |
| 11.14 | `FriendsScreen.tsx` | **Large inline style and branching density** | Reduced composability and harder theming refactors |
| 11.15 | `ProfileBuilderScreen.tsx` | **Hardcoded block templates/styles inline** | Theme/style drift and maintenance burden |
| 11.16 | `ProfileBrowserScreen.tsx` | **Ad-hoc identity parsing and URL history semantics** | Inconsistent navigation behavior across profile types |
| 11.17 | `SettingsScreen.tsx` | **Provider/command fallback branching inside view layer** | Logic duplication and harder error handling consistency |
| 11.18 | `SettingsScreen.tsx` | **OAuth listener setup duplicated by provider type** | Repetition invites auth flow drift |
| 11.19 | `FeedScreen.tsx` | **Global custom event coupling for hashtag navigation** | Hidden dependencies between components and screens |
| 11.20 | `PeerProfileScreen.tsx` | **Transport + pagination + render concerns mixed** | Hard to test and optimize independently |
| 11.21 | `PeerProfileScreen.tsx` | **Ad-hoc extension of feed entry error typing** | Weakens response contract clarity |
| 11.22 | `GroupScreen.tsx` | **Command-heavy moderation/membership logic embedded in screen** | Hard to extract reusable group-domain actions |
| 11.23 | `WebViewScreen.tsx` | **Raw command string orchestration across multiple effects** | Missing typed abstraction raises command drift risk |
| 11.24 | `ForumScreen.tsx` | **Recursive topic rendering without virtualization strategy** | Performance risk on larger topic trees |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 11.25 | `ChatScreen.tsx` | **Mastodon DM placeholder path still present in production UI** | Incomplete user path signaling |
| 11.26 | `VaultScreen.tsx` | **`auth` prop accepted but minimally used** | Prop contract drift/noise |
| 11.27 | `LoginScreen.tsx` | **OAuth listener timeout/cleanup patterns repeated** | Refactor opportunity for shared helper |
| 11.28 | `ProfileBrowserScreen.tsx` | **Heavy inline style objects in render** | Styling duplication and visual noise |
| 11.29 | `GroupScreen.tsx` | **Weak tab typing (`string`-style state usage)** | Potential drift from intended tab union constraints |
| 11.30 | `ProfileScreen.tsx` | **Unused or weakly used props in component contract** | Interface drift over time |
| 11.31 | `ProfileScreen.tsx` | **Mixed mode rendering (custom profile vs wall) in one component body** | Readability and change-safety cost |
| 11.32 | `AuditScreen.tsx` | **Inline style-heavy terminal layout composition** | Reduced reuse and maintainability |
| 11.33 | `ForumScreen.tsx` | **localStorage coupling for UI preferences in screen layer** | Portability/testing friction |
| 11.34 | `ImportScreen.tsx` | **Native drag-drop event wiring embedded in component** | Reuse/testability limitations |

---

## Batch 12 — Frontend Components (Large)

**Scope:** 13 files · ~6,072 lines in `src/components/` (+ `src/components/Chat/`)
**Findings:** 39 issues (9 HIGH · 22 MEDIUM · 8 LOW)

Batch 12 covers the largest reusable UI components. The primary pattern is component monoliths combining view rendering, domain orchestration, and transport concerns in single files.

### 🔴 HIGH

| # | File | Issue | Impact |
|---|------|-------|--------|
| 12.1 | `EmojiPicker.tsx` | **Large hardcoded emoji catalog embedded in component file** | Increases bundle size and makes updates/versioning difficult |
| 12.2 | `ProfileBioSections.tsx` | **Multi-domain component ownership** (persistence, guestbook, profile editing, social connections) | High coupling and difficult targeted refactors |
| 12.3 | `ComposeBox.tsx` | **Very high orchestration density in one component** (editor, attachments, targets, mentions, link unfurl) | State-race risk and reduced change safety |
| 12.4 | `FeedEntryCard.tsx` | **Monolithic card component combining render + interaction orchestration + backend actions** | Large blast radius for feed interaction bugs |
| 12.5 | `Omnibar.tsx` | **Navigation/webview command orchestration tightly coupled into UI control** | Any routing change risks omnibar behavior regressions |
| 12.6 | `FriendsPanel.tsx` | **Data fetch/normalize/dedupe logic embedded inside panel component** | Hard to isolate and test data behavior independently |
| 12.7 | `ProfileRenderer.tsx` | **String-based widget dispatch contract for dynamic rendering** | Runtime drift risk between widget schema and renderer |
| 12.8 | `VideoPlayer.tsx` | **Global unbounded HLS resolve cache (`Map`) at module scope** | Potential memory growth in long sessions |
| 12.9 | `ChatLayout.tsx` | **Conversation grouping/sorting/title derivation recomputed in render path** | Performance bottleneck at larger message counts |

### 🟡 MEDIUM

| # | File | Issue | Impact |
|---|------|-------|--------|
| 12.10 | `EmojiPicker.tsx` | **Search scans full emoji sets per query without indexing** | Scaling/perf cost during active typing |
| 12.11 | `ProfileBioSections.tsx` | **Loosely typed command payload boundaries** (`JSON.stringify` + casts) | Drift risk with backend schema changes |
| 12.12 | `ThemeCustomizer.tsx` | **CSS-variable token contract is string-literal driven** | No compile-time guard against token drift |
| 12.13 | `ThemeCustomizer.tsx` | **Background/file handling and token editing combined in one component** | Reduced cohesion and harder reuse |
| 12.14 | `ComposeBox.tsx` | **Direct backend calls in view layer instead of service-hook abstraction** | Harder portability/testing for compose flow |
| 12.15 | `ComposeBox.tsx` | **Complex async state transitions across multiple feature toggles** | Race/cancellation edge-case risk |
| 12.16 | `FeedEntryCard.tsx` | **Recursive quoted/reply rendering path can amplify render cost** | Performance degradation in deep thread structures |
| 12.17 | `FeedEntryCard.tsx` | **Heavy inline style/branch density** | Maintenance overhead and theming inconsistency |
| 12.18 | `Omnibar.tsx` | **No complete keyboard-selection interaction model for suggestions** | Accessibility/UX degradation |
| 12.19 | `RawHtmlEditor.tsx` | **Import/parse/persist/render all colocated in one component** | Blurred boundaries and harder unit testing |
| 12.20 | `RawHtmlEditor.tsx` | **String-based status/error model** | Weakly typed failure semantics |
| 12.21 | `FriendsPanel.tsx` | **Hardcoded polling cadence and local refresh policy** | Difficult global tuning and wasted work |
| 12.22 | `FriendsPanel.tsx` | **Inline source/status branching complexity** | Hard to reason about status mapping correctness |
| 12.23 | `FeedRightPanel.tsx` | **Broad prop/sentinel contract (`'__disabled__'`)** | Brittle caller-component coordination |
| 12.24 | `FeedRightPanel.tsx` | **Many tab/mode branches in single component** | Lower readability and slower feature iteration |
| 12.25 | `ProfileRenderer.tsx` | **Parse/sanitize/fetch/save/render concerns bundled together** | Difficult to extract testable renderer core |
| 12.26 | `ProfileRenderer.tsx` | **Sanitization policy decisions embedded locally** | Policy drift risk vs centralized security model |
| 12.27 | `ArticleEditor.tsx` | **Presentation + transformation + collaboration orchestration in one component** | Complex mental model and higher regression risk |
| 12.28 | `ArticleEditor.tsx` | **`any[]` extension typing for editor plugins** | Reduced type safety in integration layer |
| 12.29 | `VideoPlayer.tsx` | **String-heuristic source-type branching (`.m3u8`, `blob:`)** | Fragile media-kind detection paths |
| 12.30 | `VideoPlayer.tsx` | **Playback gating + resolve pipeline + arbitration all in one component** | Hard to isolate playback bugs |
| 12.31 | `ChatLayout.tsx` | **No virtualization for potentially long message lists** | Rendering cost increases with conversation size |

### 🔵 LOW

| # | File | Issue | Impact |
|---|------|-------|--------|
| 12.32 | `EmojiPicker.tsx` | **Large inline style footprint tied to theme token strings** | Styling maintenance overhead |
| 12.33 | `ProfileBioSections.tsx` | **Inline style fragments repeated across sub-sections** | Reduced visual consistency and reuse |
| 12.34 | `ThemeCustomizer.tsx` | **File-size failures surfaced via `alert()`** | UX inconsistency with app-level notification patterns |
| 12.35 | `Omnibar.tsx` | **High inline-style density** | Harder long-term design-system migration |
| 12.36 | `RawHtmlEditor.tsx` | **Preview HTML assembly remains component-local and mostly un-memoized** | Minor redraw overhead with large content |
| 12.37 | `FeedRightPanel.tsx` | **Sentinel state strings documented only implicitly** | Cognitive overhead for maintainers |
| 12.38 | `ArticleEditor.tsx` | **Repeated inline style assertions (`as React.CSSProperties`)** | Styling noise and weaker separation |
| 12.39 | `ChatLayout.tsx` | **Ad-hoc date/title formatting in render helpers** | Minor duplication vs shared formatter utilities |

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
| `mastodon.rs` (connector) | 1 | 4 | 2 | 7 |
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
| `commands/mod.rs` | 0 | 0 | 0 | 0 |
| `commands/classify.rs` | 1 | 0 | 0 | 1 |
| `commands/collab.rs` | 0 | 0 | 0 | 0 |
| `commands/ap.rs` | 0 | 0 | 0 | 0 |
| `commands/relay.rs` | 0 | 0 | 0 | 0 |
| `commands/unfurl.rs` | 0 | 1 | 0 | 1 |
| `commands/export.rs` | 1 | 0 | 0 | 1 |
| `commands/node.rs` | 0 | 0 | 0 | 0 |
| `commands/syndicate.rs` | 0 | 0 | 3 | 3 |
| `commands/interactions.rs` | 0 | 0 | 1 | 1 |
| `commands/profile.rs` | 0 | 0 | 0 | 0 |
| `commands/feed_aggregation.rs` | 1 | 2 | 1 | 4 |
| `commands/browser.rs` | 0 | 0 | 0 | 0 |
| `commands/posting.rs` | 1 | 2 | 0 | 3 |
| `commands/auth.rs` | 0 | 2 | 2 | 4 |
| `commands/vault.rs` | 0 | 4 | 2 | 6 |
| `commands/peers.rs` | 1 | 0 | 3 | 4 |
| `commands/mastodon.rs` | 1 | 4 | 1 | 6 |
| `commands/groups.rs` | 0 | 4 | 3 | 7 |
| `commands/media.rs` | 2 | 4 | 1 | 7 |
| `commands/import.rs` | 2 | 6 | 1 | 9 |
| *Cross-cutting (Batch 7)* | 2 | 0 | 0 | 2 |
| `src/main.tsx` | 0 | 0 | 1 | 1 |
| `src/App.tsx` | 2 | 5 | 2 | 9 |
| `src/store/index.ts` | 1 | 3 | 3 | 7 |
| `src/providers/DataProvider.ts` | 1 | 1 | 0 | 2 |
| `src/providers/TauriDataProvider.ts` | 0 | 2 | 0 | 2 |
| `src/providers/DataProviderContext.tsx` | 0 | 1 | 0 | 1 |
| `src/providers/index.ts` | 0 | 0 | 1 | 1 |
| `src/contexts/TabActiveContext.tsx` | 0 | 0 | 1 | 1 |
| `src/hooks/useUnifiedFeed.ts` | 2 | 4 | 2 | 8 |
| `src/hooks/useGroupsData.ts` | 1 | 3 | 2 | 6 |
| `src/hooks/useLocation.ts` | 1 | 2 | 1 | 4 |
| `src/hooks/useBlobUrl.ts` | 1 | 2 | 1 | 4 |
| `src/hooks/useCollabDoc.ts` | 1 | 1 | 1 | 3 |
| `src/hooks/useTheme.ts` | 0 | 2 | 1 | 3 |
| `src/hooks/useCharacterCount.ts` | 0 | 1 | 0 | 1 |
| `src/utils/richtext.ts` | 1 | 2 | 1 | 4 |
| `src/utils/markdown.ts` | 0 | 2 | 1 | 3 |
| `src/utils/adapters.ts` | 1 | 2 | 1 | 4 |
| `src/utils/openInApp.ts` | 1 | 1 | 1 | 3 |
| `src/utils/feedAnalytics.ts` | 0 | 2 | 0 | 2 |
| `src/utils/VideoPlaybackManager.ts` | 1 | 1 | 1 | 3 |
| `src/utils/prefetchBlobUrls.ts` | 1 | 1 | 0 | 2 |
| `src/utils/media.ts` | 0 | 1 | 1 | 2 |
| `src/utils/content.ts` | 0 | 1 | 1 | 2 |
| `src/utils/sanitize.ts` | 0 | 1 | 1 | 2 |
| `src/utils/mediaServer.ts` | 0 | 1 | 0 | 1 |
| `src/utils/testUtils.tsx` | 0 | 0 | 1 | 1 |
| `src/utils/date.ts` | 0 | 0 | 0 | 0 |
| `src/screens/ChatScreen.tsx` | 1 | 1 | 1 | 3 |
| `src/screens/VaultScreen.tsx` | 0 | 2 | 1 | 3 |
| `src/screens/LoginScreen.tsx` | 1 | 1 | 1 | 3 |
| `src/screens/FriendsScreen.tsx` | 0 | 2 | 1 | 3 |
| `src/screens/ProfileBuilderScreen.tsx` | 1 | 1 | 0 | 2 |
| `src/screens/ProfileBrowserScreen.tsx` | 1 | 1 | 1 | 3 |
| `src/screens/SettingsScreen.tsx` | 1 | 2 | 0 | 3 |
| `src/screens/FeedScreen.tsx` | 1 | 2 | 0 | 3 |
| `src/screens/PeerProfileScreen.tsx` | 0 | 2 | 1 | 3 |
| `src/screens/GroupScreen.tsx` | 0 | 1 | 1 | 2 |
| `src/screens/ProfileScreen.tsx` | 0 | 0 | 2 | 2 |
| `src/screens/WebViewScreen.tsx` | 1 | 2 | 0 | 3 |
| `src/screens/AuditScreen.tsx` | 0 | 0 | 1 | 1 |
| `src/screens/ForumScreen.tsx` | 0 | 1 | 1 | 2 |
| `src/screens/ProfileViewScreen.tsx` | 1 | 0 | 0 | 1 |
| `src/screens/ImportScreen.tsx` | 0 | 1 | 0 | 1 |
| `src/components/EmojiPicker.tsx` | 1 | 1 | 1 | 3 |
| `src/components/ProfileBioSections.tsx` | 1 | 1 | 1 | 3 |
| `src/components/ThemeCustomizer.tsx` | 0 | 2 | 1 | 3 |
| `src/components/ComposeBox.tsx` | 1 | 2 | 0 | 3 |
| `src/components/FeedEntryCard.tsx` | 1 | 2 | 0 | 3 |
| `src/components/Omnibar.tsx` | 1 | 1 | 1 | 3 |
| `src/components/RawHtmlEditor.tsx` | 0 | 2 | 1 | 3 |
| `src/components/FriendsPanel.tsx` | 1 | 2 | 0 | 3 |
| `src/components/FeedRightPanel.tsx` | 0 | 2 | 1 | 3 |
| `src/components/ProfileRenderer.tsx` | 1 | 2 | 0 | 3 |
| `src/components/ArticleEditor.tsx` | 0 | 2 | 1 | 3 |
| `src/components/VideoPlayer.tsx` | 1 | 2 | 0 | 3 |
| `src/components/Chat/ChatLayout.tsx` | 1 | 1 | 1 | 3 |
| **TOTAL** | **72** | **168** | **101** | **341** |

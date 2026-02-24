# Willow Store Evaluation (Phase 2 Final Item)

Date: 2026-02-22

## Scope
Evaluate replacing the current redb-backed `BytescapeItem` storage with a Willow-native store (`willow_store_simple_sled`), considering the current media-import pipeline and recently added Willow-style item paths.

## Current State

- Structured/social items (`BytescapeItem`, `VaultMessage`) are stored in `redb` via `DbManager`.
- Vault file metadata and blob references already live in Iroh Docs/Blobs (`SyncManager`), i.e. partially Willow-aligned behavior already exists for media.
- `BytescapeItem.path` has been added and backfilled using hierarchical format:
  - `/{source}/{type}/{year}/{month}/{id}`
- New imports now assign Willow-style paths at write time.

## Evaluation Summary

### Option A — Full Immediate Replacement (redb -> willow_store_simple_sled)

Pros:
- Single data model aligned with Willow from end to end.
- Path-native querying semantics.

Cons / Risks:
- Large blast radius: all read/write paths, dedupe keys, pagination, and filtering APIs must be rewritten.
- Potential feature regressions in import/feed/chat while replacing mature redb code.
- Current dependency ecosystem showed compile friction when enabling Willow crates in this codebase (resolved by feature-gating `willow_25`).

Assessment: **High risk right now**.

### Option B — Hybrid Staged Migration (recommended)

Pros:
- Keeps imports/feed/chat stable while continuing Willow conformance work.
- Uses `path` as migration contract now, enabling transparent backend swap later.
- Lets us migrate by domain (items first, then messages/contacts) with measurable checkpoints.

Cons:
- Temporary dual-store complexity.

Assessment: **Best balance of velocity + safety**.

## Decision

**Choose Option B (Hybrid Staged Migration).**

- Keep redb as active source of truth for structured item/message query paths in current release.
- Continue writing and maintaining canonical Willow paths on every `BytescapeItem`.
- Gate direct Willow crate usage behind Cargo feature flags until transitive dependency graph is stabilized.

## Proposed Execution Plan

1. **Stabilize path-indexed APIs on current store**
   - Add/expand path-prefix query interfaces on `DbManager` as migration-compatible API surface.
2. **Add shadow-write adapter (optional feature)**
   - On `save_item`, write to redb + Willow store in parallel under feature flag.
3. **Consistency verification tooling**
   - Compare counts/checksums/path coverage between stores.
4. **Read switch by endpoint**
   - Move archive/feed endpoints to Willow-backed reads once parity is proven.
5. **Remove redb item path for migrated entities**
   - Keep fallback read for one release cycle.

## Acceptance Criteria for Full Swap

- Import parity: Facebook, Instagram, Bluesky imports identical counts and media linkage.
- Feed parity: same item ordering and dedupe behavior across tabs.
- Migration safety: no data loss in backfilled paths and no missing records during restart.
- Performance: pagination and filtered reads remain within current UX latency.

## Notes on Dependencies

- `willow_25` has been integrated in feature-gated form (`feature = "willow"`).
- This allows immediate path-parameter conformance without destabilizing baseline builds.
- Full Willow storage activation should be coupled with dependency unification work before default enablement.

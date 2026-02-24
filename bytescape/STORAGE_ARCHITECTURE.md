# Storage Architecture & Boundaries

This document defines the storage boundaries between **redb** (local database) and **Iroh** (P2P sync) to prevent data inconsistency and clarify ownership.

## Overview

Bytescape uses a **dual-storage architecture**:

| Storage | Purpose | Sync | Location |
|---------|---------|------|----------|
| **redb** | Local indexes, cache, user preferences | ❌ No sync | `$APPDATA/db.redb` |
| **Iroh Blobs** | Media files (images, videos) | ✅ P2P synced | `$APPDATA/sync/blobs/` |
| **Iroh Docs** | Vault file metadata, P2P documents | ✅ P2P synced | `$APPDATA/sync/docs.db` |

---

## Storage Decision Matrix

### What Goes Where

| Data Type | Storage | Rationale |
|-----------|---------|-----------|
| **User sessions** (JWT, DID) | redb (keychain file) | Local-only, security-sensitive |
| **Iroh node keys** | redb (keychain file) | Local-only, security-sensitive |
| **Bluesky posts** (imported) | redb `ITEMS_TABLE` | Local archive, fast queries |
| **Facebook posts** (imported) | redb `ITEMS_TABLE` | Local archive, fast queries |
| **P2P messages** | redb `MESSAGES_TABLE` | Local cache of received messages |
| **Contacts** | redb `CONTACTS_TABLE` | Local address book |
| **Conversations** | redb `CONVERSATIONS_TABLE` | Local conversation metadata |
| **Vault files** (metadata) | Iroh Docs `vault_namespace` | P2P synced file catalog |
| **Vault files** (content) | Iroh Blobs | P2P synced binary data |
| **Sync documents** | Iroh Docs | P2P CRDT state |
| **Followed hashtags** | redb `FOLLOWED_HASHTAGS_TABLE` | Local preferences |

---

## Data Flow Diagrams

### Import Flow (Bluesky/Facebook)

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│  External    │────▶│  Parser/    │────▶│  redb        │
│  API/ZIP     │     │  Connector  │     │  ITEMS_TABLE │
└──────────────┘     └─────────────┘     └──────────────┘
                            │
                            ▼
                     ┌─────────────┐
                     │  Iroh       │
                     │  Blobs      │
                     │  (media)    │
                     └─────────────┘
```

**Process:**
1. Connector fetches/parses external data
2. Media downloaded → stored in Iroh Blobs (returns hash)
3. Metadata + blob hashes → stored in redb `ITEMS_TABLE`
4. Item is queryable locally, media is P2P-synced

---

### Vault File Upload Flow

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│  User        │────▶│  Iroh       │────▶│  Iroh        │
│  File        │     │  Blobs      │     │  Docs        │
└──────────────┘     │  (content)  │     │  (metadata)  │
                     └─────────────┘     └──────────────┘
```

**Process:**
1. File bytes → stored in Iroh Blobs (returns hash)
2. Thumbnail generated → stored in Iroh Blobs (returns hash)
3. `VaultFile` metadata (name, size, hashes, tags) → stored in Iroh Docs
4. File is immediately P2P-synced to connected peers

---

### P2P Message Flow

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│  Remote      │────▶│  Iroh       │────▶│  redb        │
│  Peer        │     │  Connection │     │  MESSAGES_   │
│              │     │             │     │  TABLE       │
└──────────────┘     └─────────────┘     └──────────────┘
                            │
                            ▼
                     ┌─────────────┐
                     │  Frontend   │
                     │  Event      │
                     └─────────────┘
```

**Process:**
1. Incoming P2P connection accepted
2. Message received via bidirectional stream
3. Message persisted to redb `MESSAGES_TABLE`
4. Frontend notified via Tauri event
5. Message queryable from local DB

---

## Table Schemas

### redb Tables

```rust
// Local posts (Bluesky, Facebook imports)
ITEMS_TABLE: TableDefinition<&str, &str>  // Key: source:original_id, Value: JSON<BytescapeItem>

// P2P and Bluesky messages
MESSAGES_TABLE: TableDefinition<&str, &str>  // Key: message_id, Value: JSON<Message>

// Known contacts
CONTACTS_TABLE: TableDefinition<&str, &str>  // Key: did, Value: JSON<Contact>

// Conversation metadata
CONVERSATIONS_TABLE: TableDefinition<&str, &str>  // Key: conversation_id, Value: JSON<Conversation>

// User preferences
FOLLOWED_HASHTAGS_TABLE: TableDefinition<&str, &str>  // Key: tag, Value: timestamp
```

### Iroh Docs Entries

```
// Vault file metadata
vault/file/{file_id} → JSON<VaultFile>

// Sync documents (created per sharing session)
doc/{namespace_id} → CRDT state
```

### Iroh Blobs

```
// Media content addressed by hash
{hash} → bytes (image, video, etc.)
{hash}:thumb → bytes (WebP thumbnail)
```

---

## Ownership Rules

### Rule 1: redb is Local Cache
- redb data is **NOT** synced to peers
- redb can be rebuilt from Iroh + re-import if lost
- redb stores denormalized data for fast queries

### Rule 2: Iroh Docs is Source of Truth for Vault
- Vault file metadata lives in Iroh Docs
- Multiple peers can edit (CRDT resolution)
- redb may cache Iroh Docs data for performance

### Rule 3: Iroh Blobs is Content-Addressed Storage
- Content addressed by BLAKE3 hash
- Deduplicated automatically
- Thumbnails stored as separate blobs

### Rule 4: Security-Sensitive Data Stays Local
- JWT tokens → keychain file only
- Iroh secret keys → keychain file only
- Never sync credentials via P2P

---

## Migration Strategy

### Current State (Technical Debt)
- ❌ `BytescapeItem` stored in redb only (not P2P synced)
- ❌ `VaultFile` stored in Iroh Docs (separate from items)
- ❌ No clear migration path between systems

### Target State
1. **Unified Item Model**: All content items use Iroh Docs for P2P sync
2. **redb as Index**: redb stores materialized views for fast queries
3. **Sync on Import**: Imported items automatically synced to Iroh

### Migration Steps
1. Add `sync_id: Option<NamespaceId>` to `BytescapeItem`
2. On import, create Iroh Doc entry + store sync_id
3. Background job syncs existing redb items to Iroh
4. Queries check Iroh first, fallback to redb

---

## Performance Considerations

### Query Patterns

| Query | Storage | Notes |
|-------|---------|-------|
| List items by date | redb | Indexed, fast |
| List items by source | redb | Filter on source field |
| Get vault stats | redb | Pre-computed counts |
| Get file content | Iroh Blobs | Stream from blob store |
| Search content | redb | Future: full-text index |
| Sync status | Iroh Docs | Check CRDT state |

### Caching Strategy

```
┌─────────────────────────────────────┐
│  Frontend Request                   │
└──────────────┬──────────────────────┘
               ▼
┌─────────────────────────────────────┐
│  redb (fast path)                   │
│  - Items, Messages, Contacts        │
│  - Materialized views               │
└──────────────┬──────────────────────┘
               │ (cache miss)
               ▼
┌─────────────────────────────────────┐
│  Iroh Docs/Blobs (slow path)        │
│  - P2P fetch                        │
│  - Update redb cache                │
└─────────────────────────────────────┘
```

---

## Error Handling

| Error | Storage | Recovery |
|-------|---------|----------|
| redb corruption | redb | Delete + re-import from source |
| Iroh blob missing | Iroh Blobs | Fetch from P2P peers |
| Iroh doc conflict | Iroh Docs | CRDT auto-resolution |
| Keychain lost | File | User must re-login |

---

## Future Considerations

1. **Willow Protocol**: Consider migrating to Willow-compatible storage for true P2P interoperability
2. **Erasure Coding**: Add redundancy for blob storage across peers
3. **Selective Sync**: Allow users to choose which namespaces to sync
4. **Offline Mode**: Queue writes locally, sync when connected

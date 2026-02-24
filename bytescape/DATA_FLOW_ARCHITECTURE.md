# Bytescape Data Engineering Architecture (Implemented)

This document defines the end-to-end data flow and storage architecture for:

1. Bluesky/Facebook post cloning and timeline display
2. Media extraction and Vault storage/display
3. Messaging import + real-time chat + archived chat partitions
4. Hard deletion pathways (Clear Vault + account switch)

## 1) Data Sources

## Social post sources
- Bluesky API
  - Following timeline (`get_timeline`)
  - Saved custom feeds (`list_saved_feeds` + feed fetch)
- Facebook archive ZIP
  - Parsed posts + media references (`import_facebook_zip` → `commit_memories`)

## Messaging sources
- Bluesky DM API (live import/send)
  - `import_vault_messages`
  - `send_bsky_dm`
- Walled-garden archives (Facebook) — archived/read-only lane in UI model

## Media sources
- Remote media URLs discovered from Bluesky/Facebook connectors
- Local filesystem media extracted from archive ZIPs
- Blob-addressed media (`blob:<hash>`) in Iroh

## 2) Display Interfaces

## Feed screen tabs
- Following: people/profile timeline only (Bluesky following timeline)
- Feeds: all followed custom feeds combined, with dropdown to select one feed or all
- All: merged + deduplicated union of Following + Feeds

## Vault screen
- File-first media browser from Iroh Docs metadata (`vault/file/*`)
- Fullscreen media viewer with navigation and optional metadata panel

## Chat screen
- Live lane: Bluesky/Mastodon sources (interactive composer)
- Archived lane: walled-garden sources (read-only)

## 3) Storage Configuration

## Local index store (redb)
- `ITEMS_TABLE`: normalized posts (`BytescapeItem`)
- `VAULT_MESSAGES_TABLE`: imported/sent messages (`VaultMessage`) with `owner_did`
- `CONTACTS_TABLE`, `CONVERSATIONS_TABLE`, `FOLLOWED_HASHTAGS_TABLE`, etc.

## Sync/media store (Iroh)
- Iroh Blobs: binary media bytes (content-addressed)
- Iroh Docs: Vault file catalog entries (`vault/file/{id}` -> `VaultFile`)

## Ownership and isolation
- Session owner DID is the tenant boundary.
- Messages are saved with `owner_did` and queried by owner.
- Logout clears local tables and synced vault file metadata.

## 4) End-to-End Flow

## A. Post cloning + feed rendering
1. Fetch from Bluesky timeline and saved feeds, parse Facebook archive posts.
2. Normalize to `BytescapeItem` and save to `ITEMS_TABLE`.
3. Feed UI composes views:
   - Following = timeline
   - Feeds = `get_unified_feed` over selected/all followed feeds
   - All = merge(Following, Feeds), sort by timestamp, dedupe by URI/CID

## B. Media extraction to Vault
1. Import connectors detect media paths/URLs.
2. Media bytes are stored in Iroh Blobs (or resolved from `blob:` hash).
3. A `VaultFile` metadata entry is written to Iroh Docs.
4. Vault UI lists from `vault_list_files` and streams bytes via `vault_get_file_data`.

## C. Messages and chat partitioning
1. Bluesky DM import stores messages as `VaultMessage { owner_did, source, ... }`.
2. Chat UI groups conversations by source:
   - live: `bluesky`, `mastodon`
   - archived: everything else
3. Send composer is enabled only for live conversations.

## D. Deletion pathways
- Clear Vault button:
  - clears all local user-scoped redb tables
  - clears all Iroh vault file metadata entries
- Logout/account switch:
  - clears all local user-scoped redb tables
  - clears all Iroh vault file metadata entries
  - clears pending import buffer
  - deletes session tokens

## 5) Required Functions / Contracts

## Import and feed contracts
- `import_bluesky_posts`
- `import_facebook_zip`, `commit_memories`
- `get_timeline`
- `list_saved_feeds`
- `get_unified_feed(tags, feed_uris, limit)`
- `list_follows(actor)`

## Vault/media contracts
- `vault_add_file`, `vault_upload_file`, `vault_upload_files_from_paths`
- `vault_list_files`, `vault_get_file_data`, `vault_get_thumbnail`
- `vault_delete_file`, `vault_deduplicate_files`, `vault_clear_all_files`

## Chat contracts
- `import_vault_messages`
- `send_bsky_dm`
- `list_vault_messages` (owner-scoped)

## Deletion/auth contracts
- `clear_vault` (full wipe of local + sync data)
- `logout` (full wipe + session removal)

## 6) Current Guarantees

- Cross-account leakage prevention for chats via `owner_did` scoping + full wipe on logout.
- Feed semantics match requested model (Following / Feeds with selector / All merged).
- Vault ingestion from imports writes media into vault catalog for discoverability.
- Deletion pathways remove both index records and synced vault media catalog entries.

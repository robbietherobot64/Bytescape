//! # db/vault.rs — Vault Item Storage & Retrieval
//!
//! This is the most important database module. It stores and retrieves
//! **BytescapeItems** — the universal container for all content in the vault
//! (posts, photos, articles, videos from any platform).
//!
//! ## What's in the Vault?
//!
//! Every piece of content the user cares about ends up here:
//! - Bluesky posts fetched from the API
//! - Facebook posts imported from a data archive
//! - Instagram photos imported from a data archive
//! - Locally composed posts
//! - Posts synced from P2P peers
//!
//! All are normalized into the same `BytescapeItem` format (defined in
//! `models/mod.rs`), so the feed UI can display them uniformly regardless
//! of where they came from.
//!
//! ## Key Concepts
//!
//! ### Deduplication
//! Items are keyed by `source:original_id` (e.g. `"bluesky:at://did:plc:..."`)
//! rather than just a UUID. This means importing the same Bluesky post twice
//! won't create duplicates — the second `save_item()` call returns `false`
//! (already exists) instead of creating a second copy.
//!
//! ### Header Cache (Performance Optimization)
//! Loading the feed requires knowing each item's metadata (title, timestamp,
//! type, source, author) but NOT the full content body. Instead of deserializing
//! every item from disk on each feed load, we cache lightweight
//! `BytescapeItemHeader` objects in memory. The cache is invalidated on any write.
//!
//! ### Syndication Dedup
//! When a user composes a post locally and syndicates it to Bluesky, the
//! local copy and the API-imported copy are different records. The
//! `cleanup_syndication_duplicates()` method finds and removes these
//! API-imported duplicates by checking `share_history` entries.
//!
//! ## Related Files
//! - `models/mod.rs` — `BytescapeItem`, `BytescapeItemHeader`, `BytescapeType`
//! - `db/mod.rs` — `ITEMS_TABLE` constant
//! - `commands/vault.rs` — Tauri commands that call these methods
//! - `connectors/bluesky.rs` — Creates items from Bluesky API responses

use super::*;
use redb::ReadableTable;


impl DbManager {
    // ─── Core Item Operations ──────────────────────────────────────────

    /// Saves a new BytescapeItem to the vault. Returns `true` if the item was
    /// new, `false` if it already existed (duplicate detected).
    ///
    /// **Deduplication**: Uses `source:original_id` as the storage key, so
    /// importing the same Bluesky post twice won't create duplicates.
    ///
    /// **Side effects**:
    /// - Writes to `ITEMS_TABLE`
    /// - Invalidates the header cache (next feed load will rebuild from disk)
    pub async fn save_item(&self, item: &BytescapeItem) -> Result<bool, String> {
        let serialized = serde_json::to_string(item).map_err(|e| e.to_string())?;
        // Use source:original_id as key for dedup, fallback to item.id
        let key = if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        };
        let db = self.db.write().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let already_exists = table.get(key.as_str()).map_err(|e| e.to_string())?.is_some();
        drop(table);
        drop(read_txn);

        if already_exists {
            println!("db: Item already exists with key: {}", key);
            return Ok(false); // not new
        }

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        self.invalidate_header_cache().await;
        Ok(true) // was new
    }

    /// Computes the storage key for a vault item.
    ///
    /// Uses `source:original_id` if the item has an `original_id` (imported from
    /// an external platform), otherwise falls back to the item's UUID. This key
    /// format enables deduplication: the same Bluesky post imported twice will
    /// produce the same key and thus overwrite rather than duplicate.
    fn item_storage_key(item: &BytescapeItem) -> String {
        if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        }
    }

    /// Saves a vault item, overwriting if it already exists (unconditional upsert).
    ///
    /// Unlike `save_item()` which skips duplicates, this always writes. Used when
    /// updating an existing item (e.g. adding a like, updating share_history).
    /// Invalidates the header cache.
    pub async fn upsert_item(&self, item: &BytescapeItem) -> Result<(), String> {
        let serialized = serde_json::to_string(item).map_err(|e| e.to_string())?;
        let key = Self::item_storage_key(item);

        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        self.invalidate_header_cache().await;
        Ok(())
    }

    // ─── Single-Item Retrieval ──────────────────────────────────────────

    /// Retrieves a single vault item by its ID.
    ///
    /// First tries a direct key lookup (O(1) for redb's B-tree). If that fails
    /// (because the key was stored as `source:original_id` rather than the item's
    /// UUID), falls back to a full table scan comparing `item.id` values.
    pub async fn get_item_by_id(&self, item_id: &str) -> Result<Option<BytescapeItem>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        // Try direct lookup first (if key is the ID)
        if let Some(val) = table.get(item_id).map_err(|e| e.to_string())? {
            let item: BytescapeItem = serde_json::from_str(val.value()).map_err(|e| e.to_string())?;
            return Ok(Some(item));
        }

        // Fallback: search values (rare cases where key is source:original_id)
        for row in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = row.map_err(|e| e.to_string())?;
            let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            if item.id == item_id {
                return Ok(Some(item));
            }
        }

        Ok(None)
    }

    /// Deletes a single vault item by its UUID.
    ///
    /// First tries a direct key lookup. If the key is `source:original_id`,
    /// falls back to a full table scan to find the matching storage key.
    /// Invalidates the header cache after deletion.
    ///
    /// Returns `true` if the item was found and deleted, `false` if not found.
    pub async fn delete_item_by_id(&self, item_id: &str) -> Result<bool, String> {
        // First, find the storage key for this item
        let storage_key = {
            let db = self.db.read().await;
            let read_txn = db.begin_read().map_err(|e| e.to_string())?;
            let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

            // Try direct lookup first
            if let Some(val) = table.get(item_id).map_err(|e| e.to_string())? {
                let item: BytescapeItem = serde_json::from_str(val.value()).map_err(|e| e.to_string())?;
                if item.id == item_id {
                    Some(item_id.to_string())
                } else {
                    None
                }
            } else {
                // Fallback: scan for the item
                let mut found_key = None;
                for row in table.iter().map_err(|e| e.to_string())? {
                    let (key, value) = row.map_err(|e| e.to_string())?;
                    let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                    if item.id == item_id {
                        found_key = Some(key.value().to_string());
                        break;
                    }
                }
                found_key
            }
        };

        let Some(key) = storage_key else {
            return Ok(false);
        };

        // Delete the item
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        self.invalidate_header_cache().await;
        Ok(true)
    }

    // ─── Bulk Operations & Dedup ────────────────────────────────────────

    /// Collects all known URIs/IDs from the vault, including:
    /// - Each item's `id` (UUID)
    /// - Each item's `original_id` (platform-specific URI)
    /// - All URIs from each item's `share_history` (syndication records)
    ///
    /// Used by the Bluesky connector to skip already-imported posts during sync.
    pub async fn get_all_known_uris(&self) -> Result<std::collections::HashSet<String>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut known = std::collections::HashSet::new();
        for row in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = row.map_err(|e| e.to_string())?;
            if let Ok(item) = serde_json::from_str::<BytescapeItem>(value.value()) {
                known.insert(item.id);
                if let Some(oid) = item.original_id {
                    known.insert(oid);
                }
                for event in item.share_history {
                    known.insert(event.uri);
                }
            }
        }
        Ok(known)
    }

    /// Removes API-imported duplicates of locally-syndicated posts.
    ///
    /// **The problem**: When a user composes a post in Bytescape and syndicates it
    /// to Bluesky, two copies exist: the local original and the API-imported copy
    /// that comes back on the next timeline sync. This method finds and removes
    /// the imported copy.
    ///
    /// **How it works**:
    /// 1. Scans non-Bluesky items for `share_history` entries targeting Bluesky
    /// 2. Collects those syndicated URIs
    /// 3. Finds Bluesky-sourced items whose `original_id` matches a syndicated URI
    /// 4. Deletes those duplicates
    ///
    /// Returns the number of duplicates removed.
    pub async fn cleanup_syndication_duplicates(&self) -> Result<usize, String> {
        let db = self.db.write().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        // 1. Collect syndicated URIs ONLY from locally-created items (non-bluesky source)
        //    that were syndicated TO Bluesky. These are the canonical copies.
        //    We want to delete API-imported duplicates of these items, not ALL bluesky items.
        let mut syndicated_uris = std::collections::HashSet::new();
        for row in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = row.map_err(|e| e.to_string())?;
            if let Ok(item) = serde_json::from_str::<BytescapeItem>(value.value()) {
                // Only collect URIs from items NOT imported from bluesky —
                // these are locally-created posts that were later syndicated.
                if item.source != "bluesky" {
                    for event in item.share_history {
                        if event.platform == "bluesky" {
                            syndicated_uris.insert(event.uri);
                        }
                    }
                }
            }
        }

        // 2. Find all items whose source is "bluesky" and original_id is in syndicated_uris
        let mut keys_to_delete = Vec::new();
        for row in table.iter().map_err(|e| e.to_string())? {
            let (key, value) = row.map_err(|e| e.to_string())?;
            if let Ok(item) = serde_json::from_str::<BytescapeItem>(value.value()) {
                if item.source == "bluesky" {
                    if let Some(ref oid) = item.original_id {
                        if syndicated_uris.contains(oid) {
                            keys_to_delete.push(key.value().to_string());
                        }
                    }
                }
            }
        }
        drop(table);
        drop(read_txn);

        if keys_to_delete.is_empty() {
            return Ok(0);
        }

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut w_table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            for k in &keys_to_delete {
                w_table.remove(k.as_str()).map_err(|e| e.to_string())?;
                println!("cleanup: Removed duplicate imported syndication vault item with key {}", k);
            }
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        self.invalidate_header_cache().await;

        Ok(keys_to_delete.len())
    }

    // ─── Header Cache & Feed Queries ────────────────────────────────────
    // The feed UI needs metadata about every vault item (title, timestamp,
    // type, source) but not the full content. These methods use a cached
    // header list to avoid expensive full-table scans on every feed load.

    /// Returns lightweight headers for ALL vault items (cached).
    /// This is the primary entry point for feed rendering.
    pub async fn list_all_item_headers(&self) -> Result<Vec<BytescapeItemHeader>, String> {
        self.get_cached_headers().await
    }

    /// Returns full items filtered by author DID (include or exclude).
    ///
    /// When `exclude = false`: returns only items by this author.
    /// When `exclude = true`: returns items by everyone EXCEPT this author.
    /// Fetches headers first (cached), then loads full items for matches.
    pub async fn list_items_filtered(&self, filter_did: Option<&str>, exclude: bool) -> Result<Vec<BytescapeItem>, String> {
        let all_headers = self.list_all_item_headers().await?;
        let filtered_headers: Vec<BytescapeItemHeader> = if let Some(did) = filter_did {
            all_headers.into_iter().filter(|h| {
                if let Some(ref author_did) = h.author_did {
                    if exclude {
                        author_did != did
                    } else {
                        author_did == did
                    }
                } else {
                    if exclude {
                        true // Not me
                    } else {
                        false // Not me
                    }
                }
            }).collect()
        } else {
            all_headers
        };

        let mut full_items = Vec::new();
        for h in filtered_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                full_items.push(item);
            }
        }
        Ok(full_items)
    }

    /// Builds the complete header list from the database (slow path).
    ///
    /// Performs a full table scan, deserializes every item, extracts its header,
    /// and sorts by timestamp (newest first). This is called only when the cache
    /// is empty — subsequent calls use `get_cached_headers()` instead.
    ///
    /// **Performance note**: For a vault with 10,000 items, this takes ~50ms.
    /// The cache eliminates this cost on every feed load after the first.
    /// Build the full header list from DB (uncached path).
    async fn build_all_headers(&self) -> Result<Vec<BytescapeItemHeader>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut headers = Vec::new();
        for row in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = row.map_err(|e| e.to_string())?;
            let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            headers.push(item.header());
        }

        headers.sort_by(|a, b| b.effective_timestamp.cmp(&a.effective_timestamp));
        Ok(headers)
    }

    /// Get all headers from the database, using the in-memory cache if available.
    /// The cache is invalidated whenever an item is saved or deleted.
    async fn get_cached_headers(&self) -> Result<Vec<BytescapeItemHeader>, String> {
        // Fast path: return cached headers if available
        {
            let cache = self.header_cache.read().await;
            if let Some(ref headers) = *cache {
                return Ok(headers.clone());
            }
        }
        // Slow path: build from DB and store in cache
        let headers = self.build_all_headers().await?;
        {
            let mut cache = self.header_cache.write().await;
            *cache = Some(headers.clone());
        }
        Ok(headers)
    }

    /// Invalidate the header cache. Call this after any write to ITEMS_TABLE.
    pub(crate) async fn invalidate_header_cache(&self) {
        let mut cache = self.header_cache.write().await;
        *cache = None;
    }

    /// Returns headers for items that belong to the current user.
    ///
    /// An item is considered "mine" if:
    /// - `author_did` matches `owner_did`, OR
    /// - `author_did` is `None` (archive imports without DID attribution), OR
    /// - `is_liked` is `true` (posts the user explicitly saved/liked)
    ///
    /// Uses the cached header list for performance.
    pub async fn list_my_item_headers(&self, owner_did: &str) -> Result<Vec<BytescapeItemHeader>, String> {
        let all_headers = self.get_cached_headers().await?;
        let headers: Vec<BytescapeItemHeader> = all_headers.into_iter().filter(|item| {
            if item.is_liked {
                true
            } else if item.author_did.as_deref() == Some(owner_did) {
                true
            } else {
                item.author_did.is_none()
            }
        }).collect();
        Ok(headers)
    }

    /// Returns items from the same calendar day across all years ("On This Day" feature).
    ///
    /// Matches items where the original post date's month and day match the given
    /// values, regardless of year. Used for nostalgic "memories" feature.
    pub async fn list_items_on_this_day(&self, owner_did: &str, month: u32, day: u32) -> Result<Vec<BytescapeItem>, String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let matching_headers: Vec<BytescapeItemHeader> = all_headers.into_iter().filter(|h| {
            h.month == month && h.day == day
        }).collect();

        let mut items = Vec::new();
        for h in matching_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                items.push(item);
            }
        }
        Ok(items)
    }

    /// Returns the total number of items in the vault.
    pub async fn count_items(&self) -> Result<usize, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let count = table.len().map_err(|e: redb::StorageError| e.to_string())?;
        Ok(count as usize)
    }

    // ─── Paginated Queries ──────────────────────────────────────────────
    // These methods support offset-based pagination for the feed UI.
    // They use a "headers-first" approach: filter/sort the lightweight
    // header cache, then bulk-fetch only the items for the current page.

    /// Returns a page of the user's vault items with total count.
    ///
    /// Uses a headers-first approach to avoid loading all items into memory:
    /// 1. Filter headers by owner and optional source
    /// 2. Slice to the requested page (offset + limit)
    /// 3. Bulk-fetch only the full items for that page
    ///
    /// Returns `(items_for_page, total_matching_count)`.
    pub async fn list_my_items_paged(&self, owner_did: &str, source_filter: Option<&str>, limit: usize, offset: usize) -> Result<(Vec<BytescapeItem>, usize), String> {
        let mut header_all = self.list_my_item_headers(owner_did).await?;

        // Optional source filter
        if let Some(src) = source_filter {
            header_all.retain(|h| h.source.eq_ignore_ascii_case(src));
        }

        let total = header_all.len();
        let page_headers: Vec<BytescapeItemHeader> = header_all.into_iter().skip(offset).take(limit).collect();
        
        if page_headers.is_empty() {
            return Ok((vec![], total));
        }

        // Bulk fetch: collect IDs needed, then single-pass table scan
        let needed_ids: std::collections::HashSet<String> = page_headers.iter().map(|h| h.id.clone()).collect();
        let full_items = self.bulk_get_items_by_ids(&needed_ids).await?;

        // Re-sort to match header order (newest first)
        let id_to_pos: std::collections::HashMap<&str, usize> = page_headers.iter().enumerate().map(|(i, h)| (h.id.as_str(), i)).collect();
        let mut sorted = full_items;
        sorted.sort_by_key(|item| id_to_pos.get(item.id.as_str()).copied().unwrap_or(usize::MAX));

        Ok((sorted, total))
    }

    /// Like `list_my_items_paged` but without the owner filter.
    ///
    /// Returns items from ALL sources, including P2P-synced posts from other
    /// users. Used by the unified feed's "All" tab.
    pub async fn list_all_items_paged(&self, source_filter: Option<&str>, limit: usize, offset: usize) -> Result<(Vec<BytescapeItem>, usize), String> {
        let mut headers = self.get_cached_headers().await?;

        // Optional source filter
        if let Some(src) = source_filter {
            headers.retain(|h| h.source.eq_ignore_ascii_case(src));
        }

        let total = headers.len();
        let page_headers: Vec<BytescapeItemHeader> = headers.into_iter().skip(offset).take(limit).collect();

        if page_headers.is_empty() {
            return Ok((vec![], total));
        }

        let needed_ids: std::collections::HashSet<String> = page_headers.iter().map(|h| h.id.clone()).collect();
        let full_items = self.bulk_get_items_by_ids(&needed_ids).await?;

        let id_to_pos: std::collections::HashMap<&str, usize> = page_headers.iter().enumerate().map(|(i, h)| (h.id.as_str(), i)).collect();
        let mut sorted = full_items;
        sorted.sort_by_key(|item| id_to_pos.get(item.id.as_str()).copied().unwrap_or(usize::MAX));

        Ok((sorted, total))
    }

    /// Fetches multiple items by their IDs in a single pass.
    ///
    /// **Optimization**: First tries direct key lookups (O(1) each in redb's B-tree).
    /// Only falls back to a full table scan for items whose storage key differs
    /// from their `item.id` (i.e. items keyed as `source:original_id`).
    ///
    /// This is much faster than calling `get_item_by_id()` N times, because it
    /// avoids opening N separate read transactions.
    async fn bulk_get_items_by_ids(&self, ids: &std::collections::HashSet<String>) -> Result<Vec<BytescapeItem>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut items = Vec::with_capacity(ids.len());

        // Try direct key lookups first (O(1) each for redb B-tree)
        let mut remaining: std::collections::HashSet<String> = ids.clone();
        for id in ids {
            if let Some(val) = table.get(id.as_str()).map_err(|e| e.to_string())? {
                let item: BytescapeItem = serde_json::from_str(val.value()).map_err(|e| e.to_string())?;
                remaining.remove(&item.id);
                items.push(item);
            }
        }

        // Fallback scan only for items not found by direct key
        // (keys stored as source:original_id instead of item.id)
        if !remaining.is_empty() {
            for row in table.iter().map_err(|e| e.to_string())? {
                if remaining.is_empty() { break; }
                let (_, value) = row.map_err(|e| e.to_string())?;
                let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                if remaining.remove(&item.id) {
                    items.push(item);
                }
            }
        }

        Ok(items)
    }

    /// Paginated query filtering by Willow path prefix.
    ///
    /// Willow uses hierarchical paths (e.g. `/vault/status/2024/...`) to organize
    /// items. This method returns items whose `path` field starts with the given
    /// prefix, enabling folder-like browsing of the vault.
    ///
    /// Returns `(items_for_page, total_matching_count)`.
    pub async fn list_my_items_by_path_prefix_paged(
        &self,
        owner_did: &str,
        path_prefix: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<BytescapeItem>, usize), String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let normalized_prefix = if path_prefix.starts_with('/') {
            path_prefix.to_string()
        } else {
            format!("/{}", path_prefix)
        };

        // Filter using the path field already present on BytescapeItemHeader
        let matching_headers: Vec<BytescapeItemHeader> = all_headers.into_iter()
            .filter(|h| h.path.starts_with(&normalized_prefix))
            .collect();

        let total = matching_headers.len();
        let page_headers: Vec<BytescapeItemHeader> = matching_headers.into_iter().skip(offset).take(limit).collect();

        // Fetch full items only for the current page
        let mut page = Vec::new();
        for h in page_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                page.push(item);
            }
        }
        Ok((page, total))
    }

    // ─── Filter Queries ─────────────────────────────────────────────────

    /// Returns all items from a specific source (e.g. "bluesky", "facebook").
    /// Filters the cached headers, then fetches full items for matches.
    pub async fn list_items_by_source(&self, source: &str, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let matching_headers: Vec<BytescapeItemHeader> = all_headers.into_iter()
            .filter(|h| h.source.eq_ignore_ascii_case(source))
            .collect();
        
        let mut items = Vec::new();
        for h in matching_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                items.push(item);
            }
        }
        Ok(items)
    }

    /// Returns all items of a specific content type (Status, Gallery, Article, Link, Video).
    pub async fn list_items_by_type(&self, item_type: BytescapeType, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let matching_headers: Vec<BytescapeItemHeader> = all_headers.into_iter()
            .filter(|h| h.item_type == item_type)
            .collect();
        
        let mut items = Vec::new();
        for h in matching_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                items.push(item);
            }
        }
        Ok(items)
    }

    /// Returns all items within a timestamp range (inclusive on both ends).
    /// Timestamps are Unix seconds. Used for calendar/timeline views.
    pub async fn list_items_by_date_range(&self, start_ts: i64, end_ts: i64, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let matching_headers: Vec<BytescapeItemHeader> = all_headers.into_iter()
            .filter(|h| h.original_timestamp >= start_ts && h.original_timestamp <= end_ts)
            .collect();
        
        let mut items = Vec::new();
        for h in matching_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                items.push(item);
            }
        }
        Ok(items)
    }

    // ─── Vault Statistics ────────────────────────────────────────────────

    /// Returns a sorted list of all distinct sources in the vault
    /// (e.g. ["bluesky", "facebook", "instagram", "local"]).
    /// Used to populate source filter dropdowns in the UI.
    pub async fn list_vault_sources(&self, owner_did: &str) -> Result<Vec<String>, String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let mut sources: Vec<String> = all_headers.iter()
            .map(|h| h.source.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        sources.sort();
        Ok(sources)
    }

    /// Computes summary statistics for the vault: total items, breakdown by
    /// source and type, and count of items with media attachments.
    ///
    /// Used to display vault health/overview on the profile page.
    pub async fn vault_stats(&self, owner_did: &str) -> Result<VaultStats, String> {
        let all_headers = self.list_my_item_headers(owner_did).await?;
        let total = all_headers.len();
        let mut by_source: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut media_count: usize = 0;

        for h in &all_headers {
            *by_source.entry(h.source.clone()).or_insert(0) += 1;
            let type_str = match h.item_type {
                BytescapeType::Status => "status",
                BytescapeType::Gallery => "gallery",
                BytescapeType::Article => "article",
                BytescapeType::Link => "link",
                BytescapeType::Video => "video",
            };
            *by_type.entry(type_str.to_string()).or_insert(0) += 1;
            if h.has_media {
                media_count += 1;
            }
        }

        Ok(VaultStats { total, by_source, by_type, media_count })
    }
}

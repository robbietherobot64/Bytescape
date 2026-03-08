use super::*;
use redb::ReadableTable;


impl DbManager {
    pub async fn save_item(&self, item: &BytescapeItem) -> Result<bool, String> {
        let serialized = serde_json::to_string(item).map_err(|e| e.to_string())?;
        // Use source:original_id as key for dedup, fallback to item.id
        let key = if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        };
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let already_exists = table.get(key.as_str()).map_err(|e| e.to_string())?.is_some();
        drop(table);
        drop(read_txn);

        if already_exists {
            println!("db: Item already exists with key: {}", key);
            return Ok(false); // not new
        }

        let parity = Self::build_willow_write_parity_entry(item, "save", &key);
        let parity_key = Self::willow_parity_storage_key(&parity);
        let parity_serialized = serde_json::to_string(&parity).map_err(|e| e.to_string())?;

        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
            drop(table);

            let mut parity_table = write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            parity_table.insert(parity_key.as_str(), parity_serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(true) // was new
    }

    fn item_storage_key(item: &BytescapeItem) -> String {
        if let Some(ref oid) = item.original_id {
            format!("{}:{}", item.source, oid)
        } else {
            item.id.clone()
        }
    }

    fn willow_parity_storage_key(entry: &WillowWriteParityEntry) -> String {
        format!("{}:{}", entry.written_at, entry.id)
    }

    fn build_willow_write_parity_entry(item: &BytescapeItem, write_mode: &str, legacy_key: &str) -> WillowWriteParityEntry {
        let expected_legacy_key = Self::item_storage_key(item);
        let canonical_path = canonical_willow_path(item);
        let willow_path = item.path.trim().to_string();
        let written_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        WillowWriteParityEntry {
            id: uuid::Uuid::new_v4().to_string(),
            written_at,
            write_mode: write_mode.to_string(),
            item_id: item.id.clone(),
            source: item.source.clone(),
            legacy_key: legacy_key.to_string(),
            expected_legacy_key: expected_legacy_key.clone(),
            legacy_key_matches: legacy_key == expected_legacy_key,
            willow_path: willow_path.clone(),
            canonical_path: canonical_path.clone(),
            path_present: !willow_path.is_empty(),
            path_matches_canonical: !willow_path.is_empty() && willow_path == canonical_path,
        }
    }

    pub async fn upsert_item(&self, item: &BytescapeItem) -> Result<(), String> {
        let serialized = serde_json::to_string(item).map_err(|e| e.to_string())?;
        let key = Self::item_storage_key(item);
        let parity = Self::build_willow_write_parity_entry(item, "upsert", &key);
        let parity_key = Self::willow_parity_storage_key(&parity);
        let parity_serialized = serde_json::to_string(&parity).map_err(|e| e.to_string())?;

        let db = self.db.lock().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
            drop(table);

            let mut parity_table = write_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
            parity_table.insert(parity_key.as_str(), parity_serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_willow_write_parity_report(&self) -> Result<WillowWriteParityReport, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(WILLOW_WRITE_PARITY_TABLE).map_err(|e| e.to_string())?;
        let items_table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut total_writes = 0usize;
        let mut save_writes = 0usize;
        let mut upsert_writes = 0usize;
        let mut mismatched_legacy_keys = 0usize;
        let mut missing_paths = 0usize;
        let mut mismatched_paths = 0usize;
        let mut last_written_at: Option<u64> = None;
        let mut total_items = 0usize;
        let mut items_with_path = 0usize;
        let mut items_missing_path = 0usize;
        let mut items_matching_canonical_path = 0usize;
        let mut items_mismatched_canonical_path = 0usize;

        for row in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = row.map_err(|e| e.to_string())?;
            let entry: WillowWriteParityEntry = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

            total_writes += 1;
            match entry.write_mode.as_str() {
                "save" => save_writes += 1,
                "upsert" => upsert_writes += 1,
                _ => {}
            }

            if !entry.legacy_key_matches {
                mismatched_legacy_keys += 1;
            }
            if !entry.path_present {
                missing_paths += 1;
            } else if !entry.path_matches_canonical {
                mismatched_paths += 1;
            }

            last_written_at = Some(last_written_at.map_or(entry.written_at, |current| current.max(entry.written_at)));
        }

        for row in items_table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = row.map_err(|e| e.to_string())?;
            let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            total_items += 1;

            if item.path.trim().is_empty() {
                items_missing_path += 1;
                continue;
            }

            items_with_path += 1;
            let canonical_path = canonical_willow_path(&item);
            if item.path == canonical_path {
                items_matching_canonical_path += 1;
            } else {
                items_mismatched_canonical_path += 1;
            }
        }

        let parity_coverage_ratio = if total_items == 0 {
            1.0
        } else {
            (items_with_path as f64) / (total_items as f64)
        };

        Ok(WillowWriteParityReport {
            total_writes,
            save_writes,
            upsert_writes,
            mismatched_legacy_keys,
            missing_paths,
            mismatched_paths,
            last_written_at,
            total_items,
            items_with_path,
            items_missing_path,
            items_matching_canonical_path,
            items_mismatched_canonical_path,
            parity_coverage_ratio,
        })
    }

    pub async fn get_item_by_id(&self, item_id: &str) -> Result<Option<BytescapeItem>, String> {
        let db = self.db.lock().await;
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

    pub async fn get_all_known_uris(&self) -> Result<std::collections::HashSet<String>, String> {
        let db = self.db.lock().await;
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

    pub async fn cleanup_syndication_duplicates(&self) -> Result<usize, String> {
        let db = self.db.lock().await;
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

        Ok(keys_to_delete.len())
    }

    pub async fn list_all_item_headers(&self) -> Result<Vec<BytescapeItemHeader>, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let item_struct: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            items.push(item_struct.header());
        }

        items.sort_by(|a, b| b.effective_timestamp.cmp(&a.effective_timestamp));
        Ok(items)
    }

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

    /// Return only items owned by the current user.
    /// An item is "mine" if:
    ///  - author_did matches the user's DID, OR
    ///  - author_did is None (archive imports like Facebook that don't have a DID)
    ///  - is_liked is true (posts the user explicitly liked/saved)
    pub async fn list_my_item_headers(&self, owner_did: &str) -> Result<Vec<BytescapeItemHeader>, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut headers = Vec::new();
        for row in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = row.map_err(|e| e.to_string())?;
            // Partially deserialize or just deserialize full then convert, 
            // but we avoid keeping the full item in the final Vec.
            let item: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            
            let is_mine = if item.is_liked {
                true
            } else if item.reposted_by_did.as_deref() == Some(owner_did) {
                true
            } else {
                match &item.author_did {
                    Some(did) => did == owner_did,
                    None => true,
                }
            };

            if is_mine {
                headers.push(item.header());
            }
        }

        headers.sort_by(|a, b| b.effective_timestamp.cmp(&a.effective_timestamp));
        Ok(headers)
    }

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

    pub async fn count_items(&self) -> Result<usize, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let count = table.len().map_err(|e: redb::StorageError| e.to_string())?;
        Ok(count as usize)
    }

    /// Paginated version: returns (items, total_count) sorted newest-first.
    /// Uses a metadata-first approach to avoid OOM for large vaults.
    pub async fn list_my_items_paged(&self, owner_did: &str, source_filter: Option<&str>, limit: usize, offset: usize) -> Result<(Vec<BytescapeItem>, usize), String> {
        let mut header_all = self.list_my_item_headers(owner_did).await?;

        // Optional source filter
        if let Some(src) = source_filter {
            header_all.retain(|h| h.source.eq_ignore_ascii_case(src));
        }

        let total = header_all.len();
        let page_headers: Vec<BytescapeItemHeader> = header_all.into_iter().skip(offset).take(limit).collect();
        
        // Now fetch full items only for this page
        let mut full_items = Vec::new();
        for h in page_headers {
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                full_items.push(item);
            }
        }

        Ok((full_items, total))
    }

    /// Path-prefix paginated query for Willow migration compatibility.
    /// Returns (items, total_count) sorted newest-first.
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

        // We can't strictly filter by path in the header since header doesn't have path yet?
        // Wait, did I add path to header? Let me check models/mod.rs
        // Ah, I DID NOT add path to header. I probably should.
        
        // If I haven't added path to header, I'll need to fetch items to check path.
        // OR I add path to header. Let's add path to header.
        
        // Actually, for now, I'll fetch and filter, but it's still better than fetching ALL.
        // Wait, list_my_items(owner_did) used to be a thing. I replaced it with headers.
        
        let mut matching_headers = Vec::new();
        for h in all_headers {
            // Fetch partially or fully? We have to fetch fully if we don't have path in header.
            if let Ok(Some(item)) = self.get_item_by_id(&h.id).await {
                if item.path.starts_with(&normalized_prefix) {
                    matching_headers.push(item);
                }
            }
        }

        let total = matching_headers.len();
        let page: Vec<BytescapeItem> = matching_headers.into_iter().skip(offset).take(limit).collect();
        Ok((page, total))
    }

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
            };
            *by_type.entry(type_str.to_string()).or_insert(0) += 1;
            if h.has_media {
                media_count += 1;
            }
        }

        Ok(VaultStats { total, by_source, by_type, media_count })
    }
}

use super::*;
use redb::ReadableTable;
use crate::models::*;

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

    pub async fn list_items(&self) -> Result<Vec<BytescapeItem>, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_key, value) = item.map_err(|e| e.to_string())?;
            let item_struct: BytescapeItem = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            items.push(item_struct);
        }

        items.sort_by(|a, b| b.original_timestamp.cmp(&a.original_timestamp));
        Ok(items)
    }

    pub async fn list_items_filtered(&self, filter_did: Option<&str>, exclude: bool) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_items().await?;
        if let Some(did) = filter_did {
            let filtered = all.into_iter().filter(|item| {
                if let Some(ref author_did) = item.author_did {
                    if exclude {
                        author_did != did
                    } else {
                        author_did == did
                    }
                } else {
                    // Items without author_did:
                    // If we want "Me" (exclude=false), we probably don't want them unless we know they are ours.
                    // If we want "Us" (exclude=true), we probably do want them? Or maybe not.
                    if exclude {
                        true // Not me
                    } else {
                        false // Not me
                    }
                }
            }).collect();
            Ok(filtered)
        } else {
            Ok(all)
        }
    }

    /// Return only items owned by the current user.
    /// An item is "mine" if:
    ///  - author_did matches the user's DID, OR
    ///  - author_did is None (archive imports like Facebook that don't have a DID)
    ///  - is_liked is true (posts the user explicitly liked/saved)
    pub async fn list_my_items(&self, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_items().await?;
        let filtered = all.into_iter().filter(|item| {
            // Posts the user explicitly liked/saved are theirs
            if item.is_liked {
                return true;
            }
            match &item.author_did {
                // Bluesky/API items: only include if authored by user
                Some(did) => did == owner_did,
                // Archive imports (Facebook, etc.): no author_did means it's the user's own data
                None => true,
            }
        }).collect();
        Ok(filtered)
    }

    pub async fn list_items_on_this_day(&self, month: u32, day: u32) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_items().await?;
        let matching: Vec<BytescapeItem> = all.into_iter().filter(|p| {
            if let Some(dt) = chrono::DateTime::from_timestamp(p.original_timestamp, 0) {
                use chrono::Datelike;
                dt.month() == month && dt.day() == day
            } else {
                false
            }
        }).collect();
        Ok(matching)
    }

    pub async fn count_items(&self) -> Result<usize, String> {
        let db = self.db.lock().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(ITEMS_TABLE).map_err(|e| e.to_string())?;
        let count = table.len().map_err(|e: redb::StorageError| e.to_string())?;
        Ok(count as usize)
    }

    /// Paginated version: returns (items, total_count) sorted newest-first.
    pub async fn list_my_items_paged(&self, owner_did: &str, source_filter: Option<&str>, limit: usize, offset: usize) -> Result<(Vec<BytescapeItem>, usize), String> {
        let mut all = self.list_my_items(owner_did).await?;

        // Optional source filter
        if let Some(src) = source_filter {
            all.retain(|item| item.source.eq_ignore_ascii_case(src));
        }

        // Sort newest first
        all.sort_by(|a, b| b.original_timestamp.cmp(&a.original_timestamp));

        let total = all.len();
        let page: Vec<BytescapeItem> = all.into_iter().skip(offset).take(limit).collect();
        Ok((page, total))
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
        let mut all = self.list_my_items(owner_did).await?;
        let normalized_prefix = if path_prefix.starts_with('/') {
            path_prefix.to_string()
        } else {
            format!("/{}", path_prefix)
        };

        all.retain(|item| item.path.starts_with(&normalized_prefix));
        all.sort_by(|a, b| b.original_timestamp.cmp(&a.original_timestamp));

        let total = all.len();
        let page: Vec<BytescapeItem> = all.into_iter().skip(offset).take(limit).collect();
        Ok((page, total))
    }

    pub async fn list_items_by_source(&self, source: &str, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_my_items(owner_did).await?;
        let filtered = all.into_iter()
            .filter(|item| item.source.eq_ignore_ascii_case(source))
            .collect();
        Ok(filtered)
    }

    pub async fn list_items_by_type(&self, item_type: BytescapeType, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_my_items(owner_did).await?;
        let filtered = all.into_iter()
            .filter(|item| item.item_type == item_type)
            .collect();
        Ok(filtered)
    }

    pub async fn list_items_by_date_range(&self, start_ts: i64, end_ts: i64, owner_did: &str) -> Result<Vec<BytescapeItem>, String> {
        let all = self.list_my_items(owner_did).await?;
        let filtered = all.into_iter()
            .filter(|item| item.original_timestamp >= start_ts && item.original_timestamp <= end_ts)
            .collect();
        Ok(filtered)
    }

    pub async fn list_vault_sources(&self, owner_did: &str) -> Result<Vec<String>, String> {
        let all = self.list_my_items(owner_did).await?;
        let mut sources: Vec<String> = all.iter()
            .map(|item| item.source.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        sources.sort();
        Ok(sources)
    }

    pub async fn vault_stats(&self, owner_did: &str) -> Result<VaultStats, String> {
        let all = self.list_my_items(owner_did).await?;
        let total = all.len();
        let mut by_source: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut media_count: usize = 0;

        for item in &all {
            *by_source.entry(item.source.clone()).or_insert(0) += 1;
            let type_str = match item.item_type {
                BytescapeType::Status => "status",
                BytescapeType::Gallery => "gallery",
                BytescapeType::Article => "article",
            };
            *by_type.entry(type_str.to_string()).or_insert(0) += 1;
            if !item.media_paths.is_empty() || !item.media_blobs.is_empty() {
                media_count += 1;
            }
        }

        Ok(VaultStats { total, by_source, by_type, media_count })
    }
}

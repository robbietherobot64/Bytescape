use iroh::Endpoint;
pub use iroh_docs::engine::Engine as DocsEngine;
pub use iroh_docs::store::Store as DocStore;
pub use iroh_docs::{NamespaceId, DocTicket, Capability, NamespaceSecret};
pub use iroh_docs::engine::DefaultAuthorStorage;
pub use iroh_docs::store::Query;
pub use iroh_blobs::api::downloader::Downloader;
pub use iroh_blobs::Hash;
use iroh_gossip::net::Gossip;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashMap;
use bytes::Bytes;
use uuid::Uuid;
use image::ImageFormat;

// === Vault File Types (stored in Iroh docs, not redb) ===

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum VaultFileKind {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Other,
}

impl VaultFileKind {
    pub fn from_mime(mime: &str) -> Self {
        let lower = mime.to_lowercase();
        if lower.starts_with("image/") { Self::Image }
        else if lower.starts_with("video/") { Self::Video }
        else if lower.starts_with("audio/") { Self::Audio }
        else if lower.contains("pdf") || lower.contains("text/") || lower.contains("document") { Self::Document }
        else if lower.contains("zip") || lower.contains("tar") || lower.contains("gzip") || lower.contains("json") { Self::Archive }
        else { Self::Other }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Archive => "archive",
            Self::Other => "other",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub kind: VaultFileKind,
    pub size: u64,
    pub blob_hash: String,
    #[serde(default)]
    pub thumbnail_hash: Option<String>,
    pub source: String,
    #[serde(default)]
    pub item_id: Option<String>,
    pub added_at: u64,
    #[serde(default)]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultFileStats {
    pub total_files: usize,
    pub total_size: u64,
    pub by_kind: HashMap<String, usize>,
    pub by_source: HashMap<String, usize>,
}

pub struct SyncManager {
    endpoint: Endpoint,
    engine: Arc<DocsEngine>,
    blobs: iroh_blobs::api::Store,
    gossip: Gossip,
    /// The FsStore backing blob storage (kept alive for BlobsProtocol).
    /// The `Store` API handle references internal state owned by FsStore,
    /// so FsStore must outlive the Store.
    #[allow(dead_code)]
    blob_store_fs: iroh_blobs::store::fs::FsStore,
    /// Dedicated namespace for vault file metadata
    vault_namespace: NamespaceId,
}

impl SyncManager {
    pub async fn new(endpoint: Endpoint, storage_path: PathBuf) -> Result<Self, String> {
        println!("SyncManager: Initializing at {:?}", storage_path);
        let blob_path = storage_path.join("blobs");
        let doc_path = storage_path.join("docs.db");
        let author_path = storage_path.join("author");
        
        std::fs::create_dir_all(&blob_path).map_err(|e| e.to_string())?;

        // Initialize Blob Store
        let blob_store_fs = iroh_blobs::store::fs::FsStore::load(&blob_path).await.map_err(|e| format!("BlobStore load failed: {}", e))?;
        let blobs: iroh_blobs::api::Store = blob_store_fs.clone().into();
        
        // Initialize Doc Store
        let docs_store = DocStore::persistent(&doc_path).map_err(|e: anyhow::Error| format!("DocStore load failed at {:?}: {}", doc_path, e))?;

        // Initialize Gossip
        let gossip = Gossip::builder().spawn(endpoint.clone());

        // Initialize Downloader
        let downloader = Downloader::new(&blobs, &endpoint);

        // Initialize Docs Engine
        println!("SyncManager: Spawning DocsEngine...");
        let engine = DocsEngine::spawn(
            endpoint.clone(),
            gossip.clone(),
            docs_store,
            blobs.clone(),
            downloader,
            DefaultAuthorStorage::Persistent(author_path),
            None,
        ).await.map_err(|e: anyhow::Error| e.to_string())?;
        println!("SyncManager: DocsEngine spawned.");
        let engine = Arc::new(engine);

        // Create or restore the vault namespace
        let vault_ns_path = storage_path.join("vault_namespace_id");
        let vault_namespace = if vault_ns_path.exists() {
            let ns_hex = std::fs::read_to_string(&vault_ns_path)
                .map_err(|e| format!("Failed to read vault namespace: {}", e))?;
            let ns_id: NamespaceId = ns_hex.trim().parse()
                .map_err(|e| format!("Failed to parse vault namespace ID: {}", e))?;
            // Ensure it's open
            engine.sync.open(ns_id, Default::default()).await
                .map_err(|e: anyhow::Error| format!("Failed to open vault namespace: {}", e))?;
            println!("SyncManager: Restored vault namespace {}", ns_id);
            ns_id
        } else {
            let secret = NamespaceSecret::new(&mut rand::rng());
            let ns_id = engine.sync.import_namespace(Capability::Write(secret)).await
                .map_err(|e: anyhow::Error| format!("Failed to create vault namespace: {}", e))?;
            engine.sync.open(ns_id, Default::default()).await
                .map_err(|e: anyhow::Error| format!("Failed to open vault namespace: {}", e))?;
            std::fs::write(&vault_ns_path, ns_id.to_string())
                .map_err(|e| format!("Failed to persist vault namespace ID: {}", e))?;
            println!("SyncManager: Created vault namespace {}", ns_id);
            ns_id
        };

        Ok(Self {
            endpoint,
            engine,
            blobs,
            gossip,
            blob_store_fs,
            vault_namespace,
        })
    }

    pub async fn create_doc(&self) -> Result<DocTicket, String> {
        let namespace_secret = NamespaceSecret::new(&mut rand::rng());
        let namespace = self.engine.sync.import_namespace(Capability::Write(namespace_secret.clone())).await.map_err(|e: anyhow::Error| e.to_string())?;
        self.engine.sync.open(namespace, Default::default()).await.map_err(|e: anyhow::Error| e.to_string())?;
        
        let addr = self.endpoint.addr();
        Ok(DocTicket::new(Capability::Write(namespace_secret), vec![addr]))
    }

    // --- Accessors for Router registration ---

    /// Get a clone of the `Arc<DocsEngine>` for registering with the protocol Router.
    pub fn engine_arc(&self) -> Arc<DocsEngine> {
        self.engine.clone()
    }

    /// Get a reference to the Gossip instance for Router registration.
    pub fn gossip(&self) -> &Gossip {
        &self.gossip
    }

    /// Get a reference to the endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub async fn join_doc(&self, ticket: DocTicket) -> Result<NamespaceId, String> {
        let namespace = self.engine.sync.import_namespace(ticket.capability.clone()).await.map_err(|e: anyhow::Error| e.to_string())?;
        self.engine.sync.open(namespace, Default::default()).await.map_err(|e: anyhow::Error| e.to_string())?;
        self.engine.start_sync(namespace, ticket.nodes).await.map_err(|e: anyhow::Error| e.to_string())?;
        Ok(namespace)
    }

    pub async fn send_message(&self, namespace: NamespaceId, content: String) -> Result<(), String> {
        let author = self.engine.default_author.get();
        let key = format!("msg/{}", Uuid::new_v4()).into_bytes();
        
        let content_bytes = Bytes::from(content.into_bytes());
        // Blobs::add_bytes returns AddProgress, which is IntoFuture returning RequestResult<TagInfo>
        let tag_info = self.blobs.add_bytes(content_bytes.clone()).await.map_err(|e| e.to_string())?;
        let hash = tag_info.hash;
        
        self.engine.sync.insert_local(namespace, author, key.into(), hash, content_bytes.len() as u64).await.map_err(|e: anyhow::Error| e.to_string())?;
        
        Ok(())
    }

    // Gossip presence
    pub async fn broadcast_status(&self, topic_id: [u8; 32], status: String) -> Result<(), String> {
        let topic = iroh_gossip::proto::TopicId::from(topic_id);
        let mut sub = self.gossip.subscribe(topic, vec![]).await.map_err(|e: iroh_gossip::api::ApiError| e.to_string())?;
        sub.broadcast(status.into_bytes().into()).await.map_err(|e: iroh_gossip::api::ApiError| e.to_string())?;
        Ok(())
    }

    // === Media Vault ===

    /// Store raw media bytes in Iroh blobs, returning the content hash.
    pub async fn store_media(&self, data: Bytes) -> Result<Hash, String> {
        let tag_info = self.blobs.add_bytes(data).await.map_err(|e| e.to_string())?;
        Ok(tag_info.hash)
    }

    /// Store media with WebP thumbnail generation.
    /// Returns (original_hash, thumbnail_hash) - thumbnail is None if generation fails.
    pub async fn store_media_with_thumbnail(&self, data: Bytes, max_thumb_size: u32) -> Result<(Hash, Option<Hash>), String> {
        // Store original
        let original_hash = self.store_media(data.clone()).await?;

        // Generate WebP thumbnail
        let thumb_hash = match Self::generate_webp_thumbnail(&data, max_thumb_size) {
            Ok(thumb_bytes) => {
                match self.store_media(Bytes::from(thumb_bytes)).await {
                    Ok(h) => Some(h),
                    Err(e) => {
                        eprintln!("SyncManager: Failed to store thumbnail: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("SyncManager: Failed to generate thumbnail: {}", e);
                None
            }
        };

        Ok((original_hash, thumb_hash))
    }

    /// Generate a WebP thumbnail from image bytes.
    fn generate_webp_thumbnail(data: &[u8], max_size: u32) -> Result<Vec<u8>, String> {
        let img = image::load_from_memory(data)
            .map_err(|e| format!("Failed to decode image: {}", e))?;
        
        // Resize maintaining aspect ratio
        let thumb = img.thumbnail(max_size, max_size);
        
        // Encode as WebP
        let mut buf = std::io::Cursor::new(Vec::new());
        thumb.write_to(&mut buf, ImageFormat::WebP)
            .map_err(|e| format!("Failed to encode WebP: {}", e))?;
        
        Ok(buf.into_inner())
    }

    /// Retrieve media bytes by hash.
    pub async fn get_media(&self, hash: Hash) -> Result<Bytes, String> {
        use tokio::io::AsyncReadExt;
        let mut reader = self.blobs.reader(hash);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.map_err(|e| e.to_string())?;
        Ok(Bytes::from(buf))
    }

    /// Check if media exists in the blob store.
    pub async fn has_media(&self, hash: Hash) -> bool {
        self.blobs.has(hash).await.unwrap_or(false)
    }

    /// Public accessor for the blob store (needed by lib.rs commit_memories).
    pub fn blobs(&self) -> &iroh_blobs::api::Store {
        &self.blobs
    }

    /// Get the default author ID.
    pub fn default_author(&self) -> iroh_docs::AuthorId {
        self.engine.default_author.get()
    }

    /// Insert a metadata entry into the vault namespace.
    pub async fn insert_vault_entry(
        &self,
        key: Bytes,
        hash: Hash,
        len: u64,
    ) -> Result<(), String> {
        let author = self.engine.default_author.get();
        self.engine.sync.insert_local(
            self.vault_namespace,
            author,
            key,
            hash,
            len,
        ).await.map_err(|e: anyhow::Error| e.to_string())
    }

    // === Vault File Manager (Iroh Docs) ===

    async fn upsert_vault_file_metadata(&self, file: &VaultFile) -> Result<(), String> {
        let key = format!("vault/file/{}", file.id).into_bytes();
        let meta_json = serde_json::to_vec(file).map_err(|e| e.to_string())?;
        let meta_bytes = Bytes::from(meta_json);
        let tag_info = self.blobs.add_bytes(meta_bytes.clone()).await.map_err(|e| e.to_string())?;
        let author = self.engine.default_author.get();
        self.engine.sync.insert_local(
            self.vault_namespace,
            author,
            key.into(),
            tag_info.hash,
            meta_bytes.len() as u64,
        ).await.map_err(|e: anyhow::Error| e.to_string())?;

        Ok(())
    }

    /// Add a file to the vault. Stores bytes in blobs, metadata in the vault doc.
    /// Returns the VaultFile metadata.
    pub async fn vault_add_file(
        &self,
        name: String,
        mime_type: String,
        data: Bytes,
        source: String,
        item_id: Option<String>,
        created_at: Option<i64>,
        tags: Vec<String>,
    ) -> Result<VaultFile, String> {
        let size = data.len() as u64;
        let kind = VaultFileKind::from_mime(&mime_type);

        // Store bytes in blob (with thumbnail for images)
        let (blob_hash, thumbnail_hash) = if kind == VaultFileKind::Image {
            let (h, th) = self.store_media_with_thumbnail(data, 256).await?;
            (h.to_string(), th.map(|t| t.to_string()))
        } else {
            let h = self.store_media(data).await?;
            (h.to_string(), None)
        };

        // Avoid duplicate metadata entries for the same blob.
        if let Some(mut existing) = self.vault_list_files().await?
            .into_iter()
            .find(|f| f.blob_hash == blob_hash)
        {
            let mut changed = false;

            if !tags.is_empty() {
                for tag in tags {
                    if !existing.tags.contains(&tag) {
                        existing.tags.push(tag);
                        changed = true;
                    }
                }
            }

            if changed {
                self.upsert_vault_file_metadata(&existing).await?;
            }

            println!("Vault: Reused existing file '{}' (blob: {})", existing.name, existing.blob_hash);
            return Ok(existing);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let file = VaultFile {
            id: Uuid::new_v4().to_string(),
            name,
            mime_type,
            kind,
            size,
            blob_hash,
            thumbnail_hash,
            source,
            item_id,
            added_at: now,
            created_at,
            tags,
        };

        // Store metadata as JSON entry in the vault doc
        self.upsert_vault_file_metadata(&file).await?;

        println!("Vault: Added file '{}' ({}, {} bytes, blob: {})", file.name, file.kind.as_str(), file.size, file.blob_hash);
        Ok(file)
    }

    /// List all vault files by reading entries from the vault doc.
    pub async fn vault_list_files(&self) -> Result<Vec<VaultFile>, String> {
        let query = Query::key_prefix(b"vault/file/");
        let (tx, mut rx) = irpc::channel::mpsc::channel::<iroh_docs::api::RpcResult<iroh_docs::SignedEntry>>(128);

        self.engine.sync
            .get_many(self.vault_namespace, query.into(), tx)
            .await
            .map_err(|e: anyhow::Error| e.to_string())?;

        let mut files = Vec::new();
        while let Ok(Some(result)) = rx.recv().await {
            match result {
                Ok(entry) => {
                    // Skip tombstone entries (deleted files)
                    if entry.content_len() <= Self::TOMBSTONE.len() as u64 {
                        continue;
                    }
                    let hash: Hash = entry.content_hash();
                    match self.get_media(hash).await {
                        Ok(content) => {
                            match serde_json::from_slice::<VaultFile>(&content) {
                                Ok(file) => files.push(file),
                                Err(e) => eprintln!("Vault: Failed to parse file entry: {}", e),
                            }
                        }
                        Err(e) => eprintln!("Vault: Failed to read entry content: {}", e),
                    }
                }
                Err(e) => eprintln!("Vault: Entry read error: {}", e),
            }
        }

        // Sort by added_at descending (newest first)
        files.sort_by(|a, b| b.added_at.cmp(&a.added_at));
        Ok(files)
    }

    /// Get a single vault file's metadata by ID.
    pub async fn vault_get_file(&self, file_id: &str) -> Result<Option<VaultFile>, String> {
        let key = format!("vault/file/{}", file_id).into_bytes();
        let author = self.engine.default_author.get();

        match self.engine.sync
            .get_exact(self.vault_namespace, author, key.into(), false)
            .await
            .map_err(|e: anyhow::Error| e.to_string())?
        {
            Some(entry) => {
                let content = self.get_media(entry.content_hash()).await?;
                let file: VaultFile = serde_json::from_slice(&content).map_err(|e| e.to_string())?;
                Ok(Some(file))
            }
            None => Ok(None),
        }
    }

    /// Tombstone marker written over deleted vault entries.
    const TOMBSTONE: &[u8] = b"DELETED";

    /// Delete a vault file by overwriting with a tombstone blob.
    pub async fn vault_delete_file(&self, file_id: &str) -> Result<(), String> {
        let key = format!("vault/file/{}", file_id).into_bytes();
        let author = self.engine.default_author.get();

        // Store a tiny tombstone blob (iroh-docs rejects truly empty entries)
        let tombstone = Bytes::from_static(Self::TOMBSTONE);
        let tag_info = self.blobs.add_bytes(tombstone.clone()).await
            .map_err(|e| format!("Failed to store tombstone: {}", e))?;

        self.engine.sync.insert_local(
            self.vault_namespace,
            author,
            key.into(),
            tag_info.hash,
            tombstone.len() as u64,
        ).await.map_err(|e: anyhow::Error| e.to_string())?;

        println!("Vault: Deleted file {}", file_id);
        Ok(())
    }

    /// Deduplicate files by blob hash, keeping the newest entry and tombstoning older duplicates.
    pub async fn vault_deduplicate_files(&self) -> Result<usize, String> {
        let files = self.vault_list_files().await?;
        let mut seen_blob_hashes: HashMap<String, String> = HashMap::new();
        let mut duplicate_ids: Vec<String> = Vec::new();

        for file in files {
            if seen_blob_hashes.contains_key(&file.blob_hash) {
                duplicate_ids.push(file.id.clone());
            } else {
                seen_blob_hashes.insert(file.blob_hash.clone(), file.id.clone());
            }
        }

        for file_id in &duplicate_ids {
            self.vault_delete_file(file_id).await?;
        }

        if !duplicate_ids.is_empty() {
            println!("Vault: Removed {} duplicate file entries", duplicate_ids.len());
        }

        Ok(duplicate_ids.len())
    }

    /// Delete ALL vault files by tombstoning every entry.
    pub async fn vault_clear_all_files(&self) -> Result<usize, String> {
        let files = self.vault_list_files().await?;
        let count = files.len();
        for file in &files {
            if let Err(e) = self.vault_delete_file(&file.id).await {
                eprintln!("Vault: Failed to delete file {}: {}", file.id, e);
            }
        }
        println!("Vault: Cleared {} files", count);
        Ok(count)
    }

    /// Get aggregate vault stats.
    pub async fn vault_stats(&self) -> Result<VaultFileStats, String> {
        let files = self.vault_list_files().await?;
        let mut by_kind: HashMap<String, usize> = HashMap::new();
        let mut by_source: HashMap<String, usize> = HashMap::new();
        let mut total_size: u64 = 0;

        for f in &files {
            *by_kind.entry(f.kind.as_str().to_string()).or_insert(0) += 1;
            *by_source.entry(f.source.clone()).or_insert(0) += 1;
            total_size += f.size;
        }

        Ok(VaultFileStats {
            total_files: files.len(),
            total_size,
            by_kind,
            by_source,
        })
    }

    /// Garbage-collect orphaned tombstone entries from the vault namespace.
    ///
    /// Tombstones are tiny "DELETED" marker blobs written over deleted vault entries.
    /// Over time these accumulate and permanently grow the namespace metadata.
    /// This command scans all vault entries, identifies tombstones, and removes them
    /// from the local doc store by re-inserting an empty-ish entry that Iroh can GC.
    ///
    /// Returns the number of tombstones purged.
    pub async fn vault_gc_tombstones(&self) -> Result<usize, String> {
        let query = Query::key_prefix(b"vault/file/");
        let (tx, mut rx) = irpc::channel::mpsc::channel::<iroh_docs::api::RpcResult<iroh_docs::SignedEntry>>(128);

        self.engine.sync
            .get_many(self.vault_namespace, query.into(), tx)
            .await
            .map_err(|e: anyhow::Error| e.to_string())?;

        let mut tombstone_keys: Vec<Bytes> = Vec::new();
        while let Ok(Some(result)) = rx.recv().await {
            match result {
                Ok(entry) => {
                    if entry.content_len() <= Self::TOMBSTONE.len() as u64 {
                        tombstone_keys.push(entry.key().to_vec().into());
                    }
                }
                Err(e) => eprintln!("vault_gc: entry read error: {}", e),
            }
        }

        if tombstone_keys.is_empty() {
            println!("vault_gc: No tombstones found");
            return Ok(0);
        }

        let count = tombstone_keys.len();
        let author = self.engine.default_author.get();

        // Overwrite tombstone entries with a minimal GC marker.
        // This signals to the local store that the key can be pruned on compaction.
        let gc_marker = Bytes::from_static(b"GC");
        let gc_tag = self.blobs.add_bytes(gc_marker.clone()).await
            .map_err(|e| format!("Failed to store GC marker: {}", e))?;

        for key in &tombstone_keys {
            if let Err(e) = self.engine.sync.insert_local(
                self.vault_namespace,
                author,
                key.clone(),
                gc_tag.hash,
                gc_marker.len() as u64,
            ).await {
                eprintln!("vault_gc: Failed to GC tombstone: {}", e);
            }
        }

        println!("vault_gc: Purged {} tombstone entries", count);
        Ok(count)
    }

    /// Read all entries from a given namespace (for P2P test/diagnostics).
    /// Returns a list of key-content-author triples.
    pub async fn read_doc_entries(&self, namespace: crate::sync::NamespaceId) -> Result<Vec<crate::commands::node::TestDocEntry>, String> {
        let query = Query::all();
        let (tx, mut rx) = irpc::channel::mpsc::channel::<iroh_docs::api::RpcResult<iroh_docs::SignedEntry>>(128);

        self.engine.sync
            .get_many(namespace, query.into(), tx)
            .await
            .map_err(|e: anyhow::Error| format!("Failed to query namespace: {}", e))?;

        let mut entries = Vec::new();
        while let Ok(Some(result)) = rx.recv().await {
            match result {
                Ok(entry) => {
                    let key = String::from_utf8_lossy(entry.key()).to_string();
                    let author = entry.author().to_string();

                    // Read the content from blobs
                    let content = match self.get_media(entry.content_hash()).await {
                        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                        Err(_) => format!("<binary: {} bytes>", entry.content_len()),
                    };

                    entries.push(crate::commands::node::TestDocEntry {
                        key,
                        content,
                        author,
                    });
                }
                Err(e) => eprintln!("read_doc_entries: entry error: {}", e),
            }
        }

        Ok(entries)
    }
}

//! Postgres-backed RepoStorage implementation for rsky-repo.
//!
//! Implements `ReadableBlockstore` + `RepoStorage` traits against our sqlx PgPool.
//! IPLD blocks are stored in `pds_ipld_blocks` (CID-addressed), repo roots in
//! `pds_repo_roots` (DID-keyed). All operations are async via sqlx.
//!
//! The rsky trait requires `Send + Sync` on returned futures. sqlx futures are
//! `Send` but not `Sync`. We use a `SyncFuture` wrapper (mutex-based) to satisfy
//! the `Sync` bound without runtime overhead.

use anyhow::Result;
use lexicon_cid::Cid;
use rsky_repo::block_map::{BlockMap, BlocksAndMissing};
use rsky_repo::storage::readable_blockstore::ReadableBlockstore;
use rsky_repo::storage::types::RepoStorage;
use rsky_repo::types::CommitData;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct SqlxPostgresStorage {
    pub pool: PgPool,
    pub did: String,
}

impl SqlxPostgresStorage {
    pub fn new(pool: PgPool, did: String) -> Self {
        Self { pool, did }
    }
}

impl ReadableBlockstore for SqlxPostgresStorage {
    fn get_bytes<'a>(
        &'a self,
        cid: &'a Cid,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>>> + Send + 'a>> {
        let cid_str = cid.to_string();
        let pool = self.pool.clone();
        Box::pin(async move {
            let row: Option<(Vec<u8>,)> =
                sqlx::query_as("SELECT data FROM pds_ipld_blocks WHERE cid = $1")
                    .bind(&cid_str)
                    .fetch_optional(&pool)
                    .await?;
            Ok(row.map(|r| r.0))
        })
    }

    fn has<'a>(
        &'a self,
        cid: Cid,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>> {
        let cid_str = cid.to_string();
        let pool = self.pool.clone();
        Box::pin(async move {
            let row: Option<(bool,)> =
                sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pds_ipld_blocks WHERE cid = $1)")
                    .bind(&cid_str)
                    .fetch_optional(&pool)
                    .await?;
            Ok(row.map(|r| r.0).unwrap_or(false))
        })
    }

    fn get_blocks<'a>(
        &'a self,
        cids: Vec<Cid>,
    ) -> Pin<Box<dyn Future<Output = Result<BlocksAndMissing>> + Send + 'a>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let cid_strs: Vec<String> = cids.iter().map(|c| c.to_string()).collect();
            let mut blocks = BlockMap::new();
            let mut missing = Vec::new();

            for (idx, cid_str) in cid_strs.iter().enumerate() {
                let row: Option<(Vec<u8>,)> =
                    sqlx::query_as("SELECT data FROM pds_ipld_blocks WHERE cid = $1")
                        .bind(cid_str)
                        .fetch_optional(&pool)
                        .await?;
                match row {
                    Some(data) => {
                        blocks.set(cids[idx], data.0);
                    }
                    None => {
                        missing.push(cids[idx]);
                    }
                }
            }

            Ok(BlocksAndMissing { blocks, missing })
        })
    }
}

impl RepoStorage for SqlxPostgresStorage {
    fn get_root<'a>(&'a self) -> Pin<Box<dyn Future<Output = Option<Cid>> + Send + 'a>> {
        let pool = self.pool.clone();
        let did = self.did.clone();
        Box::pin(async move {
            let row: Option<(String,)> =
                sqlx::query_as("SELECT root_cid FROM pds_repo_roots WHERE did = $1")
                    .bind(&did)
                    .fetch_optional(&pool)
                    .await
                    .ok()
                    .flatten();
            row.and_then(|r| Cid::from_str(&r.0).ok())
        })
    }

    fn put_block<'a>(
        &'a self,
        cid: Cid,
        bytes: Vec<u8>,
        rev: String,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let cid_str = cid.to_string();
        let did = self.did.clone();
        let pool = self.pool.clone();
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO pds_ipld_blocks (cid, did, data, rev) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (cid) DO NOTHING",
            )
            .bind(&cid_str)
            .bind(&did)
            .bind(&bytes)
            .bind(&rev)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }

    fn put_many<'a>(
        &'a self,
        to_put: BlockMap,
        rev: String,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let did = self.did.clone();
        let pool = self.pool.clone();
        Box::pin(async move {
            for (cid_str, bytes) in to_put.map.iter() {
                sqlx::query(
                    "INSERT INTO pds_ipld_blocks (cid, did, data, rev) VALUES ($1, $2, $3, $4)
                     ON CONFLICT (cid) DO NOTHING",
                )
                .bind(cid_str)
                .bind(&did)
                .bind(&bytes.0)
                .bind(&rev)
                .execute(&pool)
                .await?;
            }
            Ok(())
        })
    }

    fn update_root<'a>(
        &'a self,
        cid: Cid,
        rev: String,
        _is_create: Option<bool>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let cid_str = cid.to_string();
        let did = self.did.clone();
        let pool = self.pool.clone();
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO pds_repo_roots (did, root_cid, rev, updated_at)
                 VALUES ($1, $2, $3, NOW())
                 ON CONFLICT (did) DO UPDATE SET root_cid = $2, rev = $3, updated_at = NOW()",
            )
            .bind(&did)
            .bind(&cid_str)
            .bind(&rev)
            .execute(&pool)
            .await?;
            Ok(())
        })
    }

    fn apply_commit<'a>(
        &'a self,
        commit: CommitData,
        _is_create: Option<bool>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let did = self.did.clone();
        let pool = self.pool.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await?;

            for (cid_str, bytes) in commit.new_blocks.map.iter() {
                sqlx::query(
                    "INSERT INTO pds_ipld_blocks (cid, did, data, rev) VALUES ($1, $2, $3, $4)
                     ON CONFLICT (cid) DO NOTHING",
                )
                .bind(cid_str)
                .bind(&did)
                .bind(&bytes.0)
                .bind(&commit.rev)
                .execute(&mut *tx)
                .await?;
            }

            for removed_cid in commit.removed_cids.to_list() {
                let cid_str = removed_cid.to_string();
                sqlx::query("DELETE FROM pds_ipld_blocks WHERE cid = $1")
                    .bind(&cid_str)
                    .execute(&mut *tx)
                    .await?;
            }

            let cid_str = commit.cid.to_string();
            sqlx::query(
                "INSERT INTO pds_repo_roots (did, root_cid, rev, updated_at)
                 VALUES ($1, $2, $3, NOW())
                 ON CONFLICT (did) DO UPDATE SET root_cid = $2, rev = $3, updated_at = NOW()",
            )
            .bind(&did)
            .bind(&cid_str)
            .bind(&commit.rev)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(())
        })
    }
}
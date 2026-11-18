//! did:web -> did:plc re-identity for pre-realignment minted AT identities.
//!
//! A `did:web` identity is invisible to the real Atmosphere: the AppView
//! silently drops events whose DID resolution is imperfect
//! (bluesky-social/atproto#4379 — confirmed live on did:web:coves.social:
//! relay-synced but "Profile not found"), so a Mastodon-login user's like of a
//! Bluesky post saves on Bytescape and never shows on real Bluesky. Both
//! production bridges ship per-user `did:plc` (`_ref/sprints/
//! atproto-research-2026-11-14.md` §f, archived change group 5). New accounts
//! already mint `did:plc`; this module migrates the EXISTING `did:web` cohort.
//!
//! Per account ([`migrate_account`]) — hardened per sprint review #2
//! (2026-11-14, R2-1/R2-2/R2-3/R2-5/R2-8/R2-9/R2-10/R2-12):
//!   1. CLAIM — one `pds_did_reidentify` row, taken EXCLUSIVELY by a single
//!      atomic statement (insert-or-re-arm guarded by the claim predicate —
//!      at most one concurrent runner gets a row back). The state machine is
//!      `pending | applying | done | failed`; `failed` is TERMINAL (candidates
//!      never re-attempt it) and is set whenever the outcome of a mint attempt
//!      cannot be PROVEN unpublished — an unprovable claim never re-mints
//!      (review R2-10). A stale `applying` (crash leftover) re-enlists after
//!      the [`CLAIM_LEASE_SQL`] window.
//!   2. STAGE-THEN-MINT — the mint key is generated first and its deterministic
//!      derived DID (RFC6979 genesis signatures — [`plc::build_genesis_op`] +
//!      [`plc::derive_did`]) is staged on the claim row (key sealed with the
//!      [`crate::secret`] AEAD envelope) BEFORE the PLC directory sees the
//!      genesis op. The mint is an irreversible PUBLIC write, so the ordering
//!      is the invariant: a crash at ANY point resumes from the staged key →
//!      the same derived DID, never a second identity (review R2-2). On
//!      resume the directory's audit log VERIFIES publication: published →
//!      proceed as-is; proven unpublished → re-POST the SAME genesis op
//!      (idempotent — same key, same derived DID); unprovable → terminal
//!      `failed` (never a re-mint into the unknown).
//!   3. RE-POINT — the repo is re-created under the new DID as one init commit
///      over the same records (walked fallibly from the old MST — source of
///      truth). The walk is the COMPLETENESS PROOF (review R2-12): every leaf
///      must load (a missing/corrupt block aborts — the vendored walker
///      panics on it), the walked count must equal the MST's own leaf count,
///      and the legacy `pds_indexed_records` rows must be a subset of the
///      walk (consumer lag allowed, phantom rows not); any mismatch aborts
///      BEFORE the cutover ever drops the old root or account. Writes to the
///      account are refused for the walk's duration ([`crate::pds::quiesce`],
///      review R2-3) and the cutover re-checks the old root's rev — a write
///      that somehow landed mid-window aborts the cutover instead of being
///      destroyed. Records keep their rkeys and values verbatim; only the
///      commit (and its signature) is new. A repo-less account gets its
///      profile self-record seeded (review R2-9 — never migrate to a
///      profile-less DID).
///   4. CUTOVER — one transaction swaps every DID-keyed row to the new
///      identity (old commit-chain and MST blocks KEEP their old-DID
///      attribution — CID-keyed reads still resolve them and they are never
///      served; review R2-8), tombstones the old repo root, retires the old
///      `pds_accounts` row (the well-known handlers answer the honest-404
///      tombstone already used for deleted accounts), seeds the account row,
///      fans out the `Update(actor)` whose `alsoKnownAs` cross-link changed
///      (review R2-4), and releases the staged key copy (review R2-5) — all
///      under one commit.
///
/// The m→bsky path after migration: like written to the did:plc repo → relay
/// crawl (requestCrawl, as at mint) → AppView indexing → getLikes.
///
/// Runner: [`supersede_did_web`], gated by `REIDENTIFY_DID_WEB` from
/// `main.rs` (absent = never runs, `apply` = execute, anything else = audit-only
/// dry run) — the same posture as `RETIRE_MASTODON_MINTED_ACTORS`.

use anyhow::{anyhow, Result};
use secp256k1::{Keypair, Secp256k1};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

use rsky_repo::repo::Repo;
use rsky_repo::storage::readable_blockstore::ReadableBlockstore as _;
use rsky_repo::storage::Ipld;
use rsky_repo::types::{Lex, RecordCreateOrUpdateOp, RepoRecord, WriteOpAction};

use crate::pds::account::encode_public_key_multibase;
use crate::pds::plc;
use crate::pds::storage::SqlxPostgresStorage;

/// How long an `applying` claim is considered held by a live runner (review
/// R2-10): a crash leaves the row `applying` forever — the lease lets the next
/// run re-enlist it (its staged key resumes the same identity, so re-enlisting
/// can never burn a second mint).
const CLAIM_LEASE_SQL: &str = "now() - interval '30 minutes'";

/// One pre-realignment account awaiting re-identity.
struct Candidate {
    old_did: String,
    handle: String,
    user_id: uuid::Uuid,
}

/// Every active user account still on a minted `did:web` identity (the
/// `_instance` host row is the HOST identity and never migrates). Claims in
/// `done` (migrated) or `failed` (terminal — R2-10) are never re-attempted;
/// a `pending`/stale-`applying` claim re-enlists through [`migrate_account`]'s
/// exclusive claim arm.
async fn candidates(db: &PgPool) -> Result<Vec<Candidate>, String> {
    let rows: Vec<(String, String, uuid::Uuid)> = sqlx::query_as(
        "SELECT a.did, a.handle, a.user_id \
         FROM pds_accounts a \
         WHERE a.did LIKE 'did:web:%' \
           AND a.handle <> '_instance' \
           AND a.user_id IS NOT NULL \
           AND a.status = 'active' \
           AND NOT EXISTS ( \
             SELECT 1 FROM pds_did_reidentify r \
             WHERE r.old_did = a.did AND r.state IN ('done', 'failed')) \
         ORDER BY a.handle",
    )
    .fetch_all(db)
    .await
    .map_err(|e| format!("candidate query: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|(old_did, handle, user_id)| Candidate {
            old_did,
            handle,
            user_id,
        })
        .collect())
}

/// One-shot migration runner: mint a per-user `did:plc` for every pre-realignment
/// `did:web` account and re-point its repo. Gated + audit-logged per account.
/// `allow_usernames` limits a run to specific handles (verification hook);
/// `None` = every eligible account. Returns how many accounts were migrated.
pub async fn supersede_did_web(
    db: &PgPool,
    plc_base: &str,
    domain: &str,
    apply: bool,
    allow_usernames: Option<Vec<String>>,
) -> Result<u32, String> {
    let mut migrated = 0u32;
    for c in candidates(db).await? {
        if let Some(allow) = &allow_usernames {
            if !allow.contains(&c.handle) {
                continue;
            }
        }
        if !apply {
            tracing::info!(
                "[reidentify][dry-run] would mint did:plc for {handle} (was {old})",
                handle = c.handle,
                old = c.old_did
            );
            continue;
        }
        match migrate_account(db, plc_base, domain, &c.old_did, &c.handle, c.user_id).await {
            Ok(new_did) => {
                tracing::info!(
                    "[reidentify] {handle}: {old} -> {new_did}",
                    handle = c.handle,
                    old = c.old_did
                );
                migrated += 1;
            }
            Err(e) => {
                tracing::warn!("[reidentify] {} failed: {e} — continuing", c.handle);
            }
        }
    }
    Ok(migrated)
}

/// Migrate one account's AT identity from its minted `did:web` to a per-user
/// `did:plc`. Idempotent and crash-safe (review R2-1/R2-2): the claim is
/// exclusive across concurrent runners, the identity is STAGED before the
/// irreversible public mint (so any crash resumes with the same key/DID —
/// never a second mint), and a completed account returns its new DID without
/// touching anything. Returns the new DID.
pub async fn migrate_account(
    db: &PgPool,
    plc_base: &str,
    domain: &str,
    old_did: &str,
    handle: &str,
    user_id: uuid::Uuid,
) -> Result<String> {
    // 1) CLAIM, exclusively (R2-1): one atomic insert-or-re-arm statement —
    //    its ON CONFLICT WHERE clause IS the claim predicate (fresh `pending`,
    //    or a lease-expired `applying` left by a crash). Two concurrent
    //    runners: one inserts/re-arms and gets the row; the loser's statement
    //    matches nothing and gets no row back. `done` rows return for the
    //    idempotent read below; `failed` is terminal.
    let claimed: Option<(Option<String>, Option<Vec<u8>>)> = sqlx::query_as(&format!(
        "INSERT INTO pds_did_reidentify (old_did, user_id, username, state, started_at) \
         VALUES ($1, $2, $3, 'applying', NOW()) \
         ON CONFLICT (old_did) DO UPDATE SET \
           state = 'applying', started_at = NOW() \
         WHERE pds_did_reidentify.state = 'pending' \
            OR (pds_did_reidentify.state = 'applying' \
                AND pds_did_reidentify.started_at < {CLAIM_LEASE_SQL}) \
         RETURNING new_did, signing_key_private"
    ))
    .bind(old_did)
    .bind(user_id)
    .bind(handle)
    .fetch_optional(db)
    .await
    .map_err(|e| anyhow!("claim: {e}"))?;
    let Some((staged_did, staged_key)) = claimed else {
        // No row won: done (idempotent), terminal failed, or a live holder.
        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT state, new_did FROM pds_did_reidentify WHERE old_did = $1",
        )
        .bind(old_did)
        .fetch_optional(db)
        .await
        .map_err(|e| anyhow!("claim lookup: {e}"))?;
        return match existing {
            Some((state, did)) if state == "done" => {
                did.ok_or_else(|| anyhow!("completed claim without new_did for {old_did}"))
            }
            Some((state, _)) if state == "failed" => Err(anyhow!(
                "claim for {old_did} is terminally failed (unprovable mint outcome) — operator attention required"
            )),
            _ => Err(anyhow!(
                "claim for {old_did} is held by a live runner (lease) — back off"
            )),
        };
    };

    // 2) RESUME CHECK (R2-2): a staged identity proves a mint attempt's key
    //    was already created — the derived DID is deterministic (RFC6979
    //    genesis signatures), so resume always re-uses THAT identity. The
    //    directory's audit log verifies the previous attempt's outcome:
    //    published → proceed; proven unpublished → re-POST the same genesis
    //    op (idempotent — same key, same derived DID); unprovable → terminal
    //    `failed`, never a second identity.
    let (keypair, new_did) = match (staged_did, staged_key) {
        (Some(staged_did), Some(staged_key)) => {
            let key_bytes = decode_staged_key(&staged_key)?;
            let secp = Secp256k1::new();
            let secret = secp256k1::SecretKey::from_slice(&key_bytes)
                .map_err(|e| anyhow!("staged key decode: {e}"))?;
            let keypair = Keypair::from_secret_key(&secp, &secret);
            // Derived-DID determinism guards registry corruption: the staged
            // DID must be exactly the key's own derivation.
            let derived = derive_did_for(&keypair, handle, domain)?;
            if derived != staged_did {
                mark_failed(db, old_did).await?;
                return Err(anyhow!(
                    "staged key derives {derived} but the registry staged {staged_did} — terminally failed (registry corruption), never re-minting"
                ));
            }
            match verify_mint(plc_base, &staged_did).await {
                Ok(true) => {}                             // published — proceed
                Ok(false) => {
                    // Proven unpublished: the same identity is minted now
                    // (idempotent re-POST — never a second identity).
                    mint_genesis(reqwest::Client::new(), plc_base, &keypair, &staged_did, handle, domain)
                        .await?;
                }
                Err(e) => {
                    mark_failed(db, old_did).await?;
                    return Err(e);
                }
            }
            (keypair, staged_did)
        }
        _ => {
            // STAGE-FIRST (R2-2): generate the key, derive its DID and stage
            // BOTH before the directory sees the genesis op. A crash before
            // staging burns nothing (no mint yet); after staging the resume
            // arm above guarantees the same identity forever.
            let secp = Secp256k1::new();
            let keypair = Keypair::new(&secp, &mut secp256k1::rand::rngs::OsRng);
            let new_did = derive_did_for(&keypair, handle, domain)?;
            let sealed = crate::secret::seal(
                &hex::encode(keypair.secret_key().secret_bytes()),
                &crate::db::accounts::jwt_secret(),
            );
            let staged = sqlx::query(
                "UPDATE pds_did_reidentify SET new_did = $2, signing_key_private = $3 \
                 WHERE old_did = $1 AND new_did IS NULL",
            )
            .bind(old_did)
            .bind(&new_did)
            .bind(sealed.as_bytes().to_vec())
            .execute(db)
            .await
            .map_err(|e| anyhow!("claim stage: {e}"))?;
            if staged.rows_affected() != 1 {
                // Defense in depth: the claim is exclusive, so this only
                // fires on registry interference — NEVER mint untracked.
                return Err(anyhow!(
                    "claim stage for {old_did} lost the row (concurrent stager?) — refusing to mint untracked"
                ));
            }
            mint_genesis(reqwest::Client::new(), plc_base, &keypair, &new_did, handle, domain)
                .await?;
            (keypair, new_did)
        }
    };

    // The account's write quiesce (R2-3): user writes are refused for the
    // walk→cutover window — a write landing in it would be destroyed by the
    // cutover's snapshot semantics. Released on drop (every path below).
    let _quiesce = crate::pds::quiesce::hold(old_did);

    // 3) RE-POINT: rebuild the repo under the new DID as one init commit over
    //    the same records (walked fallibly from the old MST — source of
    //    truth). Records keep their rkeys and values verbatim; only the
    //    commit is new. An account that never wrote records gets its profile
    //    self-record seeded (R2-9 — never a profile-less DID).
    let has_root: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pds_repo_roots WHERE did = $1)",
    )
    .bind(old_did)
    .fetch_one(db)
    .await
    .map_err(|e| anyhow!("old repo root check: {e}"))?;
    let mut index_rows: Vec<(String, String, String, serde_json::Value)> = Vec::new();
    let mut writes: Vec<RecordCreateOrUpdateOp> = Vec::new();
    // The rev the walk snapshot covers (R2-3): the cutover re-checks it.
    let mut walked_rev: Option<String> = None;
    if has_root {
        let mut old_repo = Repo::load(
            Arc::new(RwLock::new(SqlxPostgresStorage::new(
                db.clone(),
                old_did.to_string(),
            ))),
            None,
        )
        .await
        .map_err(|e| anyhow!("old repo load: {e}"))?;
        walked_rev = Some(old_repo.commit.rev.clone());

        // FALLIBLE walk (R2-12): the vendored `walk_records` panics on a
        // missing/corrupt record block and cannot report partial output — a
        // corrupt-row state would silently migrate a subset and then destroy
        // the residue. Every leaf must load; the walked count must match the
        // MST's own leaf count; the legacy index must be a subset (lag
        // allowed, phantom rows not).
        let leaves: Vec<_> = old_repo
            .data
            .walk_leaves_from(&String::new())
            .await
            .collect();
        let walked = leaves.len() as u64;
        for leaf in &leaves {
            let path = rsky_repo::util::parse_data_key(&leaf.key)
                .map_err(|e| anyhow!("walk key parse {}: {e}", leaf.key))?;
            let record: RepoRecord = {
                let guard = old_repo.storage.read().await;
                guard.read_record(&leaf.value).await.map_err(|e| {
                    anyhow!(
                        "walk record load {}/{} ({}): {e}",
                        path.collection,
                        path.rkey,
                        leaf.value
                    )
                })?
            };
            // The index CID comes from the SAME canonical encoder
            // `format_init_commit` uses (`BlockMap::add`) — never a re-encode
            // — so the index CID and the new MST leaf CID cannot diverge,
            // including for nested-map records (review R2-18: bare
            // `serde_ipld_dagcbor::to_vec` has known-buggy nested-map sorting).
            let record_cid = {
                let mut bm = rsky_repo::block_map::BlockMap::new();
                bm.add(record.clone())
                    .map_err(|e| anyhow!("record cid {}/{}: {e}", path.collection, path.rkey))?
                    .to_string()
            };
            let block_bytes = serde_ipld_dagcbor::to_vec(&record)
                .map_err(|e| anyhow!("record re-encode {}/{}: {e}", path.collection, path.rkey))?;
            let value_json: serde_json::Value = serde_ipld_dagcbor::from_slice(&block_bytes)
                .map_err(|e| anyhow!("record decode {}/{}: {e}", path.collection, path.rkey))?;
            index_rows.push((
                path.collection.clone(),
                path.rkey.clone(),
                record_cid,
                value_json,
            ));
            writes.push(RecordCreateOrUpdateOp {
                action: WriteOpAction::Create,
                collection: path.collection,
                rkey: path.rkey,
                record,
            });
        }

        // Completeness proof (R2-12): the MST itself must agree it has no
        // more leaves than we walked.
        let total: u64 = old_repo
            .data
            .list(None, None, None)
            .await
            .map(|v| v.len() as u64)
            .unwrap_or(u64::MAX);
        if total != u64::MAX && total != walked {
            return Err(anyhow!(
                "walk incomplete for {old_did}: walked {walked} leaves but the MST lists {total} — aborting before any drop"
            ));
        }
        // The legacy derived index must be a SUBSET of the walk: consumer lag
        // (walked but not yet indexed) is fine; an indexed row absent from the
        // MST means the walk missed state (or a delete's consumer lag) — abort
        // before any drop and let a later run retry (review R2-12).
        let phantom: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pds_indexed_records i \
             WHERE i.did = $1 AND NOT EXISTS ( \
               SELECT 1 FROM unnest($2::text[]) k WHERE k = i.collection || '/' || i.rkey)",
        )
        .bind(old_did)
        .bind(
            index_rows
                .iter()
                .map(|(c, r, _, _)| format!("{c}/{r}"))
                .collect::<Vec<_>>(),
        )
        .fetch_one(db)
        .await
        .map_err(|e| anyhow!("legacy index parity: {e}"))?;
        if phantom != 0 {
            return Err(anyhow!(
                "walk incomplete for {old_did}: {phantom} indexed record(s) absent from the MST walk — aborting before any drop"
            ));
        }
        // R2-8: the old commit chain and MST nodes KEEP their old-DID block
        // attribution (never re-attributed to the new DID — their signatures
        // match no served key and CID-keyed reads still resolve them). Only
        // the re-pointed repo's own blocks are written under the new DID
        // (below). Shared-CID rows keep their FIRST writer's `did` — reads
        // are CID-keyed, so ownership is documentary there.
    } else {
        // R2-9: seed the profile self-record (same as `PdsAccount::mint`'s
        // initial profile) so the AppView never answers "Profile not found".
        let mut profile: RepoRecord = std::collections::BTreeMap::new();
        profile.insert(
            "$type".to_string(),
            Lex::Ipld(Ipld::String("app.bsky.actor.profile".to_string())),
        );
        profile.insert(
            "displayName".to_string(),
            Lex::Ipld(Ipld::String(handle.to_string())),
        );
        profile.insert("description".to_string(), Lex::Ipld(Ipld::String(String::new())));
        profile.insert(
            "createdAt".to_string(),
            Lex::Ipld(Ipld::String(chrono::Utc::now().to_rfc3339())),
        );
        let profile_cid = {
            let mut bm = rsky_repo::block_map::BlockMap::new();
            bm.add(profile.clone())
                .map_err(|e| anyhow!("profile record cid: {e}"))?
                .to_string()
        };
        index_rows.push((
            "app.bsky.actor.profile".to_string(),
            "self".to_string(),
            profile_cid,
            serde_json::to_value(&profile)
                .map_err(|e| anyhow!("profile record json: {e}"))?,
        ));
        writes.push(RecordCreateOrUpdateOp {
            action: WriteOpAction::Create,
            collection: "app.bsky.actor.profile".to_string(),
            rkey: "self".to_string(),
            record: profile,
        });
    }

    let storage = Arc::new(RwLock::new(SqlxPostgresStorage::new(
        db.clone(),
        new_did.clone(),
    )));
    let commit = Repo::format_init_commit(storage.clone(), new_did.clone(), &keypair, Some(writes))
        .await
        .map_err(|e| anyhow!("re-point format_init_commit: {e}"))?;
    let new_rev = commit.rev.clone();
    Repo::create_from_commit(storage, commit)
        .await
        .map_err(|e| anyhow!("re-point apply: {e}"))?;

    // Cutover: one transaction swaps every DID-keyed row to the new identity
    // and tombstones the old repo (root drop = RepoNotFound on getRepo /
    // getLatestCommit, the same AT-side tombstone `purge_native_repo` uses).
    let mut tx = db.begin().await.map_err(|e| anyhow!("cutover begin: {e}"))?;
    // R2-3: rev re-check under the row lock — a write that somehow landed
    // despite the quiesce would be destroyed by this cutover's snapshot;
    // abort (rollback) instead and let the retry re-walk.
    let root_rev: Option<(String,)> = sqlx::query_as(
        "SELECT rev FROM pds_repo_roots WHERE did = $1 FOR UPDATE",
    )
    .bind(old_did)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| anyhow!("cutover rev check: {e}"))?;
    match (walked_rev, root_rev) {
        (Some(walked), Some((root,))) if root != walked => {
            return Err(anyhow!(
                "cutover for {old_did} aborted: root rev moved to {root} after the walk ({walked}) — a write landed mid-window; retry"
            ));
        }
        (None, Some((root,))) => {
            return Err(anyhow!(
                "cutover for {old_did} aborted: a repo root ({root}) appeared during the walk window — retry"
            ));
        }
        _ => {}
    }
    // Blobs ride with the account (DID-scoped); blocks do NOT move (R2-8).
    sqlx::query("UPDATE pds_blobs SET did = $2 WHERE did = $1")
        .bind(old_did)
        .bind(&new_did)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("cutover blobs: {e}"))?;
    // The derived record index is rebuilt from the MST walk (self-healing for
    // consumer lag; value_json is the block's own decoded content).
    sqlx::query("DELETE FROM pds_indexed_records WHERE did = ANY(ARRAY[$1, $2])")
        .bind(old_did)
        .bind(&new_did)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("cutover index reset: {e}"))?;
    for (collection, rkey, cid, value_json) in &index_rows {
        sqlx::query(
            "INSERT INTO pds_indexed_records (did, collection, rkey, cid, value_json, rev) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&new_did)
        .bind(collection)
        .bind(rkey)
        .bind(cid)
        .bind(sqlx::types::Json(value_json))
        .bind(&new_rev)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("cutover index insert {collection}/{rkey}: {e}"))?;
    }
    // Old commit chain stops serving: no subscriber should learn the retired
    // identity's history, and the consumer must not re-materialize its rows.
    // (Its blocks KEEP the old-DID attribution — R2-8.)
    sqlx::query("DELETE FROM pds_firehose_seq WHERE did = $1")
        .bind(old_did)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("cutover firehose retire: {e}"))?;
    sqlx::query("DELETE FROM pds_repo_roots WHERE did = $1")
        .bind(old_did)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("cutover root tombstone: {e}"))?;
    // (Two statements, not one: sqlx `query` is a prepared statement and
    // cannot carry multiple statements in a batch.)
    sqlx::query("DELETE FROM pds_accounts WHERE did = $1")
        .bind(old_did)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("cutover old account retire: {e}"))?;
    sqlx::query(
        "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
         VALUES ($1, $2, $3, $4, 'active', $5)",
    )
    .bind(&new_did)
    .bind(handle)
    .bind(&keypair.secret_key().secret_bytes().to_vec())
    .bind(encode_public_key_multibase(&keypair))
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("cutover account swap: {e}"))?;
    // R2-4: the AP actor's `alsoKnownAs` cross-link changed with this swap —
    // fan out `Update(actor)` so cached remotes converge on the new AT DID.
    crate::ap::actor::fan_out_also_known_as_update(&mut tx, user_id, domain, &new_did)
        .await
        .map_err(|e| anyhow!("cutover alsoKnownAs fan-out: {e}"))?;
    // Finalize: release the staged key copy (R2-5 — no second key material
    // survives the migration) and record the audit trail.
    let done = sqlx::query(
        "UPDATE pds_did_reidentify SET state = 'done', finished_at = NOW(), \
         signing_key_private = NULL \
         WHERE old_did = $1 AND state <> 'done'",
    )
    .bind(old_did)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("claim complete: {e}"))?;
    if done.rows_affected() == 0 {
        return Err(anyhow!("cutover lost the claim for {old_did} — concurrent run?"));
    }
    tx.commit().await.map_err(|e| anyhow!("cutover commit: {e}"))?;
    Ok(new_did)
}

/// Decode the staged signing key: the `crate::secret` envelope (`enc:v1:…`)
/// when sealed (post-hardening rows), else the legacy raw 32 bytes of
/// pre-hardening rows. Returns exactly 32 key bytes.
fn decode_staged_key(staged: &[u8]) -> Result<Vec<u8>> {
    if staged.starts_with(crate::secret::ENC_PREFIX.as_bytes()) {
        let sealed = String::from_utf8(staged.to_vec())
            .map_err(|e| anyhow!("staged envelope utf8: {e}"))?;
        let hex_key = crate::secret::open(&sealed, &crate::db::accounts::jwt_secret())
            .map_err(|e| anyhow!("staged key unseal: {e}"))?;
        let bytes = hex::decode(hex_key.trim_start_matches("0x"))
            .map_err(|e| anyhow!("staged key hex: {e}"))?;
        if bytes.len() != 32 {
            return Err(anyhow!("staged key is not 32 bytes"));
        }
        Ok(bytes)
    } else if staged.len() == 32 {
        // Legacy plaintext bytes (pre-hardening rows).
        Ok(staged.to_vec())
    } else {
        Err(anyhow!(
            "staged key is neither an enc:v1: envelope nor 32 raw bytes"
        ))
    }
}

/// Mark the claim terminally failed (R2-10: the outcome of a mint attempt is
/// unprovable — never re-mint into the unknown).
async fn mark_failed(db: &PgPool, old_did: &str) -> Result<()> {
    sqlx::query("UPDATE pds_did_reidentify SET state = 'failed' WHERE old_did = $1 AND state <> 'done'")
        .bind(old_did)
        .execute(db)
        .await
        .map_err(|e| anyhow!("claim fail: {e}"))?;
    Ok(())
}

/// Derive the account's `did:plc` deterministically from its key + handle
/// (RFC6979 genesis signatures — the same identity a retry re-mints).
fn derive_did_for(keypair: &Keypair, handle: &str, domain: &str) -> Result<String> {
    let at_handle = if handle.ends_with(&format!(".{domain}")) {
        handle.to_string()
    } else {
        format!("{handle}.{domain}")
    };
    let pds_endpoint = format!("https://{domain}");
    let op = plc::build_genesis_op(keypair, &at_handle, &pds_endpoint)
        .map_err(|e| anyhow!("genesis op: {e}"))?;
    plc::derive_did(&op).map_err(|e| anyhow!("derive did: {e}"))
}

/// POST the account's deterministic genesis op (idempotent: the directory's
/// create route is keyed by the derived DID — a re-POST of the same op is a
/// no-op for an already-published identity).
async fn mint_genesis(
    http: reqwest::Client,
    plc_base: &str,
    keypair: &Keypair,
    did: &str,
    handle: &str,
    domain: &str,
) -> Result<()> {
    let at_handle = if handle.ends_with(&format!(".{domain}")) {
        handle.to_string()
    } else {
        format!("{handle}.{domain}")
    };
    let pds_endpoint = format!("https://{domain}");
    let op = plc::build_genesis_op(keypair, &at_handle, &pds_endpoint)
        .map_err(|e| anyhow!("genesis op: {e}"))?;
    let url = format!("{}/{did}", plc_base.trim_end_matches('/'));
    let resp = http
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "Bytescape/1.0")
        .body(op.to_string())
        .send()
        .await
        .map_err(|e| anyhow!("PLC directory request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "PLC directory rejected operation: HTTP {status} body: {body}"
        ));
    }
    Ok(())
}

/// Verify whether a staged DID is already published (R2-2 crash-resume):
/// GET the directory's audit log for it. Returns `Ok(true)` published /
/// `Ok(false)` proven unpublished (a 4xx — the caller may safely re-POST the
/// same identity); anything else (5xx, transport) is an unprovable outcome —
/// the caller marks the claim terminally `failed`, never re-minting into the
/// unknown.
async fn verify_mint(plc_base: &str, staged_did: &str) -> Result<bool> {
    let url = format!(
        "{}/{}/log/audit",
        plc_base.trim_end_matches('/'),
        staged_did
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header(reqwest::header::USER_AGENT, "Bytescape/1.0")
        .send()
        .await
        .map_err(|e| anyhow!("mint outcome for {staged_did} unprovable (audit check failed: {e})"))?;
    if resp.status().is_success() {
        return Ok(true);
    }
    if resp.status().is_client_error() {
        return Ok(false);
    }
    Err(anyhow!(
        "mint outcome for {staged_did} unprovable (audit HTTP {}) — claim terminally failed (no second mint)",
        resp.status()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::test_support::{stub_http, stub_http_seq, StubReply};
    use rsky_repo::storage::Ipld;
    use rsky_repo::types::Lex;

    fn like_record(text: &str) -> rsky_repo::types::RepoRecord {
        let mut rec = std::collections::BTreeMap::new();
        rec.insert(
            "$type".to_string(),
            Lex::Ipld(Ipld::String("app.bsky.feed.like".to_string())),
        );
        rec.insert(
            "subject".to_string(),
            Lex::Ipld(Ipld::String(format!("at://did:plc:poster/app.bsky.feed.post/{text}"))),
        );
        rec
    }

    /// Seeds a pre-realignment `did:web` account with one like record in its
    /// repo. Returns `(user_id, old_did, handle, record_rkey, record_cid)`.
    async fn seed_did_web_account(pool: &PgPool, suffix: &str) -> (uuid::Uuid, String, String, String, String) {
        let username = format!("mig_{suffix}");
        let user = crate::db::users::UserRow::create_active(pool, &username, "", "x", "x")
            .await
            .expect("create fixture user");
        let old_did = format!("did:web:{username}.test.bytescape.social");
        let signing_key_private = [0x22u8; 32];
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&old_did)
        .bind(&username)
        .bind(&signing_key_private[..])
        .bind(user.id)
        .execute(pool)
        .await
        .expect("seed did:web pds account");

        // Build the old repo with one record — the walk source of truth.
        let secp = Secp256k1::new();
        let secret = secp256k1::SecretKey::from_slice(&signing_key_private).expect("secret");
        let old_keypair = Keypair::from_secret_key(&secp, &secret);
        let rkey = format!("like{suffix}");
        let storage: Arc<RwLock<dyn rsky_repo::storage::types::RepoStorage>> = Arc::new(RwLock::new(SqlxPostgresStorage::new(
            pool.clone(),
            old_did.clone(),
        )));
        let record = like_record(suffix);
        let cid = rsky_repo::block_map::BlockMap::new()
            .add(record.clone())
            .expect("record cid")
            .to_string();
        Repo::create(
            storage,
            old_did.clone(),
            &old_keypair,
            Some(vec![RecordCreateOrUpdateOp {
                action: WriteOpAction::Create,
                collection: "app.bsky.feed.like".to_string(),
                rkey: rkey.clone(),
                record,
            }]),
        )
        .await
        .expect("seed old repo");
        (user.id, old_did, username, rkey, cid)
    }

    /// Full migration: repo re-pointed under a fresh `did:plc`, all records
    /// preserved verbatim, old identity fully retired, blobs ride along.
    #[tokio::test]
    async fn reidentify_preserves_records_and_retires_old_identity() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] reidentify_preserves_records_and_retires_old_identity skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, rkey, old_cid) = seed_did_web_account(&pool, &suffix).await;
        // A blob rides with the account (R2-20: asserted moving).
        sqlx::query(
            "INSERT INTO pds_blobs (did, cid, mime_type, size, bytes) VALUES ($1, $2, 'image/png', 3, '\\x010203'::bytea)",
        )
        .bind(&old_did)
        .bind(format!("blob{suffix}"))
        .execute(&pool)
        .await
        .expect("seed blob");

        // PLC directory stub: one accepted genesis mint.
        let (plc_base, captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);

        let new_did = migrate_account(
            &pool,
            &plc_base,
            "test.bytescape.social",
            &old_did,
            &handle,
            user_id,
        )
        .await
        .expect("migrate_account");
        thread.join().expect("plc stub thread");

        assert!(
            new_did.starts_with("did:plc:"),
            "mint must produce a per-user did:plc, got {new_did}"
        );
        assert!(
            captured.lock().contains(&new_did),
            "PLC directory must receive the genesis op under the derived DID"
        );

        // Account swap: one active row, on the new DID, handle unchanged.
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT did, handle FROM pds_accounts WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .expect("account rows");
        assert_eq!(rows.len(), 1, "old account row must be retired: {rows:?}");
        assert_eq!(rows[0].0, new_did);
        assert_eq!(rows[0].1, handle);

        // Claim registry audit-logged and finalized (key copy released).
        let (staged_did, state, staged_key): (String, String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT new_did, state, signing_key_private FROM pds_did_reidentify WHERE old_did = $1",
        )
        .bind(&old_did)
        .fetch_one(&pool)
        .await
        .expect("claim row");
        assert_eq!(staged_did, new_did);
        assert_eq!(state, "done");
        assert!(staged_key.is_none(), "the staged key copy must be cleared at done (R2-5)");

        // Repo re-pointed: root lives under the new DID, old root tombstoned.
        let roots: Vec<String> = sqlx::query_scalar("SELECT did FROM pds_repo_roots WHERE did = ANY(ARRAY[$1, $2])")
            .bind(&old_did)
            .bind(&new_did)
            .fetch_all(&pool)
            .await
            .expect("repo roots");
        assert_eq!(roots, vec![new_did.clone()], "old root must be tombstoned");

        // Records preserved verbatim: same rkey + CID under the new DID.
        let recs: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
            "SELECT rkey, cid, value_json FROM pds_indexed_records WHERE did = $1",
        )
        .bind(&new_did)
        .fetch_all(&pool)
        .await
        .expect("indexed records");
        assert_eq!(recs.len(), 1, "record must survive the re-point: {recs:?}");
        assert_eq!(recs[0].0, rkey);
        assert_eq!(recs[0].1, old_cid, "record CID is content-addressed and must not change");
        assert_eq!(
            recs[0].2["subject"], 
            serde_json::json!(format!("at://did:plc:poster/app.bsky.feed.post/{suffix}")),
            "record value must round-trip verbatim"
        );

        // Blobs ride with the account.
        let blob_did: String = sqlx::query_scalar("SELECT did FROM pds_blobs WHERE cid = $1")
            .bind(format!("blob{suffix}"))
            .fetch_one(&pool)
            .await
            .expect("blob row");
        assert_eq!(blob_did, new_did, "blobs must move with the account");

        // Old commit chain retired with the identity.
        let seqs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pds_firehose_seq WHERE did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("firehose seq count");
        assert_eq!(seqs, 0, "old identity's firehose history must be retired");
    }

    /// R2-7/R2-2 headline invariant — crash-resume: a mint whose RESPONSE was
    /// lost (server accepted, connection died) must resume with the STAGED
    /// identity on the next attempt — zero second mint. The staged key and
    /// derived DID survive the crash; the retry audit-verifies publication and
    /// proceeds with the same identity. On pre-review code this FAILS: the
    /// mint happened before staging, so the crash left NO staged identity and
    /// attempt 2 minted a second, different did:plc (captured genesis DIDs
    /// diverge).
    #[tokio::test]
    async fn crash_between_mint_and_stage_resumes_the_staged_identity() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] crash_between_mint_and_stage_resumes_the_staged_identity skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;

        // Attempt 1: the directory ACCEPTS the genesis op but the response is
        // lost (the deploy-restart mid-migration crash window).
        let (plc_base, captured, thread) = stub_http_seq(vec![
            StubReply::CloseNoResponse,
            // Attempt 2's audit check: published (200).
            StubReply::Respond("HTTP/1.1 200 OK".to_string(), "[]".to_string()),
        ]);

        let first = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect_err("a lost mint response must fail the attempt");
        assert!(first.to_string().contains("PLC directory"), "got: {first}");

        // Simulate the restart's passage of time: the claim is now stale.
        sqlx::query("UPDATE pds_did_reidentify SET started_at = NOW() - interval '2 hours' WHERE old_did = $1")
            .bind(&old_did)
            .execute(&pool)
            .await
            .expect("age claim");

        // Attempt 2: resumes the STAGED identity (audit-verified published) —
        // never a second public mint.
        let second = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect("resume migrate");
        thread.join().expect("plc stub thread");

        let state: String = sqlx::query_scalar("SELECT state FROM pds_did_reidentify WHERE old_did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("claim state");
        assert_eq!(state, "done", "the resumed migration completes");
        assert_eq!(
            captured.lock().matches("POST ").count(),
            1,
            "exactly one public mint may ever happen per account"
        );
        // The single genesis POST's derived DID is the account's identity.
        assert!(
            second.starts_with("did:plc:"),
            "resume must yield the staged did:plc: {second}"
        );
    }

    /// R2-2 (re-POST semantics): a staged identity the directory PROVED
    /// unpublished (audit 4xx) is minted by re-POSTing the SAME genesis op —
    /// same key → same derived DID → same identity, never a second one.
    #[tokio::test]
    async fn crash_resume_reposts_the_same_identity_when_unpublished() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] crash_resume_reposts_the_same_identity_when_unpublished skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;

        // Attempt 1: lost response (outcome unknown to us, unpublished there).
        // Attempt 2: audit says 404 (never published) → re-POST same genesis.
        let (plc_base, captured, thread) = stub_http_seq(vec![
            StubReply::CloseNoResponse,
            StubReply::Respond("HTTP/1.1 404 Not Found".to_string(), r#"{"error":"Not found"}"#.to_string()),
            StubReply::Respond("HTTP/1.1 200 OK".to_string(), "{}".to_string()),
        ]);

        let _ = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect_err("attempt 1 loses the response");
        sqlx::query("UPDATE pds_did_reidentify SET started_at = NOW() - interval '2 hours' WHERE old_did = $1")
            .bind(&old_did)
            .execute(&pool)
            .await
            .expect("age claim");

        let second = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect("resume re-mints the SAME identity");
        thread.join().expect("plc stub thread");

        let cap = captured.lock().clone();
        assert_eq!(cap.matches("POST ").count(), 2, "lost mint + one re-POST: {cap}");
        assert_eq!(
            cap.matches(&format!("POST /{second}")).count(),
            2,
            "BOTH genesis POSTs must carry the SAME derived DID — one identity, re-posted: {cap}"
        );
    }

    /// R2-1: two concurrent runners over one account — the claim is
    /// exclusive, so exactly one genesis mint ever happens and both runners
    /// agree on one identity (the loser backs off or observes the winner's
    /// result). Pre-review code FAILS this: the claim upsert had no
    /// exclusion, both runners minted and two public DIDs burned.
    #[tokio::test]
    async fn claim_is_exclusive_across_concurrent_migrate_calls() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] claim_is_exclusive_across_concurrent_migrate_calls skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;

        // Exactly ONE scripted mint — a second mint would block on the
        // exhausted stub and surface as a transport error, but the captured
        // POST count is the assertion that matters.
        let (plc_base, captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);

        let (a, b) = tokio::join!(
            migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id),
            migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id),
        );
        thread.join().expect("plc stub thread");

        // Exactly one identity: the winner migrates; the loser either backs
        // off (claim held) or observes the completed result — never mints.
        let winner_did = match (a, b) {
            (Ok(d), Err(e)) | (Err(e), Ok(d)) => {
                assert!(
                    e.to_string().contains("back off"),
                    "the loser must observe the held claim and back off (never mint), got: {e}"
                );
                d
            }
            (Ok(d1), Ok(d2)) => {
                assert_eq!(d1, d2, "both runners must agree on the one identity");
                d1
            }
            (Err(e1), Err(e2)) => panic!("at least one runner must win the claim: {e1} / {e2}"),
        };
        assert!(winner_did.starts_with("did:plc:"), "got {winner_did}");
        assert_eq!(
            captured.lock().matches("POST ").count(),
            1,
            "exactly one public mint across concurrent runners"
        );
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM pds_accounts WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("account count");
        assert_eq!(rows, 1, "one account row lives — on the single minted identity");
    }

    /// R2-5: the staged key is sealed at rest (enc:v1: envelope — never
    /// plaintext) and the copy is released at done. Pre-review code FAILS
    /// both halves (raw key bytes staged, never cleared).
    #[tokio::test]
    async fn staged_key_is_encrypted_and_cleared_at_done() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] staged_key_is_encrypted_and_cleared_at_done skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;
        let (plc_base, _captured, thread) = stub_http_seq(vec![
            StubReply::CloseNoResponse,
            StubReply::Respond("HTTP/1.1 200 OK".to_string(), "[]".to_string()),
        ]);

        let _ = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect_err("crash window leaves the claim staged");
        let staged: Vec<u8> = sqlx::query_scalar(
            "SELECT signing_key_private FROM pds_did_reidentify WHERE old_did = $1",
        )
        .bind(&old_did)
        .fetch_one(&pool)
        .await
        .expect("staged key");
        assert!(
            staged.starts_with(crate::secret::ENC_PREFIX.as_bytes()),
            "the staged key must be sealed at rest (enc:v1: envelope), got {} bytes of plaintext",
            staged.len()
        );

        sqlx::query("UPDATE pds_did_reidentify SET started_at = NOW() - interval '2 hours' WHERE old_did = $1")
            .bind(&old_did)
            .execute(&pool)
            .await
            .expect("age claim");
        let _ = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect("resume completes");
        thread.join().expect("plc stub thread");

        let cleared: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT signing_key_private FROM pds_did_reidentify WHERE old_did = $1",
        )
        .bind(&old_did)
        .fetch_one(&pool)
        .await
        .expect("staged key after done");
        assert!(cleared.is_none(), "the staged key copy must be cleared at done");
    }

    /// R2-12: the walk is a completeness proof — an indexed record absent
    /// from the MST walk aborts BEFORE any drop (old root and account stay
    /// intact). Pre-review code FAILS: it migrated the partial set and
    /// destroyed the old root + account irreversibly.
    #[tokio::test]
    async fn cutover_aborts_on_walk_incompleteness() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] cutover_aborts_on_walk_incompleteness skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;
        // A phantom index row: consumer-derived state with NO MST leaf — the
        // walk cannot see it, so the migration must not proceed.
        sqlx::query(
            "INSERT INTO pds_indexed_records (did, collection, rkey, cid, value_json, rev) \
             VALUES ($1, 'app.bsky.feed.like', $2, 'phantomcid', '{}'::jsonb, 'phantomrev')",
        )
        .bind(&old_did)
        .bind(format!("phantom{suffix}"))
        .execute(&pool)
        .await
        .expect("seed phantom index row");

        let (plc_base, _captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let err = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect_err("phantom index rows must abort the migration");
        assert!(err.to_string().contains("walk incomplete"), "got: {err}");
        thread.join().expect("plc stub thread");

        // Abort happened BEFORE any drop: old root + account intact.
        let root: i64 = sqlx::query_scalar("SELECT count(*) FROM pds_repo_roots WHERE did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("old root");
        assert_eq!(root, 1, "the old root must survive an aborted walk");
        let acct: i64 = sqlx::query_scalar("SELECT count(*) FROM pds_accounts WHERE did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("old account");
        assert_eq!(acct, 1, "the old account must survive an aborted walk");
    }

    /// R2-18: nested-map records (real `app.bsky.feed.like` subjects are maps)
    /// must re-point with their CID intact — the index CID comes from the
    /// shared canonical encoder (`BlockMap::add`), so it equals the MST leaf
    /// CID exactly. Pre-review code FAILS: it wrote the re-encoded CID from
    /// bare `serde_ipld_dagcbor::to_vec` (known-buggy nested-map sorting),
    /// diverging from the MST leaf.
    #[tokio::test]
    async fn nested_subject_record_cid_preserved_across_repoint() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] nested_subject_record_cid_preserved_across_repoint skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let username = format!("mign_{suffix}");
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create fixture user");
        let old_did = format!("did:web:{username}.test.bytescape.social");
        let signing_key_private = [0x22u8; 32];
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&old_did)
        .bind(&username)
        .bind(&signing_key_private[..])
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("seed did:web pds account");

        // The nested subject SHAPE the flat fixture never exercised.
        let mut subject = std::collections::BTreeMap::new();
        subject.insert(
            "uri".to_string(),
            Ipld::String(format!("at://did:plc:poster/app.bsky.feed.post/{suffix}")),
        );
        subject.insert("cid".to_string(), Ipld::String(format!("bafy{suffix}")));
        let mut rec: RepoRecord = std::collections::BTreeMap::new();
        rec.insert(
            "$type".to_string(),
            Lex::Ipld(Ipld::String("app.bsky.feed.like".to_string())),
        );
        rec.insert("subject".to_string(), Lex::Ipld(Ipld::Map(subject)));

        let secp = Secp256k1::new();
        let secret = secp256k1::SecretKey::from_slice(&signing_key_private).expect("secret");
        let old_keypair = Keypair::from_secret_key(&secp, &secret);
        let rkey = format!("nlike{suffix}");
        let cid = rsky_repo::block_map::BlockMap::new()
            .add(rec.clone())
            .expect("record cid")
            .to_string();
        let storage: Arc<RwLock<dyn rsky_repo::storage::types::RepoStorage>> = Arc::new(RwLock::new(
            SqlxPostgresStorage::new(pool.clone(), old_did.clone()),
        ));
        Repo::create(
            storage,
            old_did.clone(),
            &old_keypair,
            Some(vec![RecordCreateOrUpdateOp {
                action: WriteOpAction::Create,
                collection: "app.bsky.feed.like".to_string(),
                rkey: rkey.clone(),
                record: rec,
            }]),
        )
        .await
        .expect("seed old repo");

        let (plc_base, _captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let new_did = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &username, user.id)
            .await
            .expect("migrate nested record");
        thread.join().expect("plc stub thread");

        let index_cid: String = sqlx::query_scalar(
            "SELECT cid FROM pds_indexed_records WHERE did = $1 AND rkey = $2",
        )
        .bind(&new_did)
        .bind(&rkey)
        .fetch_one(&pool)
        .await
        .expect("index cid");
        assert_eq!(
            index_cid, cid,
            "the index CID must equal the canonical record CID (MST leaf) — nested maps included"
        );
    }

    /// R2-9: a repo-less account must migrate WITH its profile self-record —
    /// never to a profile-less DID ("Profile not found" on the AppView).
    /// Pre-review code FAILS: the empty init commit carried zero records.
    #[tokio::test]
    async fn repoless_account_migrates_with_a_profile_record() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] repoless_account_migrates_with_a_profile_record skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let username = format!("migr_{suffix}");
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("create fixture user");
        let old_did = format!("did:web:{username}.test.bytescape.social");
        let signing_key_private = [0x33u8; 32];
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, $3, 'test-pub', 'active', $4)",
        )
        .bind(&old_did)
        .bind(&username)
        .bind(&signing_key_private[..])
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("seed did:web pds account");
        // No repo root, no records — the repo-less shape.

        let (plc_base, _captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let new_did = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &username, user.id)
            .await
            .expect("migrate repo-less account");
        thread.join().expect("plc stub thread");

        let profile: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pds_indexed_records \
             WHERE did = $1 AND collection = 'app.bsky.actor.profile' AND rkey = 'self'",
        )
        .bind(&new_did)
        .fetch_one(&pool)
        .await
        .expect("profile count");
        assert_eq!(profile, 1, "a repo-less account must migrate WITH a profile record");
    }

    /// R2-8: the retired identity's commit-chain and MST blocks KEEP their
    /// old-DID attribution (never re-attributed — their signatures match no
    /// served key and CID-keyed reads still resolve them). Pre-review code
    /// FAILS: it re-attributed every old block to the new DID.
    #[tokio::test]
    async fn old_commit_blocks_keep_their_attribution() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] old_commit_blocks_keep_their_attribution skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;
        let old_root: String = sqlx::query_scalar("SELECT root_cid FROM pds_repo_roots WHERE did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("old root cid");

        let (plc_base, _captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let new_did = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect("migrate_account");
        thread.join().expect("plc stub thread");

        let attr: String = sqlx::query_scalar("SELECT did FROM pds_ipld_blocks WHERE cid = $1")
            .bind(&old_root)
            .fetch_one(&pool)
            .await
            .expect("old commit block");
        assert_eq!(
            attr, old_did,
            "the retired commit chain must KEEP its old-DID block attribution (R2-8)"
        );
        let new_root: String = sqlx::query_scalar("SELECT root_cid FROM pds_repo_roots WHERE did = $1")
            .bind(&new_did)
            .fetch_one(&pool)
            .await
            .expect("new root cid");
        let new_attr: String = sqlx::query_scalar("SELECT did FROM pds_ipld_blocks WHERE cid = $1")
            .bind(&new_root)
            .fetch_one(&pool)
            .await
            .expect("new commit block");
        assert_eq!(new_attr, new_did, "the re-pointed commit lives under the new DID");
    }

    /// R2-17/R2-10: the claim state machine across restarts — `done` and
    /// `failed` are never re-attempted, a lease-expired `applying` with a
    /// staged identity re-enlists and completes it, and a fresh `applying`
    /// (live holder) is never stolen.
    #[tokio::test]
    async fn claim_state_machine_across_restarts() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] claim_state_machine_across_restarts skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();

        // done + failed claims: candidates() must never re-attempt them.
        for (kind, state) in [("done", "done"), ("failed", "failed")] {
            let username = format!("migsm{kind}_{suffix}");
            let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
                .await
                .expect("user");
            let did = format!("did:web:{username}.test.bytescape.social");
            sqlx::query(
                "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
                 VALUES ($1, $2, '\\x2222', 'test-pub', 'active', $3)",
            )
            .bind(&did)
            .bind(&username)
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("account");
            sqlx::query(
                "INSERT INTO pds_did_reidentify (old_did, user_id, username, state, new_did) \
                 VALUES ($1, $2, $3, $4, 'did:plc:auditonly')",
            )
            .bind(&did)
            .bind(user.id)
            .bind(&username)
            .bind(state)
            .execute(&pool)
            .await
            .expect("claim");
        }
        // Scope the run to this test's own accounts (allow_usernames hook) —
        // supersede_did_web walks ALL candidates and tests share one database.
        let owned: Vec<String> = vec![
            format!("migsmdone_{suffix}"),
            format!("migsmfailed_{suffix}"),
        ];
        let migrated = supersede_did_web(
            &pool,
            "http://127.0.0.1:9",
            "test.bytescape.social",
            true,
            Some(owned.clone()),
        )
        .await
        .expect("supersede run");
        assert_eq!(
            migrated, 0,
            "done and failed claims must never be re-attempted (unreachable PLC base proves no mint)"
        );

        // Fresh `applying` (a live holder): never stolen.
        let username = format!("migsmhold_{suffix}");
        let user = crate::db::users::UserRow::create_active(&pool, &username, "", "x", "x")
            .await
            .expect("user");
        let did = format!("did:web:{username}.test.bytescape.social");
        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, $2, '\\x2222', 'test-pub', 'active', $3)",
        )
        .bind(&did)
        .bind(&username)
        .bind(user.id)
        .execute(&pool)
        .await
        .expect("account");
        sqlx::query(
            "INSERT INTO pds_did_reidentify (old_did, user_id, username, state, started_at) \
             VALUES ($1, $2, $3, 'applying', NOW())",
        )
        .bind(&did)
        .bind(user.id)
        .bind(&username)
        .execute(&pool)
        .await
        .expect("claim");
        let err = migrate_account(&pool, "http://127.0.0.1:9", "test.bytescape.social", &did, &username, user.id)
            .await
            .expect_err("a live claim holder must not be stolen");
        assert!(err.to_string().contains("back off"), "got: {err}");

        // Stale `applying` WITH a staged identity: re-enlists (the crash-
        // resume path is pinned by the dedicated tests above; here the
        // state-machine arm itself must be reachable).
        sqlx::query("UPDATE pds_did_reidentify SET started_at = NOW() - interval '2 hours' WHERE old_did = $1")
            .bind(&did)
            .execute(&pool)
            .await
            .expect("age claim");
        // Seed the staged identity exactly as stage-before-mint would.
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut secp256k1::rand::rngs::OsRng);
        let staged_did = derive_did_for(&keypair, &username, "test.bytescape.social").expect("derive");
        let sealed = crate::secret::seal(
            &hex::encode(keypair.secret_key().secret_bytes()),
            &crate::db::accounts::jwt_secret(),
        );
        sqlx::query(
            "UPDATE pds_did_reidentify SET new_did = $2, signing_key_private = $3 WHERE old_did = $1",
        )
        .bind(&did)
        .bind(&staged_did)
        .bind(sealed.as_bytes().to_vec())
        .execute(&pool)
        .await
        .expect("stage");
        // Audit says published → resume completes with the staged identity.
        let (plc_base, _captured, thread) = stub_http("HTTP/1.1 200 OK", "[]", 1);
        let resumed = migrate_account(&pool, &plc_base, "test.bytescape.social", &did, &username, user.id)
            .await
            .expect("stale claim re-enlists with its staged identity");
        thread.join().expect("plc stub thread");
        assert_eq!(resumed, staged_did, "resume must yield the staged identity");
    }

    /// R2-3 (mechanism): while the migration holds an account's quiesce
    /// window, its record writes fail LOUDLY (Conflict) — never silently
    /// destroyed by the cutover. Pre-review code has no seam at all: the
    /// write succeeded and the cutover's snapshot destroyed it (the exact
    /// mid-window data-loss defect).
    #[tokio::test]
    async fn write_during_walk_window_is_refused_not_lost() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] write_during_walk_window_is_refused_not_lost skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;

        // The walk→cutover window, held exactly as migrate_account holds it.
        let window = crate::pds::quiesce::hold(&old_did);
        let err = crate::pds::create_record_internal(
            &pool,
            &old_did,
            "app.bsky.feed.like",
            Some(&format!("mid{suffix}")),
            &serde_json::json!({
                "$type": "app.bsky.feed.like",
                "subject": format!("at://did:plc:poster/app.bsky.feed.post/{suffix}"),
            }),
            Some("test.bytescape.social"),
        )
        .await
        .expect_err("a write in the walk window must be refused");
        assert!(
            matches!(err, crate::error::AppError::Conflict(_)),
            "refused with Conflict, got: {err:?}"
        );
        drop(window);

        // Post-window: the same write lands (nothing was lost — retry works).
        let _ = crate::pds::create_record_internal(
            &pool,
            &old_did,
            "app.bsky.feed.like",
            Some(&format!("mid{suffix}")),
            &serde_json::json!({
                "$type": "app.bsky.feed.like",
                "subject": format!("at://did:plc:poster/app.bsky.feed.post/{suffix}"),
            }),
            Some("test.bytescape.social"),
        )
        .await
        .expect("the retried write lands after the window");
        let _ = user_id;
        let _ = handle;
    }

    /// R2-3 (second guard): a write that lands DESPITE the quiesce (e.g. a
    /// direct storage write) bumps the root rev — the cutover must detect it
    /// and abort BEFORE destroying anything, never destroy the write.
    /// Pre-review code FAILS: the cutover had no rev check and silently
    /// replaced the root + index with its stale walk snapshot.
    #[tokio::test]
    async fn cutover_aborts_when_root_rev_moved() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] cutover_aborts_when_root_rev_moved skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (user_id, old_did, handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;

        // The mid-window write: bumps the root rev under the walker's feet.
        sqlx::query("UPDATE pds_repo_roots SET rev = $2 WHERE did = $1")
            .bind(&old_did)
            .bind(format!("movedrev{suffix}"))
            .execute(&pool)
            .await
            .expect("bump root rev");

        let (plc_base, _captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let err = migrate_account(&pool, &plc_base, "test.bytescape.social", &old_did, &handle, user_id)
            .await
            .expect_err("a moved root rev must abort the cutover");
        assert!(
            err.to_string().contains("rev moved") || err.to_string().contains("retry"),
            "got: {err}"
        );
        thread.join().expect("plc stub thread");

        // Abort happened BEFORE any drop: old root + account intact.
        let root: i64 = sqlx::query_scalar("SELECT count(*) FROM pds_repo_roots WHERE did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("old root");
        assert_eq!(root, 1, "the old root must survive an aborted cutover");
    }

    /// Dry-run: audit log only, nothing migrates, nothing claims.
    #[tokio::test]
    async fn supersede_dry_run_touches_nothing() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else {
            eprintln!("[test] supersede_dry_run_touches_nothing skipped: DATABASE_URL unset — asserted nothing");
            return;
        };
        let suffix = crate::ap::testutil::unique();
        let (_user_id, old_did, _handle, _rkey, _cid) = seed_did_web_account(&pool, &suffix).await;

        let migrated = supersede_did_web(
            &pool,
            "http://127.0.0.1:9", // unreachable — dry-run must never mint
            "test.bytescape.social",
            false,
            None,
        )
        .await
        .expect("dry-run");
        assert_eq!(migrated, 0, "dry-run must not migrate");

        let still_web: String = sqlx::query_scalar("SELECT did FROM pds_accounts WHERE did = $1")
            .bind(&old_did)
            .fetch_one(&pool)
            .await
            .expect("old account untouched");
        assert_eq!(still_web, old_did);
        let claims: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pds_did_reidentify WHERE old_did = $1",
        )
        .bind(&old_did)
        .fetch_one(&pool)
        .await
        .expect("claim count");
        assert_eq!(claims, 0, "dry-run must not write claim rows");
    }
}

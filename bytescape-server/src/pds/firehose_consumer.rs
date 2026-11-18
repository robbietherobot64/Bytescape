//! Firehose consumer — background task that reads our own firehose events
//! and writes derived index data to pds_indexed_records.

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Notify;

/// Spawn the firehose consumer as a background task.
/// It reads from pds_firehose_seq, processes events, and writes to pds_indexed_records.
pub fn spawn(pool: PgPool, shutdown: Arc<Notify>) {
    tokio::spawn(async move {
        tracing::info!("Firehose consumer started");

        // Get last consumed offset
        let last_seq: i64 = sqlx::query_scalar(
            "SELECT last_seq FROM pds_consumer_offset WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        tracing::info!("Firehose consumer starting from seq {}", last_seq);

        let mut current_seq = last_seq;

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    tracing::info!("Firehose consumer shutting down");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Poll for new events
                    match fetch_events(&pool, current_seq).await {
                        Ok(events) => {
                            if events.is_empty() {
                                continue;
                            }
                            for event in events {
                                match process_event(&pool, &event).await {
                                    Ok(_) => {
                                        current_seq = event.seq;
                                        update_offset(&pool, current_seq).await.ok();
                                    }
                                    Err(e) => {
                                        tracing::error!("Firehose consumer error processing seq {}: {}", event.seq, e);
                                        // Don't advance offset on error — retry next poll
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Firehose consumer fetch error: {}", e);
                        }
                    }
                }
            }
        }
    });
}

#[derive(Debug, Clone)]
pub struct FirehoseRow {
    pub seq: i64,
    pub did: String,
    pub commit_cid: String,
    pub prev_cid: Option<String>,
    pub rev: String,
    pub ops: serde_json::Value,
    pub blocks: Vec<u8>,
}

async fn fetch_events(pool: &PgPool, after_seq: i64) -> Result<Vec<FirehoseRow>> {
    let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, String, serde_json::Value, Vec<u8>)>(
        "SELECT seq, did, commit_cid, prev_cid, rev, ops, blocks
         FROM pds_firehose_seq
         WHERE seq > $1
         ORDER BY seq ASC
         LIMIT 100",
    )
    .bind(after_seq)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(seq, did, commit_cid, prev_cid, rev, ops, blocks)| FirehoseRow {
            seq,
            did,
            commit_cid,
            prev_cid,
            rev,
            ops,
            blocks,
        })
        .collect())
}

async fn process_event(pool: &PgPool, event: &FirehoseRow) -> Result<()> {
    let empty_vec = vec![];
    let ops = event.ops.as_array().unwrap_or(&empty_vec);

    for op in ops {
        let path = op.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let action = op.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let cid = op.get("cid").and_then(|v| v.as_str());

        // Parse path into collection/rkey
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() != 2 {
            continue;
        }
        let collection = parts[0];
        let rkey = parts[1];

        match action {
            "create" | "update" => {
                if let Some(cid_str) = cid {
                    // Decode the record from the CAR blocks
                    let value_json = decode_record_from_car(&event.blocks, cid_str);

                    sqlx::query(
                        "INSERT INTO pds_indexed_records (did, collection, rkey, cid, value_json, rev)
                         VALUES ($1, $2, $3, $4, $5, $6)
                         ON CONFLICT (did, collection, rkey)
                         DO UPDATE SET cid = $4, value_json = $5, rev = $6, indexed_at = NOW()",
                    )
                    .bind(&event.did)
                    .bind(collection)
                    .bind(rkey)
                    .bind(cid_str)
                    .bind(sqlx::types::Json(&value_json))
                    .bind(&event.rev)
                    .execute(pool)
                    .await?;
                }
            }
            "delete" => {
                sqlx::query(
                    "DELETE FROM pds_indexed_records
                     WHERE did = $1 AND collection = $2 AND rkey = $3",
                )
                .bind(&event.did)
                .bind(collection)
                .bind(rkey)
                .execute(pool)
                .await?;
            }
            _ => {}
        }

        // Calendar records → derived index. Local did:web repos never reach
        // the relay (design decision 2c), so the local consumer carries the
        // same arms as appview/consumer.rs — spec events/ingestion.
        if crate::calendar::is_event_path(path) || crate::calendar::is_rsvp_path(path) {
            let uri = format!("at://{}/{}", event.did, path);
            match action {
                "create" | "update" => {
                    if let Some(cid_str) = cid {
                        let value_json = decode_record_from_car(&event.blocks, cid_str);
                        if crate::calendar::is_rsvp_path(path) {
                            match crate::calendar::parse_rsvp(&value_json) {
                                Some(r) => {
                                    crate::calendar::upsert_rsvp(pool, &uri, &event.did, r).await?;
                                }
                                None => {
                                    tracing::warn!("[PDS] Skipping malformed calendar rsvp {}", uri);
                                }
                            }
                        } else {
                            match crate::calendar::parse_event(&value_json, collection) {
                                Some(ev) => {
                                    crate::calendar::upsert_event(pool, &uri, cid_str, &event.did, ev, "atproto").await?;
                                }
                                None => {
                                    tracing::warn!("[PDS] Skipping malformed calendar event {}", uri);
                                }
                            }
                        }
                    }
                }
                "delete" => {
                    if crate::calendar::is_rsvp_path(path) {
                        crate::calendar::delete_rsvp(pool, &uri).await?;
                    } else {
                        crate::calendar::delete_event(pool, &uri).await?;
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

async fn update_offset(pool: &PgPool, seq: i64) -> Result<()> {
    sqlx::query(
        "UPDATE pds_consumer_offset SET last_seq = $1, updated_at = NOW() WHERE id = 1",
    )
    .bind(seq)
    .execute(pool)
    .await?;
    Ok(())
}

/// Decode a record from CAR-encoded blocks by CID.
/// The CAR format is: [header_chunk] [block_chunk...] where each chunk is uvarint-length-prefixed.
/// Each block chunk is: [cid_bytes] [block_data].
fn decode_record_from_car(blocks: &[u8], target_cid: &str) -> serde_json::Value {
    use std::io::{Cursor, Read};

    let mut reader = Cursor::new(blocks);

    // Read CAR header chunk (skip it)
    if let Ok(header_len) = read_uvarint(&mut reader) {
        let mut header = vec![0u8; header_len as usize];
        let _ = reader.read_exact(&mut header);
    }

    // Read block chunks
    while let Ok(chunk_len) = read_uvarint(&mut reader) {
        let mut chunk = vec![0u8; chunk_len as usize];
        if reader.read_exact(&mut chunk).is_err() {
            break;
        }

        // Parse CID from chunk
        let mut chunk_reader = Cursor::new(&chunk);
        let cid_result: Result<lexicon_cid::Cid, _> = lexicon_cid::Cid::read_bytes(&mut chunk_reader);
        match cid_result {
            Ok(cid) => {
                if cid.to_string() == target_cid {
                    // Found the target block — decode as DAG-CBOR
                    let remaining = &chunk[chunk_reader.position() as usize..];
                    if let Ok(value) = serde_ipld_dagcbor::from_reader::<serde_json::Value, _>(&mut Cursor::new(remaining)) {
                        return value;
                    }
                }
            }
            Err(_) => continue,
        }
    }

    // Fallback: if we can't decode, store the CID reference
    serde_json::json!({"cid": target_cid})
}

fn read_uvarint(reader: &mut std::io::Cursor<&[u8]>) -> Result<u64, std::io::Error> {
    use std::io::Read;

    let mut out: u64 = 0;
    let mut buf = [0u8; 1];
    let mut i = 0;

    while reader.read(&mut buf).map(|n| n > 0).unwrap_or(false) {
        let b = buf[0] as u64;
        let k = b & 0x7F;
        out |= k << (i * 7);

        if b & 0x80 == 0 {
            return Ok(out);
        }
        i += 1;
        if i > 9 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "uvarint too long"));
        }
    }

    Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "uvarint eof"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::Row;
    use std::io::Cursor;

    fn uvarint(mut n: u64) -> Vec<u8> {
        let mut out = vec![];
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            out.push(if n == 0 { b } else { b | 0x80 });
            if n == 0 {
                break;
            }
        }
        out
    }

    /// Structurally valid CIDv1 (dag-cbor + sha2-256) whose string form the
    /// decoder matches against, plus its raw bytes for the CAR entry.
    fn cid_for(seed: u8) -> (Vec<u8>, String) {
        let mut bytes = vec![0x01, 0x71, 0x12, 0x20];
        bytes.extend([seed; 32]);
        let cid = lexicon_cid::Cid::read_bytes(&mut Cursor::new(&bytes)).expect("valid cid");
        (bytes, cid.to_string())
    }

    /// Synthetic local CAR: header chunk + one [cid][dag-cbor block] entry.
    fn car_with(cid_bytes: &[u8], record: &serde_json::Value) -> Vec<u8> {
        let header = serde_ipld_dagcbor::to_vec(&json!({"roots": [], "version": 1})).unwrap();
        let block = serde_ipld_dagcbor::to_vec(record).unwrap();
        let mut out = uvarint(header.len() as u64);
        out.extend(header);
        let mut entry = cid_bytes.to_vec();
        entry.extend(block);
        out.extend(uvarint(entry.len() as u64));
        out.extend(entry);
        out
    }

    /// Local PDS frames upsert flat calendar rows and deletes remove them
    /// (spec: native did:web events get the same index treatment as the
    /// relay path; replay never duplicates). Skips without DATABASE_URL.
    #[tokio::test]
    async fn local_frames_index_calendar_records() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL unset — skipping DB integration test");
            return;
        };
        let pool = PgPool::connect(&url).await.expect("db connect");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let did = format!("did:web:ctestcal{}.bytescape.social", stamp);
        let path = "community.lexicon.calendar.event/localtest";
        let uri = format!("at://{}/{}", did, path);
        let (cid_bytes, cid_str) = cid_for(21);

        let record = json!({
            "$type": "community.lexicon.calendar.event",
            "name": "Local frame event",
            "createdAt": "2026-11-01T12:00:00Z",
            "startsAt": "2026-11-21T09:00:00Z",
            "mode": "community.lexicon.calendar.event#virtual"
        });
        let blocks = car_with(&cid_bytes, &record);
        let make_row = |action: &str, blocks: Vec<u8>| FirehoseRow {
            seq: 0,
            did: did.clone(),
            commit_cid: cid_str.clone(),
            prev_cid: None,
            rev: "testrev".into(),
            ops: json!([{"action": action, "path": path, "cid": cid_str}]),
            blocks,
        };

        // create → row lands with flat columns
        process_event(&pool, &make_row("create", blocks.clone())).await.expect("create");
        let row = sqlx::query("SELECT name, starts_at, mode FROM calendar_events WHERE uri = $1")
            .bind(&uri)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .expect("local event row indexed");
        let (name, starts_at, mode): (String, Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
            (row.get(0), row.get(1), row.get(2));
        assert_eq!(name, "Local frame event");
        assert_eq!(starts_at.expect("starts_at").to_rfc3339(), "2026-11-21T09:00:00+00:00");
        assert_eq!(mode.as_deref(), Some("community.lexicon.calendar.event#virtual"));

        // replay → idempotent (one row)
        process_event(&pool, &make_row("update", blocks)).await.expect("replay");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_events WHERE uri = $1")
            .bind(&uri).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 1, "local replay must never duplicate");

        // delete → row gone
        process_event(&pool, &make_row("delete", vec![])).await.expect("delete");
        let gone = sqlx::query("SELECT 1 FROM calendar_events WHERE uri = $1")
            .bind(&uri).fetch_optional(&pool).await.unwrap();
        assert!(gone.is_none(), "deleted local event removed");

        // Cleanup (generic pds_indexed_records write also ran — by design).
        let _ = sqlx::query("DELETE FROM pds_indexed_records WHERE did = $1").bind(&did).execute(&pool).await;
    }
}
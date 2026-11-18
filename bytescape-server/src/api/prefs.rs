//! Network-native preference sync — Bytescape's sources project onto the AT
//! Protocol preference record so picks travel with the DID and are visible to
//! Bluesky and other clients:
//!
//! - feed subscriptions → `app.bsky.actor.defs#savedFeedsPrefV2` items
//!   (type "feed") — the same saved/pinned-feeds surface every client renders;
//! - picked interests → `app.bsky.actor.defs#interestsPref` tags.
//!
//! Writes are read-modify-write through `app.bsky.actor.getPreferences` /
//! `putPreferences`, serialized on a process-wide lock (adequate at our scale;
//! no deadlock possible). We only manage feed items pointing at the Bytescape
//! generator DID and tags that are picker ids — everything else in the record
//! is preserved untouched.

use serde_json::{json, Value};
use std::sync::LazyLock;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::api::feedgen;
use crate::db::accounts::LinkedAccountRow;
use crate::db::subscriptions::SubscriptionRow;
use crate::db::users::UserRow;
use crate::oauth::bluesky::BlueskyApiClient;

static PREFS_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const MANAGED_FEED_PREFIX: &str = "at://did:web:feeds.bytescape.social/";

const T_SAVED_V2: &str = "app.bsky.actor.defs#savedFeedsPrefV2";
const T_SAVED_V1: &str = "app.bsky.actor.defs#savedFeedsPref";
const T_INTERESTS: &str = "app.bsky.actor.defs#interestsPref";

fn managed_feed(value: &str) -> bool {
    value.starts_with(MANAGED_FEED_PREFIX)
}

/// Fire-and-forget push for write paths (bootstrap / subscription mutations).
pub fn push_async(db: sqlx::PgPool, user_id: Uuid) {
    tokio::spawn(async move {
        if let Err(e) = push(&db, user_id).await {
            tracing::warn!("prefs push failed for {}: {}", user_id, e);
        }
    });
}

/// Project local sources onto the network preference record.
pub async fn push(db: &sqlx::PgPool, user_id: Uuid) -> Result<(), String> {
    let _guard = PREFS_LOCK.lock().await;
    let user = UserRow::find_by_id(db, user_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("user not found")?;
    let Some(client) = client_for(db, user_id).await? else {
        return Ok(()); // no linked Bluesky account — app-DB stays authoritative
    };

    let feed_uris = active_feed_uris(db, user_id).await?;

    let prefs = client.get_preferences().await?;
    let items = prefs["preferences"].as_array().cloned().unwrap_or_default();

    let ours: Vec<Value> = feed_uris
        .iter()
        .map(|(uri, pinned)| json!({ "id": uri, "type": "feed", "value": uri, "pinned": pinned }))
        .collect();

    let mut out: Vec<Value> = Vec::new();
    let mut saw_v2 = false;
    let mut saw_interests = false;
    for mut item in items {
        match item["$type"].as_str().unwrap_or("") {
            T_SAVED_V2 => {
                saw_v2 = true;
                // Local feed subs are authoritative: strip ALL remote feed
                // entries and write ours as the complete set. Filtering only
                // managed/known feeds left Bluesky-deleted-but-locally-removed
                // feeds behind and re-appended locals → 10x duplicates.
                let kept: Vec<Value> = item["items"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter(|it| it["type"].as_str() != Some("feed"))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let mut all = kept;
                all.extend(ours.clone());
                item["items"] = Value::Array(all);
            }
            T_SAVED_V1 => {
                // Legacy shape: patch in place only when present. Non-feed
                // entries (e.g. other prefs) survive; feed lists are replaced.
                for key in ["saved", "pinned"] {
                    let kept: Vec<Value> = item[key]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter(|v| {
                                    let s = v.as_str().unwrap_or("");
                                    !s.contains("app.bsky.feed.generator")
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    let mut all = kept;
                    if key == "saved" {
                        all.extend(feed_uris.iter().map(|(u, _)| Value::String(u.clone())));
                    } else {
                        all.extend(
                            feed_uris
                                .iter()
                                .filter(|(_, p)| *p)
                                .map(|(u, _)| Value::String(u.clone())),
                        );
                    }
                    item[key] = Value::Array(all);
                }
            }
            T_INTERESTS => {
                saw_interests = true;
                let kept: Vec<Value> = item["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter(|t| {
                                feedgen::topic_for_interest(t.as_str().unwrap_or("")).is_none()
                            })
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let mut all = kept;
                all.extend(user.interests.iter().map(|t| Value::String(t.clone())));
                item["tags"] = Value::Array(all);
            }
            _ => {}
        }
        out.push(item);
    }
    if !saw_v2 {
        out.push(json!({ "$type": T_SAVED_V2, "items": ours }));
    }
    if !saw_interests {
        out.push(json!({ "$type": T_INTERESTS, "tags": user.interests }));
    }

    client.put_preferences(&Value::Array(out)).await?;
    Ok(())
}

/// One-time import for accounts with nothing local: bring the network's saved
/// feeds and interest tags in as sources. Returns true when an import ran.
pub async fn pull_if_empty(db: &sqlx::PgPool, user_id: Uuid) -> Result<bool, String> {
    let _guard = PREFS_LOCK.lock().await;
    let user = UserRow::find_by_id(db, user_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("user not found")?;
    let rows = SubscriptionRow::find_by_user(db, user_id)
        .await
        .map_err(|e| e.to_string())?;
    if !rows.is_empty() || !user.interests.is_empty() {
        return Ok(false);
    }
    let Some(client) = client_for(db, user_id).await? else {
        return Ok(false);
    };
    let prefs = client.get_preferences().await.unwrap_or(json!({}));
    let items = prefs["preferences"].as_array().cloned().unwrap_or_default();

    let mut feed_uris: Vec<(String, bool)> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    for item in &items {
        match item["$type"].as_str().unwrap_or("") {
            T_SAVED_V2 => {
                if let Some(arr) = item["items"].as_array() {
                    for it in arr {
                        if it["type"].as_str() == Some("feed") {
                            if let Some(v) = it["value"].as_str() {
                                feed_uris
                                    .push((v.to_string(), it["pinned"].as_bool().unwrap_or(false)));
                            }
                        }
                    }
                }
            }
            T_SAVED_V1 => {
                let pinned_set: std::collections::HashSet<&str> = item["pinned"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                let arr = item["saved"].as_array().or_else(|| item["pinned"].as_array());
                if let Some(arr) = arr {
                    for v in arr.iter().filter_map(|v| v.as_str()) {
                        if v.contains("app.bsky.feed.generator") {
                            feed_uris.push((v.to_string(), pinned_set.contains(v)));
                        }
                    }
                }
            }
            T_INTERESTS => {
                if let Some(arr) = item["tags"].as_array() {
                    for v in arr.iter().filter_map(|v| v.as_str()) {
                        tags.push(v.trim_start_matches('#').to_lowercase());
                    }
                }
            }
            _ => {}
        }
    }

    // Tags that are picker ids become interests (expanded through the topic
    // catalog); everything else becomes a plain hashtag source.
    let mut interests: Vec<String> = Vec::new();
    for tag in tags {
        if feedgen::topic_for_interest(&tag).is_some() {
            if !interests.contains(&tag) {
                interests.push(tag);
            }
        } else {
            let _ = SubscriptionRow::create(
                db,
                user_id,
                "hashtag",
                &tag,
                &format!("#{}", tag),
                "#",
                "hashtags",
            )
            .await;
        }
    }
    for (uri, pinned) in feed_uris {
        let label = match client.feed_display_name(&uri).await {
            Some(name) => name,
            None => readable_feed_label(&uri),
        };
        if let Ok(row) = SubscriptionRow::create(
            db,
            user_id,
            "bluesky_feed",
            &uri,
            &format!("Feed: {}", label),
            "📡",
            "feeds",
        )
        .await
        {
            if pinned {
                let _ = SubscriptionRow::set_pinned(db, row.id, user_id, true).await;
            }
        }
    }
    if !interests.is_empty() {
        UserRow::set_interests(db, user_id, &interests)
            .await
            .map_err(|e| e.to_string())?;
        crate::api::subscriptions::expand_interests(db, user_id, &interests)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// Humanized fallback label for a saved-feed URI whose display name couldn't be
/// resolved. Takes the rkey; if it looks like an auto-generated AT Protocol key
/// (long lowercase base32-ish run) we can't make it readable, so we surface it
/// as a neutral "Saved feed" rather than a meaningless hash. Otherwise the rkey
/// is a meaningful slug and we title-case it.
fn readable_feed_label(uri: &str) -> String {
    let rkey = uri.rsplit('/').next().unwrap_or("feed");
    let looks_generated = rkey.len() >= 12
        && rkey.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if looks_generated {
        "Saved feed".to_string()
    } else {
        rkey.replace('-', " ")
            .replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Extract (feed_uri, pinned) pairs from a Bluesky preferences item array.
fn parse_saved_feed_uris(items: &[Value]) -> Vec<(String, bool)> {
    let mut feed_uris: Vec<(String, bool)> = Vec::new();
    for item in items {
        match item["$type"].as_str().unwrap_or("") {
            T_SAVED_V2 => {
                if let Some(arr) = item["items"].as_array() {
                    for it in arr {
                        if it["type"].as_str() == Some("feed") {
                            if let Some(v) = it["value"].as_str() {
                                feed_uris.push((v.to_string(), it["pinned"].as_bool().unwrap_or(false)));
                            }
                        }
                    }
                }
            }
            T_SAVED_V1 => {
                let pinned_set: std::collections::HashSet<&str> = item["pinned"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                let arr = item["saved"].as_array().or_else(|| item["pinned"].as_array());
                if let Some(arr) = arr {
                    for v in arr.iter().filter_map(|v| v.as_str()) {
                        if v.contains("app.bsky.feed.generator") {
                            feed_uris.push((v.to_string(), pinned_set.contains(v)));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    feed_uris
}

/// Two-way reconciliation of Bluesky-saved feeds with local `bluesky_feed`
/// subscriptions (runs on feed load, not just when empty):
///  - a feed saved on Bluesky but missing locally is added;
///  - a local `bluesky_feed` sub no longer on Bluesky is removed (the user
///    deleted it in the Bluesky app — "deleting on Bluesky deletes here").
/// Bytescape-managed sources (interests → bootstrap_feed/hashtag) are never
/// touched. After reconciling, push() writes the result back so the two sides
/// converge. Returns the number of local rows changed.
pub async fn reconcile_feeds(db: &sqlx::PgPool, user_id: Uuid) -> Result<usize, String> {
    let _guard = PREFS_LOCK.lock().await;
    let Some(client) = client_for(db, user_id).await? else {
        return Ok(0); // no linked Bluesky account — nothing to reconcile
    };
    let prefs = client.get_preferences().await.unwrap_or(json!({}));
    let items = prefs["preferences"].as_array().cloned().unwrap_or_default();
    let remote: Vec<(String, bool)> = parse_saved_feed_uris(&items);
    let remote_set: std::collections::HashSet<&str> = remote.iter().map(|(u, _)| u.as_str()).collect();

    let rows = SubscriptionRow::find_by_user(db, user_id)
        .await
        .map_err(|e| e.to_string())?;

    // Remove local Bluesky-feed subs the user deleted on Bluesky.
    let mut changed = 0usize;
    for row in rows.iter().filter(|r| r.source == "bluesky_feed") {
        if !remote_set.contains(row.source_id.as_str()) {
            let _ = SubscriptionRow::delete(db, row.id, user_id).await;
            changed += 1;
        }
    }

    // Add remote saved feeds not present locally.
    let local: std::collections::HashSet<String> = rows
        .iter()
        .filter(|r| r.source == "bluesky_feed")
        .map(|r| r.source_id.clone())
        .collect();
    for (uri, pinned) in &remote {
        if managed_feed(uri) || local.contains(uri) {
            continue;
        }
        // Resolve the feed's real display name; fall back to a readable label
        // rather than the raw rkey (auto-generated keys look like
        // "aaab6r6rkviuc" and are meaningless to the user).
        let label = match client.feed_display_name(uri).await {
            Some(name) => name,
            None => readable_feed_label(uri),
        };
        if let Ok(row) = SubscriptionRow::create(
            db,
            user_id,
            "bluesky_feed",
            uri,
            &format!("Feed: {}", label),
            "📡",
            "feeds",
        )
        .await
        {
            if *pinned {
                let _ = SubscriptionRow::set_pinned(db, row.id, user_id, true).await;
            }
            changed += 1;
        }
    }

    Ok(changed)
}

async fn active_feed_uris(db: &sqlx::PgPool, user_id: Uuid) -> Result<Vec<(String, bool)>, String> {
    let rows = SubscriptionRow::find_by_user(db, user_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .filter(|r| r.enabled && r.category == "feeds" && r.source_id.starts_with("at://"))
        .map(|r| (r.source_id.clone(), r.pinned))
        .collect())
}

// getPreferences/putPreferences are PDS-side endpoints (verified live: the
// AppView answers MethodNotImplemented; the PDS answers the scope check). The
// token must carry the aud-qualified rpc: scopes from BSKY_OAUTH_SCOPES —
// tokens minted before those scopes existed need one account re-link.
async fn client_for(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Option<BlueskyApiClient>, String> {
    let accounts = LinkedAccountRow::find_by_user(db, user_id)
        .await
        .map_err(|e| e.to_string())?;
    let Some(acct) = accounts.iter().find(|a| a.provider == "bluesky") else {
        return Ok(None);
    };
    let token = crate::oauth::bluesky::fresh_token(db, acct).await?;
    let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
    let seed = hex::decode(acct.dpop_private_key.as_deref().unwrap_or("")).unwrap_or_default();
    let client = if seed.len() == 32 {
        BlueskyApiClient::new(&token, &seed, &pds)?
    } else {
        BlueskyApiClient::new_bearer(&token, &pds)
    };
    Ok(Some(client))
}

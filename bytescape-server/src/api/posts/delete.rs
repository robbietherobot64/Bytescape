use axum::{extract::{Path, State}, http::HeaderMap, Json};
use uuid::Uuid;

use crate::db;
use crate::error::AppError;
use crate::oauth;
use crate::AppState;

use super::extract_user_id;

/// Resolve the PDS URL from a DID by querying the PLC directory or did:web document.
/// Falls back to `fallback_pds` if resolution fails.
async fn resolve_pds_from_did(http: &reqwest::Client, did: &str, fallback_pds: &str) -> String {
    match crate::oauth::bluesky::resolve_pds_endpoint(http, did).await {
        Ok(endpoint) => {
            tracing::info!("[bluesky_delete] Resolved PDS for {}: {}", did, endpoint);
            endpoint
        }
        Err(e) => {
            tracing::warn!("[bluesky_delete] Failed to resolve PDS for {}: {}, falling back to {}", did, e, fallback_pds);
            fallback_pds.to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/posts/{id} — delete own post
// ---------------------------------------------------------------------------

pub(super) async fn delete_post_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let http = reqwest::Client::new();

    // ── 1. Local Bytescape post (UUID) ───────────────────────────────────
    if let Ok(uuid) = Uuid::parse_str(&post_id) {
        let row = sqlx::query_as::<_, db::posts::PostRow>("SELECT * FROM posts WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?;

        if let Some(post) = row {
            if post.user_id != user_id {
                return Err(AppError::Auth("Cannot delete another user's post".into()));
            }

            // ── Load posting identities (A4-T13) ──
            let posting_identities = db::posting_identities::PostingIdentityRow::find_by_user(&state.db, user_id)
                .await
                .unwrap_or_default();
            let _at_identity = posting_identities.iter().find(|pi| pi.protocol == "atproto");

            let cross_posts = db::cross_posts::CrossPostRow::find_by_post(&state.db, uuid).await.unwrap_or_default();
            let accounts = db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id).await.unwrap_or_default();
            let mut errors: Vec<String> = Vec::new();

            // Federate delete to each external platform
            // A4-T13: Match cross_posts to the correct linked account by platform + external_handle
            for cp in &cross_posts {
                match cp.platform.as_str() {
                    "bluesky" => {
                        // Match by provider + external_handle (e.g., @username)
                        let acct = accounts.iter().find(|a| {
                            if a.provider != "bluesky" { return false; }
                            let expected_handle = format!("@{}", a.external_username);
                            cp.external_handle.as_deref() == Some(&expected_handle)
                        }).or_else(|| accounts.iter().find(|a| a.provider == "bluesky")); // fallback to first bluesky account
                        if let Some(acct) = acct {
                            let did = acct.metadata["did"].as_str().unwrap_or(&acct.external_id);
                            let rkey = cp.external_id.as_deref()
                                .or_else(|| cp.external_url.rsplit('/').next())
                                .unwrap_or("")
                                .trim_start_matches("at://")
                                .split('/')
                                .last()
                                .unwrap_or("");

                            // Resolve PDS from DID
                            let pds_url = match oauth::bluesky::resolve_pds_from_did(&http, did).await {
                                Ok(pds) => pds,
                                Err(e) => {
                                    tracing::warn!("[delete] Cross-post PDS resolution failed: {}", e);
                                    crate::oauth::bluesky::get_pds_url(&acct.metadata)
                                }
                            };

                            if !rkey.is_empty() {
                                if let Err(e) = oauth::bluesky::delete_record(&state.db, acct, "app.bsky.feed.post", rkey).await {
                                    tracing::error!("Failed to delete Bluesky post {}: {}", cp.external_url, e);
                                    errors.push(format!("Bluesky: {}", e));
                                }
                            }
                        }
                    }
                    "mastodon" => {
                        // Cascade-delete the federated copy on the home instance
                        // (M-linked users' statuses live on their instance).
                        if let Some(acct) = accounts.iter().find(|a| a.provider == "mastodon") {
                            match super::mastodon_home_instance(acct) {
                                Ok(instance) => {
                                    let status_id = cp.external_id.as_deref()
                                        .or_else(|| cp.external_url.rsplit('/').next())
                                        .unwrap_or("");
                                    if !status_id.is_empty() {
                                        if let Err(e) = oauth::mastodon::delete_status(
                                            &http, &instance, &acct.access_token, status_id,
                                        ).await {
                                            tracing::error!("[delete] Mastodon delete failed for {}: {}", cp.external_url, e);
                                            errors.push(format!("Mastodon: {}", e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("[delete] Mastodon home instance unresolved: {}", e);
                                    errors.push(format!("Mastodon: {}", e));
                                }
                            }
                        }
                    }
                    // Lemmy cross-post delete removed — Sprint 1 (Lemmy deletion)
                    _ => {}
                }
            }

            if !errors.is_empty() {
                return Ok(Json(serde_json::json!({
                    "deleted": false,
                    "errors": errors,
                })));
            }

            // AP Delete delivery (Phase C): delete the native PDS record so
            // the bridge emits an Undo of the stored AP original. Log and
            // continue — the local delete proceeds regardless (the record may
            // never have been written, e.g. non-public posts).
            if let Some((did,)) = sqlx::query_as::<_, (String,)>(
                "SELECT did FROM pds_accounts WHERE user_id = $1",
            )
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            {
                if let Err(e) = crate::pds::delete_record_internal(
                    &state.db,
                    &did,
                    "app.bsky.feed.post",
                    &uuid.to_string(),
                )
                .await
                {
                    tracing::warn!("[delete] PDS record delete failed for {did}/{uuid}: {e}");
                }
            }

            let _ = db::cross_posts::CrossPostRow::delete_by_post(&state.db, uuid).await;

            sqlx::query("DELETE FROM posts WHERE id = $1")
                .bind(uuid)
                .execute(&state.db)
                .await
                .map_err(AppError::Database)?;

            return Ok(Json(serde_json::json!({"deleted": true, "id": uuid.to_string()})));
        }
    }

    // ── 2. Bluesky cached post (at:// URI) ───────────────────────────────
    if post_id.starts_with("at://") {
        tracing::info!("[delete] Attempting Bluesky cached post delete: {}", post_id);
        let accounts = db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id).await.unwrap_or_default();
        if let Some(acct) = accounts.iter().find(|a| a.provider == "bluesky") {
            let did = acct.metadata["did"].as_str().unwrap_or(&acct.external_id);
            let rkey = post_id.rsplit('/').next().unwrap_or("");
            tracing::info!("[delete] Bluesky DID={}, rkey={}", did, rkey);

            // Resolve PDS from DID instead of relying on stored instance
            let pds_url = match oauth::bluesky::resolve_pds_from_did(&http, did).await {
                Ok(pds) => {
                    tracing::info!("[delete] Resolved PDS from DID: {}", pds);
                    pds
                }
                Err(e) => {
                    tracing::warn!("[delete] PDS resolution failed ({}), falling back to stored PDS URL", e);
                    crate::oauth::bluesky::get_pds_url(&acct.metadata)
                }
            };

            if !rkey.is_empty() && !did.is_empty() {
                if let Err(e) = oauth::bluesky::delete_record(&state.db, acct, "app.bsky.feed.post", rkey).await {
                    tracing::error!("[delete] Bluesky deleteRecord failed for {}: {}", post_id, e);
                    return Ok(Json(serde_json::json!({
                        "deleted": false,
                        "errors": [format!("Bluesky: {}", e)],
                    })));
                }
                tracing::info!("[delete] Bluesky deleteRecord succeeded for {}", post_id);
                // Delete from cached_posts
                let _ = sqlx::query("DELETE FROM cached_posts WHERE user_id = $1 AND source = 'bluesky' AND external_id = $2")
                    .bind(user_id)
                    .bind(&post_id)
                    .execute(&state.db)
                    .await;
                return Ok(Json(serde_json::json!({"deleted": true, "id": post_id})));
            }
            tracing::error!("[delete] Bluesky delete skipped: rkey={} did={}", rkey, did);
        } else {
            tracing::error!("[delete] No Bluesky linked account found for user {}", user_id);
        }
    }

    // Lemmy cached post delete removed — Sprint 1 (Lemmy deletion)

    Err(AppError::NotFound("Post not found".into()))
}

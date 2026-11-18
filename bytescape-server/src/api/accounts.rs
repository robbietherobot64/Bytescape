//! API — Linked account management.
//! GET    /api/accounts       — list linked accounts
//! POST   /api/accounts/link  — link account via credentials or OAuth
//! DELETE /api/accounts/:id   — unlink an account

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth, error::AppError, oauth, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/accounts", get(list_accounts))
        .route("/api/accounts/link", post(link_account_credentials))
        .route("/api/accounts/refresh-follows", post(refresh_follows))
        .route("/api/accounts/{id}", delete(unlink_account))
        .route("/api/accounts/stats", get(account_stats))
        .route("/api/friends", get(mutual_friends))
        // Account deletion / restoration
        .route("/api/account/delete", post(delete_account))
        .route("/api/account/restore", post(restore_account))
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;

    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

/// One-per-network rule: reject a second link for a provider that already has
/// one. Exactly one Bluesky link and one Mastodon link per Bytescape account.
/// Applies to authenticated linking only (not fresh signup provisioning).
pub(crate) async fn ensure_one_per_network(
    db: &sqlx::PgPool,
    user_id: Uuid,
    provider: &str,
) -> Result<(), AppError> {
    let existing =
        crate::db::accounts::LinkedAccountRow::count_by_user_provider(db, user_id, provider).await?;
    if existing > 0 {
        let network = match provider {
            "bluesky" => "Bluesky",
            "mastodon" => "Mastodon",
            other => other,
        };
        return Err(AppError::BadRequest(format!(
            "You already have a {} account linked",
            network
        )));
    }
    Ok(())
}

#[derive(Serialize)]
struct AccountView {
    id: String,
    provider: String,
    external_username: String,
    created_at: String,
    following: u32,
    followers: u32,
    posts: u32,
    /// Mastodon only: how many home-instance follows the link-time import
    /// mirrored into `follows` (0 until an import has run).
    imported_follows: u32,
}

async fn list_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AccountView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let rows = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    let accounts: Vec<AccountView> = rows
        .into_iter()
        .map(|r| AccountView {
            id: r.id.to_string(),
            provider: r.provider,
            external_username: r.external_username,
            created_at: r.created_at.to_rfc3339(),
            following: r.metadata["following"].as_u64().unwrap_or(0) as u32,
            followers: r.metadata["followers"].as_u64().unwrap_or(0) as u32,
            posts: r.metadata["posts_count"].as_u64().unwrap_or(0) as u32,
            imported_follows: r.metadata["mastodon_following_imported"].as_u64().unwrap_or(0)
                as u32,
        })
        .collect();

    Ok(Json(accounts))
}

async fn unlink_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Get account info before deleting so we know the provider
    let account = crate::db::accounts::LinkedAccountRow::find_by_id(&state.db, id)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Account not found".into()))?;

    if account.user_id != user_id {
        return Err(AppError::NotFound("Account not found".into()));
    }

    let deleted = crate::db::accounts::LinkedAccountRow::delete(&state.db, id, user_id)
        .await
        .map_err(AppError::Database)?;

    if deleted == 0 {
        return Err(AppError::NotFound("Account not found".into()));
    }

    // Clean up follow subscriptions that were created from this provider
    let _ = crate::db::subscriptions::SubscriptionRow::delete_by_source(
        &state.db, user_id, &account.provider,
    )
    .await;
    // Clean up imported feeds from this provider (bluesky_saved_feed)
    let import_source = match account.provider.as_str() {
        "bluesky" => "bluesky_saved_feed",
        _ => "",
    };
    if !import_source.is_empty() {
        let _ = crate::db::subscriptions::SubscriptionRow::delete_by_source_any(
            &state.db, user_id, import_source,
        )
        .await;
    }
    // Lemmy community cleanup removed — Sprint 1 (Lemmy deletion)

    // Clean up imported follows from this provider
    if account.provider == "bluesky" {
        let follow_type = "bluesky_import";
        match sqlx::query("DELETE FROM follows WHERE follower_id = $1 AND follow_type = $2")
            .bind(user_id)
            .bind(follow_type)
            .execute(&state.db)
            .await
        {
            Ok(r) => {
                tracing::info!(user_id = %user_id, provider = %account.provider, deleted = r.rows_affected(), "Cleaned up imported follows on account unlink");
            }
            Err(e) => {
                tracing::error!(user_id = %user_id, provider = %account.provider, error = %e, "Failed to clean up imported follows on account unlink");
            }
        }
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[derive(Deserialize)]
struct LinkCredentialsReq {
    provider: String,           // "bluesky"
    identifier: String,         // Bluesky handle/email
    password: String,
    #[serde(default)]
    instance: Option<String>,   // Bluesky PDS URL (optional; defaults to bsky.social)
}

#[derive(Serialize)]
struct LinkResponse {
    id: String,
    provider: String,
    external_username: String,
    display_name: String,
    external_id: String,
}

async fn link_account_credentials(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LinkCredentialsReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let auth_user_id = crate::api::oauth::extract_user_id_optional(&headers, &state.config.jwt_secret);
    let is_login_flow = auth_user_id.is_none();
    let http = reqwest::Client::new();

    // For login flow (no auth), create a temporary Bytescape user now.
    // The real username will be set during the setup phase.
    let mut login_user: Option<(Uuid, String)> = None;
    if is_login_flow {
        // Unauthenticated "login flow" provisions a new account — that is a
        // signup and must respect the invite gate.
        if state.config.require_invite_code {
            return Err(AppError::Auth("Invite code required to create an account".into()));
        }
        let temp_username = format!("user_{}", uuid::Uuid::new_v4().to_string().replace('-', "").get(..12).unwrap_or("temp"));
        let temp_password = uuid::Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password(&temp_password)
            .map_err(|e| AppError::Internal(format!("Password hash failed: {}", e)))?;
        let empty_key = "0".repeat(64);
        let user = crate::db::users::UserRow::create_active(
            &state.db, &temp_username, "", &password_hash, &empty_key,
        ).await?;
        login_user = Some((user.id, temp_username));
    }
    let user_id = auth_user_id.or_else(|| login_user.as_ref().map(|(id, _)| *id))
        .ok_or_else(|| AppError::Auth("Authentication required".into()))?;

    // One-per-network rule: an AUTHENTICATED user may hold at most one link per
    // network (anti-spam-farm). This is linking, not the unauthenticated login/
    // signup provisioning path — a fresh account is exempt until it links.
    if auth_user_id.is_some() {
        ensure_one_per_network(&state.db, user_id, req.provider.as_str()).await?;
    }

    match req.provider.as_str() {
         "bluesky" => {
             let identifier = req.identifier.trim().trim_start_matches('@');
             if identifier.is_empty() {
                 return Err(AppError::BadRequest("Handle or email is required".into()));
             }
             let pds_url = req.instance.as_deref().unwrap_or("https://bsky.social").to_string();
             let (access_token, did, handle, refresh_token) = oauth::bluesky::create_session(
                 &http, &identifier, &req.password, &pds_url,
             )
             .await
             .map_err(|e| AppError::Internal(format!("Bluesky login failed: {}", e)))?;



            // Fetch profile stats
            let (followers, following, posts_count, display_name) = oauth::bluesky::get_profile(
                &http, &did, &access_token, &pds_url,
            )
            .await
            .map_err(|e| AppError::Internal(format!("Bluesky profile fetch failed: {}", e)))?;

            let display = if display_name.is_empty() { handle.clone() } else { display_name };

            let row = crate::db::accounts::LinkedAccountRow::create(
                &state.db,
                user_id,
                "bluesky",
                &did,
                &handle,
                &access_token,
                Some(&refresh_token),
                None,
                None,
                None,
                &serde_json::json!({"instance": pds_url, "did": did, "followers": followers, "following": following, "posts_count": posts_count}),
            )
            .await
            .map_err(AppError::Database)?;

            let username = row.external_username.clone();

            // Populate follow subscriptions from linked Bluesky account
            match oauth::bluesky::get_follows(&http, &did, &access_token, &pds_url).await {
                Ok(mut follows) => {
                    eprintln!("[link] API returned {} follows, creating subscriptions...", follows.len());

                    // Count authoritative follow records from the repo for comparison
                    match oauth::bluesky::count_follow_records(&http, &did, &pds_url, &access_token).await {
                        Ok(repo_dids) => {
                            eprintln!("[link] Repo has {} total follow records (vs {} from getFollows)", repo_dids.len(), follows.len());
                            if repo_dids.len() > follows.len() {
                                let follows_dids: std::collections::HashSet<&str> = follows.iter().map(|(d, _, _)| d.as_str()).collect();
                                let missing: Vec<String> = repo_dids.iter().filter(|d| !follows_dids.contains(d.as_str())).cloned().collect();
                                eprintln!("[link] Resolving {} missing follows via getProfiles...", missing.len());
                                match oauth::bluesky::get_profiles(&http, &pds_url, &access_token, &missing).await {
                                    Ok(profiles) => {
                                        let mut resolved_dids: std::collections::HashSet<String> = std::collections::HashSet::new();
                                        for p in &profiles {
                                            let pdid = p["did"].as_str().unwrap_or("").to_string();
                                            let phandle = p["handle"].as_str().unwrap_or("").to_string();
                                            let pdisplay = p["displayName"].as_str().unwrap_or("").to_string();
                                            if !pdid.is_empty() {
                                                resolved_dids.insert(pdid.clone());
                                                follows.push((pdid, phandle, pdisplay));
                                            }
                                        }
                                        for did in &missing {
                                            if !resolved_dids.contains(did.as_str()) {
                                                eprintln!("[link] Dead/unresolvable follow DID: {}", did);
                                                follows.push((did.clone(), String::new(), String::new()));
                                            }
                                        }
                                        eprintln!("[link] After merge: {} total follows ({} live + {} dead)", follows.len(), profiles.len(), missing.len() - profiles.len());
                                    }
                                    Err(e) => {
                                        eprintln!("[link] Failed to resolve missing profiles: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[link] Could not count repo follow records: {}", e);
                        }
                    }
                    let mut count = 0usize;
                    let mut failures = 0usize;
                    for (follow_did, fhandle, fdisplay) in &follows {
                        let label = if fdisplay.is_empty() {
                            if fhandle.is_empty() { &follow_did[follow_did.rfind(':').map_or(0, |i| i + 1)..] } else { fhandle }
                        } else { fdisplay };
                        if let Err(e) = crate::db::subscriptions::SubscriptionRow::create(
                            &state.db,
                            user_id,
                            "bluesky",
                            follow_did,
                            label,
                            "👤",
                            "follows",
                        )
                        .await
                        {
                            failures += 1;
                            if failures <= 3 {
                                eprintln!("[link] FAILED to create follow sub for did={} handle={}: {}", follow_did, fhandle, e);
                            }
                        } else {
                            count += 1;
                        }
                    }
                    eprintln!("[link] Created {} follow subscriptions ({} API, {} failures)", count, follows.len(), failures);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch Bluesky follows after link (non-fatal): {}", e);
                }
            }

            // Import saved Bluesky feeds as subscriptions
            match oauth::bluesky::get_saved_feeds(&http, &pds_url, &access_token).await {
                Ok(feed_uris) => {
                    let mut feed_count = 0usize;
                    for uri in &feed_uris {
                        let label = match oauth::bluesky::BlueskyPublicApi::get_feed_generator(uri).await {
                            Ok(info) => {
                                info["view"]["displayName"].as_str()
                                    .unwrap_or_else(|| uri.rsplit('/').next().unwrap_or(uri))
                                    .to_string()
                            }
                            Err(_) => uri.rsplit('/').next().unwrap_or(uri).to_string(),
                        };
                        if let Err(e) = crate::db::subscriptions::SubscriptionRow::create(
                            &state.db, user_id,
                            "bluesky_saved_feed",
                            uri,
                            &label,
                            "📡",
                            "feeds",
                        ).await {
                            tracing::warn!("Failed to create feed subscription for {}: {}", uri, e);
                        } else {
                            feed_count += 1;
                        }
                    }
                    tracing::info!("Imported {} saved Bluesky feeds as subscriptions", feed_count);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch Bluesky saved feeds (non-fatal): {}", e);
                }
            }

            let mut resp = serde_json::json!({
                "id": row.id.to_string(),
                "provider": "bluesky",
                "external_username": username.clone(),
                "display_name": display,
                "external_id": row.external_id,
            });
            if is_login_flow {
                let (uid, _) = login_user.as_ref().ok_or_else(|| AppError::Auth("Login session missing".into()))?;
                let access_token = crate::auth::create_token(*uid, &username, &state.config.jwt_secret)?;
                let manager = crate::auth::RefreshTokenManager { db: state.db.clone() };
                let (refresh_token, _) = manager.create(*uid).await?;
                resp["access_token"] = serde_json::Value::String(access_token);
                resp["refresh_token"] = serde_json::Value::String(refresh_token);
                resp["is_new_user"] = serde_json::Value::Bool(true);
            }
            Ok((StatusCode::CREATED, Json(resp)))
        }

        _ => Err(AppError::BadRequest(format!(
            "Unsupported provider: {}. Use 'bluesky'.",
            req.provider
        ))),
    }
}

#[derive(Serialize)]
struct AccountStatsResponse {
    following: u32,
    followers: u32,
    posts: u32,
}

async fn account_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AccountStatsResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    let mut total_following = 0u32;
    let mut total_followers = 0u32;
    let mut total_posts = 0u32;

    for acct in &accounts {
        if acct.provider == "bluesky" {
            total_following += acct.metadata["following"].as_u64().unwrap_or(0) as u32;
            total_followers += acct.metadata["followers"].as_u64().unwrap_or(0) as u32;
            total_posts += acct.metadata["posts_count"].as_u64().unwrap_or(0) as u32;
        }
    }

    Ok(Json(AccountStatsResponse {
        following: total_following,
        followers: total_followers,
        posts: total_posts,
    }))
}

#[derive(Serialize)]
struct MutualFriend {
    handle: String,
    display_name: String,
    avatar: Option<String>,
}

async fn mutual_friends(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MutualFriend>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    let mut all_mutual = Vec::new();

    // Include native Bytescape follows — get the FOLLOWED user's info, not the follower
    let native_follows: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"SELECT f.followed_actor,
           u.display_name
         FROM follows f
         LEFT JOIN users u ON u.username = REPLACE(REPLACE(f.followed_actor, 'https://bytescape.social/@', ''), 'http://bytescape.social/@', '')
         WHERE f.status = 'accepted'
         AND f.follower_id = $1
         AND f.follow_type = 'native'"#
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    for (actor, display) in native_follows {
        let handle = actor.split('/').last().unwrap_or(&actor).trim_start_matches('@').to_string();
        all_mutual.push(MutualFriend {
            handle: handle.clone(),
            display_name: display.unwrap_or_else(|| handle.clone()),
            avatar: None,
        });
    }

    for acct in &accounts {
        if acct.provider == "bluesky" {
            let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);
            let http = reqwest::Client::new();

            let follows = oauth::bluesky::get_follows(&http, &acct.external_id, &acct.access_token, &pds).await.unwrap_or_default();
            let followers = oauth::bluesky::get_followers(&http, &acct.external_id, &acct.access_token, &pds).await.unwrap_or_default();

            let follows_set: std::collections::HashSet<String> = follows.iter().map(|(did, _, _)| did.clone()).collect();
            for (did, handle, display, avatar) in &followers {
                if follows_set.contains(did) {
                    all_mutual.push(MutualFriend {
                        handle: handle.clone(),
                        display_name: display.clone(),
                        avatar: avatar.clone(),
                    });
                }
            }
        }
    }

    Ok(Json(all_mutual))
}

#[derive(Serialize)]
struct RefreshFollowsResponse {
    count: usize,
}

async fn refresh_follows(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RefreshFollowsResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let accounts = crate::db::accounts::LinkedAccountRow::find_by_user(&state.db, user_id)
        .await
        .map_err(AppError::Database)?;

    let http = reqwest::Client::new();
    let mut total = 0usize;

    for acct in &accounts {
        match acct.provider.as_str() {
            "bluesky" => {
                let pds = crate::oauth::bluesky::get_pds_url(&acct.metadata);

                let follows = match oauth::bluesky::get_follows(&http, &acct.external_id, &acct.access_token, &pds).await {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!("Failed to fetch follows for {}: {}", acct.external_username, e);
                        continue;
                    }
                };

                for (follow_did, handle, display) in &follows {
                    let label = if display.is_empty() { handle } else { display };
                    match crate::db::subscriptions::SubscriptionRow::create(
                        &state.db, user_id, "bluesky", follow_did, label, "👤", "follows",
                    ).await {
                        Ok(_) => total += 1,
                        Err(e) => { tracing::warn!("Failed to upsert follow for {}: {}", handle, e); }
                    }
                }

                // Also refresh saved Bluesky feeds
                match oauth::bluesky::get_saved_feeds(&http, &pds, &acct.access_token).await {
                    Ok(feed_uris) => {
                        for uri in &feed_uris {
                            let label = match oauth::bluesky::BlueskyPublicApi::get_feed_generator(uri).await {
                                Ok(info) => info["view"]["displayName"].as_str()
                                    .unwrap_or_else(|| uri.rsplit('/').next().unwrap_or(uri)).to_string(),
                                Err(_) => uri.rsplit('/').next().unwrap_or(uri).to_string(),
                            };
                            let _ = crate::db::subscriptions::SubscriptionRow::create(
                                &state.db, user_id, "bluesky_saved_feed", uri, &label, "📡", "feeds",
                            ).await;
                        }
                    }
                    Err(e) => { tracing::warn!("Failed to refresh saved Bluesky feeds: {}", e); }
                }
            }
            "mastodon" => {
                // Live re-import of the home-instance follow graph (the same
                // delete-then-reimport the sign-up/link hooks run). Synchronous
                // here: this endpoint is an explicit user action whose whole
                // point is the resulting count.
                let instance = acct
                    .metadata
                    .get("instance")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if instance.is_empty() {
                    tracing::warn!(
                        "Mastodon account {} has no instance metadata — skipping follow import",
                        acct.external_username
                    );
                    continue;
                }
                match crate::ap::follow_import::import_follows(
                    &state.db,
                    user_id,
                    &instance,
                    &acct.access_token,
                    &state.config.jwt_secret,
                )
                .await
                {
                    Ok(outcome) => {
                        crate::ap::follow_import::persist_outcome(&state.db, user_id, &outcome)
                            .await;
                        total += outcome.written;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to import Mastodon follows for {}: {}",
                            acct.external_username,
                            e
                        );
                    }
                }
            }
            _ => {}
        }
    }

    tracing::info!("Refresh-follows: upserted {} follows for user {}", total, user_id);
    Ok(Json(RefreshFollowsResponse { count: total }))
}

// ---------------------------------------------------------------------------
// Account Deletion & Restoration
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DeleteAccountReq {
    #[serde(default)]
    immediate: bool,
}

/// POST /api/account/delete — Soft-delete or immediately delete account.
///
/// Immediate mode: cascade-deletes all user data (FK cascades handle most tables).
/// Also cleans up tables without FK constraints: chat_messages, matrix_room_members.
/// Soft-delete mode: marks account for deletion in 30 days, revokes tokens.
async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeleteAccountReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    // Fetch username for response before deletion
    let username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch username: {}", e)))?;

    let username = username.ok_or_else(|| AppError::NotFound("User not found".into()))?;

    if req.immediate {
        // Federated retirement FIRST (add-identity-realignment): standing
        // remote interactions get their full-embed Undos and the actor's
        // Delete fan-out is frozen while followers/delivery history still
        // exist. The staged rows outlive the row delete below (user_id NULL)
        // and the drain worker delivers them. Staging before the content
        // purge is load-bearing: it marks standing originals undone_at, so
        // purge_native_repo's after_delete cannot double-emit retractions.
        if let Err(e) = crate::ap::retire::stage_actor_retirement(&state.db, user_id).await {
            tracing::warn!("[retire] staging for deleted account {user_id} failed: {e} — purging anyway");
        }
        // Home-REST favourites are NOT in `ap_activities` — retract them as
        // the home actor while the link tokens still exist (best-effort,
        // recorded; deletion never blocks).
        match crate::api::feed::retract_home_rest_reactions(&state.db, user_id).await {
            Ok(n) if n > 0 => tracing::info!("[delete] retracted {n} home-REST favourite(s) for {user_id}"),
            Ok(_) => {}
            Err(e) => tracing::warn!("[delete] home-REST retraction for {user_id} failed: {e} — purging anyway"),
        }

        // Purge Bytescape-authored content (stories + mirrored status) from the
        // AT Protocol repo before the DB row goes away. Per deletion policy this
        // removes com.bytescape.story + app.bsky.actor.status for BOTH native and
        // Bluesky-linked accounts; standard Bluesky records (app.bsky.feed.post)
        // are left intact for linked accounts (they belong to the user's Bluesky
        // presence). Native accounts get their full repo wiped separately below.
        let mut dids: Vec<String> = sqlx::query_scalar(
            "SELECT did FROM pds_accounts WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        let linked_dids: Vec<String> = sqlx::query_scalar(
            "SELECT external_id FROM linked_accounts WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        dids.extend(linked_dids);
        dids.sort();
        dids.dedup();
        for did in &dids {
            crate::api::stories::purge_author_content(&state.db, did).await;
        }
        // Native accounts: full repo wipe (Bytescape owns the PDS). Linked
        // (Bluesky) accounts keep their standard records — only the Bytescape
        // story/status above were removed.
        let native_dids: Vec<String> = sqlx::query_scalar(
            "SELECT did FROM pds_accounts WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        for did in &native_dids {
            crate::pds::purge_native_repo(&state.db, did).await;
        }

        // Clean up tables without FK constraints pointing to users
        // chat_messages: no FK to users, clean up by provider_account_id via linked_accounts
        let linked_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id::text FROM linked_accounts WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for lid in &linked_ids {
            sqlx::query("DELETE FROM chat_messages WHERE provider_account_id = $1")
                .bind(lid)
                .execute(&state.db)
                .await
                .ok();
        }

        // Also delete chat_messages where sender matches user's linked accounts
        // and matrix_room_members for user's linked accounts
        // These are best-effort cleanups — failure should not block deletion

        // Delete cached feed posts (FK cascade should handle this, but be defensive)
        sqlx::query("DELETE FROM cached_posts WHERE user_id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await
            .ok();

        // Now cascade-delete the user — all FK cascades fire
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete account: {}", e)))?;
        // Any previously-retired minted actor's staged rows purge now if its
        // drain already completed (spec: "purged with the account").
        crate::ap::retire::on_account_gone(&state.db, user_id).await;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("User not found".into()));
        }

        Ok(Json(serde_json::json!({
            "ok": true,
            "message": "Account permanently deleted",
            "immediate": true,
            "username": username,
        })))
    } else {
        // Soft-delete with 30-day grace period
        let delete_at = chrono::Utc::now() + chrono::Duration::days(30);
        let _updated = crate::db::users::UserRow::soft_delete(&state.db, user_id, delete_at)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        // Revoke all refresh tokens
        let manager = auth::RefreshTokenManager { db: state.db.clone() };
        let _ = manager.revoke_all_for_user(user_id).await;

        Ok(Json(serde_json::json!({
            "ok": true,
            "message": "Account scheduled for deletion",
            "scheduled": delete_at.to_rfc3339(),
            "override_available": true,
            "username": username,
        })))
    }
}

/// POST /api/account/restore — Restore a soft-deleted account within grace period.
async fn restore_account(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let restored = crate::db::users::UserRow::restore(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Account is not deleted or cannot be restored".into()))?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "message": "Account restored",
        "user_id": restored.id.to_string(),
        "username": restored.username,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::testutil::{connect_pool, enable_ap_flags, make_local_actor, test_state, unique};
    use sqlx::PgPool;

    /// Seed one standing outbound Like + its delivery-history row (the shape
    /// `insert_activity` + a delivered fan-out leave behind).
    async fn seed_standing_like(pool: &PgPool, actor_id: &str, user_id: Uuid, suffix: &str) -> String {
        let id = format!("{actor_id}/activities/like-{suffix}");
        let payload = serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": id,
            "type": "Like",
            "actor": actor_id,
            "object": format!("https://remote.test/users/z/statuses/{suffix}"),
            "to": ["https://remote.test/users/z"],
        });
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload, user_id) \
             VALUES ($1, $2, 'Like', $3, 'out', $4, $5)",
        )
        .bind(&id)
        .bind(actor_id)
        .bind(format!("https://remote.test/users/z/statuses/{suffix}"))
        .bind(&payload)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed like");
        crate::ap::delivery::enqueue(pool, &id, "https://remote.test/users/z/inbox")
            .await
            .expect("seed delivery history");
        sqlx::query("UPDATE ap_delivery_queue SET status = 'delivered' WHERE activity_id = $1")
            .bind(&id)
            .execute(pool)
            .await
            .expect("history delivered");
        id
    }

    /// Task 4.1 acceptance (handler level): `delete_account(immediate)` stages
    /// the shared retraction+Delete routine BEFORE the purge — the standing
    /// remote favourite gets its full-embed Undo and the `Delete` is queued to
    /// every delivery-history inbox before the row delete, and the staged rows
    /// outlive the account (user_id NULL beats the 087 cascade).
    #[tokio::test]
    async fn immediate_delete_stages_retraction_and_delete_before_purge() {
        let _due = crate::ap::testutil::due_rows_lock().lock().await;
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = unique();
        let like_id = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;

        let jwt = crate::auth::create_token(la.user.id, &la.username, &state.config.jwt_secret)
            .expect("token");
        let app = crate::api::routes()
            .merge(crate::ap::routes())
            .with_state(state.clone());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/account/delete")
            .header("authorization", format!("Bearer {jwt}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"immediate":true}"#))
            .expect("delete request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "delete completes");
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
        let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
        assert_eq!(doc["ok"], serde_json::json!(true));

        // Account row gone; staged rows survive (the drain delivers them).
        assert!(
            crate::db::users::UserRow::find_by_id(&pool, la.user.id).await.expect("reload").is_none(),
            "account deleted"
        );
        // Undo embeds the exact original and is queued to the delivery-history
        // inbox BEFORE the row delete — the Delete(Tombstone) is deliberately
        // deferred until the retractions leave `pending` (FIFO tombstone
        // ordering, drain pass 2), then finalize purges the staged rows.
        let (undo_id, undo_payload): (String, serde_json::Value) = sqlx::query_as(
            "SELECT id, payload FROM ap_activities \
             WHERE actor_id = $1 AND activity_type = 'Undo'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("staged undo");
        assert_eq!(
            undo_payload["object"]["id"], serde_json::json!(like_id),
            "Undo embeds the exact original (Mastodon undo_like rule)"
        );
        let (queued,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue \
             WHERE inbox_url = 'https://remote.test/users/z/inbox' AND activity_id = $1",
        )
        .bind(&undo_id)
        .fetch_one(&pool)
        .await
        .expect("queue rows");
        assert_eq!(queued, 1, "retraction queued to every delivery-history inbox");

        // Drain choreography: retractions first…
        sqlx::query("UPDATE ap_delivery_queue SET status = 'delivered' WHERE activity_id = $1")
            .bind(&undo_id)
            .execute(&pool)
            .await
            .expect("deliver undo");
        crate::ap::retire::drain_once(&pool).await.expect("drain 1");
        let (delete_payload,): (serde_json::Value,) = sqlx::query_as(
            "SELECT payload FROM ap_activities \
             WHERE actor_id = $1 AND activity_type = 'Delete'",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("staged delete after retractions drain");
        assert_eq!(
            delete_payload["object"]["type"], serde_json::json!("Tombstone"),
            "Delete(Tombstone) staged once retractions left pending"
        );
        let (delivered_del,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_delivery_queue \
             WHERE inbox_url = 'https://remote.test/users/z/inbox' AND activity_id = $1",
        )
        .bind(delete_payload["id"].as_str().unwrap())
        .fetch_one(&pool)
        .await
        .expect("delete queue rows");
        assert_eq!(delivered_del, 1, "Delete queued to every delivery-history inbox");

        // …then finalize purges the staged rows ("purged with the account").
        sqlx::query("UPDATE ap_delivery_queue SET status = 'delivered' WHERE activity_id = $1")
            .bind(delete_payload["id"].as_str().unwrap())
            .execute(&pool)
            .await
            .expect("deliver delete");
        crate::ap::retire::drain_once(&pool).await.expect("drain 2 (finalize)");
        let (remaining,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE actor_id = $1",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("staged rows");
        assert_eq!(remaining, 0, "staged rows purged after the drain");

        let _ = sqlx::query(
            "DELETE FROM ap_delivery_queue WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)",
        )
        .bind(&la.actor_id)
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM retired_ap_actors WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
    }

    /// Task 4.3 acceptance (death-path half): soft-delete and restore emit
    /// NOTHING federated — only real death federates (design.md Decision 4).
    /// Complements `ap/retire.rs::soft_delete_and_restore_emit_no_federation`.
    #[tokio::test]
    async fn soft_delete_and_restore_through_handlers_emit_nothing() {
        enable_ap_flags();
        let Some(pool) = connect_pool().await else { return };
        let Some(state) = test_state(&pool) else { return };
        let la = make_local_actor(&pool, "bytescape.test").await;
        let suffix = unique();
        let _ = seed_standing_like(&pool, &la.actor_id, la.user.id, &suffix).await;
        let jwt = crate::auth::create_token(la.user.id, &la.username, &state.config.jwt_secret)
            .expect("token");

        let app = crate::api::routes().with_state(state.clone());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/account/delete")
            .header("authorization", format!("Bearer {jwt}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"immediate":false}"#))
            .expect("soft delete request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200);

        let app = crate::api::routes().with_state(state);
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/account/restore")
            .header("authorization", format!("Bearer {jwt}"))
            .body(axum::body::Body::empty())
            .expect("restore request");
        let resp = tower::ServiceExt::oneshot(app, req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200);

        let (federated,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM ap_activities WHERE actor_id = $1 AND activity_type IN ('Undo','Delete')",
        )
        .bind(&la.actor_id)
        .fetch_one(&pool)
        .await
        .expect("federated count");
        assert_eq!(federated, 0, "nothing federated at soft-delete or restore");
        assert!(
            crate::ap::retire::load_retired_actor(&pool, &la.actor_id).await.is_none(),
            "no retirement staged on soft-delete/restore"
        );

        let _ = sqlx::query(
            "DELETE FROM ap_delivery_queue WHERE activity_id IN (SELECT id FROM ap_activities WHERE actor_id = $1)",
        )
        .bind(&la.actor_id)
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM ap_activities WHERE actor_id = $1")
            .bind(&la.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(la.user.id)
            .execute(&pool)
            .await;
    }
}

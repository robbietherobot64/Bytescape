use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::StreamExt;
use serde::Serialize;
use std::convert::Infallible;
use std::io::Read;
use tokio::io::AsyncWriteExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;

use crate::{auth, db, error::AppError, AppState};
use sqlx::Row;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/import", post(unified_import))
        .route("/api/import/stream", post(unified_import_stream))
        .route("/api/import/{session_id}/social-graph", get(social_graph_handler))
        .layer(DefaultBodyLimit::disable())
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

#[derive(Serialize)]
struct ImportResponse {
    imported: usize,
    session_id: String,
    source: String,
    errors: Vec<String>,
}

async fn unified_import(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let temp_path = format!("/tmp/bytescape_import_{}.zip", Uuid::new_v4());
    let mut temp_file = tokio::fs::File::create(&temp_path).await
        .map_err(|e| AppError::Internal(format!("Failed to create temp file: {}", e)))?;

    let mut found_field = false;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            found_field = true;
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::BadRequest(format!("Stream error: {}", e)))?
            {
                temp_file.write_all(&chunk).await
                    .map_err(|e| AppError::Internal(format!("Write error: {}", e)))?;
            }
            break;
        }
    }

    if !found_field {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest("No file provided".into()));
    }

    // Flush temp file to disk before handing off to sync reader
    temp_file.sync_all().await
        .map_err(|e| AppError::Internal(format!("Failed to flush temp file: {}", e)))?;

    // Detect source by peeking at ZIP contents
    let source = {
        let std_file = std::fs::File::open(&temp_path)
            .map_err(|e| AppError::BadRequest(format!("Failed to open file: {}", e)))?;
        let mut archive = zip::ZipArchive::new(std_file)
            .map_err(|e| AppError::BadRequest(format!("Invalid ZIP: {}", e)))?;

        let mut detected = String::new();
        let len = archive.len();
        for i in 0..len {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name();
                // Mastodon GDPR archives: outbox.json / actor.json at any depth.
                if name.ends_with("outbox.json") || name.ends_with("actor.json") {
                    detected = "mastodon".to_string();
                    break;
                } else if name.contains("/posts/") || name.contains("/posts_and_comments/") || name.contains("your_facebook_activity/") || name.contains("your_posts.html") || name.contains("your_posts_1.html") {
                    detected = "facebook".to_string();
                    break;
                } else if name.contains("your_instagram_activity/") || name.contains("/content/posts_") || name.contains("/media/") {
                    detected = "instagram".to_string();
                    break;
                } else if name.contains("your_threads_activity/") || name.contains("threads_activity/") || name.contains("/threads_posts/") {
                    detected = "threads".to_string();
                    break;
                }
            }
        }
        detected
    };

    if source.is_empty() {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest("Could not detect archive source. Expected Facebook, Instagram, Threads, or Mastodon ZIP.".into()));
    }

    // Delegate to the appropriate handler
    let result = match source.as_str() {
        "facebook" => {
            crate::api::import_facebook::import_facebook_archive_from_path(
                &state, user_id, &temp_path).await
        }
        "instagram" => {
            crate::api::import_instagram::import_instagram_archive_from_path(
                &state, user_id, &temp_path).await
        }
        "threads" => {
            // Threads uses the same parser as Instagram (both Meta JSON format)
            crate::api::import_instagram::import_instagram_archive_from_path(
                &state, user_id, &temp_path).await
        }
        "mastodon" => {
            crate::api::import_mastodon::import_mastodon_archive_from_path(
                &state, user_id, &temp_path).await
        }
        _ => {
            tokio::fs::remove_file(&temp_path).await.ok();
            return Err(AppError::BadRequest(format!("Unsupported source: {}", source)));
        }
    };

    tokio::fs::remove_file(&temp_path).await.ok();

    match result {
        Ok((imported, session_id, errors)) => {
            Ok(Json(ImportResponse {
                imported,
                session_id,
                source,
                errors,
            }))
        }
        Err(e) => Err(e),
    }
}

async fn unified_import_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>> + Send + 'static>,
    AppError,
> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let temp_path = format!("/tmp/bytescape_import_{}.zip", Uuid::new_v4());
    let mut temp_file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create temp file: {}", e)))?;

    let mut found_field = false;
    let mut original_filename = String::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            found_field = true;
            if let Some(name) = field.file_name() {
                original_filename = name.to_string();
            }
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::BadRequest(format!("Stream error: {}", e)))?
            {
                temp_file.write_all(&chunk).await
                    .map_err(|e| AppError::Internal(format!("Write error: {}", e)))?;
            }
            break;
        }
    }

    if !found_field {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest("No file provided".into()));
    }

    temp_file.sync_all().await
        .map_err(|e| AppError::Internal(format!("Failed to flush temp file: {}", e)))?;

    // Detect source by peeking at ZIP contents
    let source = {
        let std_file = std::fs::File::open(&temp_path)
            .map_err(|e| AppError::BadRequest(format!("Failed to open file: {}", e)))?;
        let mut archive = zip::ZipArchive::new(std_file)
            .map_err(|e| AppError::BadRequest(format!("Invalid ZIP: {}", e)))?;

        let mut detected = String::new();
        let len = archive.len();
        for i in 0..len {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name();
                if name.ends_with("outbox.json") || name.ends_with("actor.json") {
                    detected = "mastodon".to_string();
                    break;
                } else if name.contains("/posts/") || name.contains("/posts_and_comments/") || name.contains("your_facebook_activity/") || name.contains("your_posts.html") || name.contains("your_posts_1.html") {
                    detected = "facebook".to_string();
                    break;
                } else if name.contains("your_instagram_activity/") || name.contains("/content/posts_") || name.contains("/media/") {
                    detected = "instagram".to_string();
                    break;
                } else if name.contains("your_threads_activity/") || name.contains("threads_activity/") || name.contains("/threads_posts/") {
                    detected = "threads".to_string();
                    break;
                }
            }
        }
        detected
    };

    if source.is_empty() {
        tokio::fs::remove_file(&temp_path).await.ok();
        return Err(AppError::BadRequest(
            "Could not detect archive source. Expected Facebook, Instagram, Threads, or Mastodon ZIP.".into(),
        ));
    }

    // Detect source from ZIP contents
    let source = {
        let file = std::fs::File::open(&temp_path).map_err(|e| AppError::Internal(format!("Failed to open temp file: {}", e)))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| AppError::BadRequest(format!("Invalid ZIP: {}", e)))?;
        let mut detected = String::new();
        for i in 0..zip.len() {
            let name = {
                let entry = zip.by_index(i).map_err(|e| AppError::Internal(format!("ZIP read error: {}", e)))?;
                entry.name().to_string()
            };
            if name.ends_with("outbox.json") || name.ends_with("actor.json") {
                detected = "mastodon".to_string();
                break;
            } else if name.contains("your_facebook_activity/") {
                detected = "facebook".to_string();
                break;
            } else if name.contains("your_instagram_activity/") {
                detected = "instagram".to_string();
                break;
            } else if name.contains("threads/threads_and_replies") || name.contains("text_post_app_text_posts") || name.contains("your_threads_activity/") {
                detected = "threads".to_string();
                break;
            }
        }
        if detected.is_empty() {
            return Err(AppError::BadRequest("Could not detect archive source. Expected Facebook, Instagram, Threads, or Mastodon ZIP.".into()));
        }
        detected
    };

    // Use a tokio unbounded channel — UnboundedSender is Send + Sync,
    // so it can be used from a std::thread::spawn context
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Capture the tokio runtime handle BEFORE spawning the thread
    let runtime_handle = tokio::runtime::Handle::current();

    let temp_path_clone = temp_path.clone();
    let original_filename_clone = original_filename.clone();
    let state_clone = state.clone();
    let tx_clone = tx.clone();
    let source_clone = source.clone();

    // Spawn the heavy parsing in a real OS thread — completely outside tokio
    std::thread::spawn(move || {
        let (posts, errors) = match source_clone.as_str() {
            "facebook" => {
                crate::api::import_facebook::parse_facebook_archive_sync(
                    &temp_path_clone, &crate::api::import::tokio_to_std_sender(&tx_clone),
                )
            }
            "threads" => {
                crate::api::import_instagram::parse_threads_archive_sync(
                    &temp_path_clone, &crate::api::import::tokio_to_std_sender(&tx_clone),
                )
            }
            "instagram" => {
                crate::api::import_instagram::parse_instagram_archive_sync(
                    &temp_path_clone, &crate::api::import::tokio_to_std_sender(&tx_clone),
                )
            }
            "mastodon" => {
                crate::api::import_mastodon::parse_mastodon_archive_sync(
                    &temp_path_clone, &crate::api::import::tokio_to_std_sender(&tx_clone),
                )
            }
            _ => (vec![], vec!["Unsupported source".to_string()]),
        };

        // ─── Contact parsing ─────────────────────────────────────────────────
        // Re-open the ZIP to extract contacts from friends/followers files
        let contacts = parse_contacts_from_archive(&temp_path_clone, &source_clone);
        eprintln!("[import] Parsed {} contacts from archive (source={})", contacts.len(), source_clone);

        // Extract the archive owner's own identity (handle, name) for matching
        let owner_identity = parse_archive_owner_handle(&temp_path_clone, &source_clone, &original_filename_clone);
        eprintln!("[import] Owner identity from archive: {:?}", owner_identity);

        // DB inserts + matching — use the captured runtime handle
        runtime_handle.block_on(async move {
            let send = |val: serde_json::Value| {
                let _ = tx_clone.send(val.to_string());
            };

            send(serde_json::json!({"type":"phase","phase":"inserting","total":posts.len()}));

            let session = match db::imports::ImportRow::create(&state_clone.db, user_id, &source_clone).await {
                Ok(s) => s,
                Err(e) => {
                    send(serde_json::json!({"type":"error","message":format!("Failed to create import session: {}", e)}));
                    send(serde_json::json!({"type":"complete","imported":0,"errors":errors}));
                    return;
                }
            };

            db::imports::ImportRow::start(&state_clone.db, session.id).await.ok();

            let mut imported = 0usize;
            // Distinct local media files referenced by successfully imported
            // posts — mirrors what the sync parser extracted to ./uploads/import/.
            // (R4: import_sessions must record total/imported media accurately;
            // this previously always recorded 0.)
            let mut imported_media: std::collections::HashSet<String> = std::collections::HashSet::new();
            for post in &posts {
                let content_obj = serde_json::json!({
                    "text": post.content_text,
                    "display_name": post.display_name,
                    "action_text": post.action_text,
                    "group_name": post.group_name,
                    "link_url": post.link_url,
                    "link_title": post.link_title,
                    "media_urls": post.media_urls,
                });

                match db::posts::PostRow::create(
                    &state_clone.db, user_id, &source_clone, None, None, &content_obj, "short",
                    None, None, None, post.original_ts, true, Some(session.id), "public",
                ).await {
                    Ok(_) => {
                        imported += 1;
                        imported_media.extend(
                            post.media_urls.iter().filter(|u| u.starts_with("/uploads/import/")).cloned(),
                        );
                    }
                    Err(e) => {
                        if errors.len() < 10 {
                            send(serde_json::json!({"type":"error","message":format!("DB insert error: {}", e)}));
                        }
                    }
                }
            }

            if imported > 0 {
                db::imports::ImportRow::complete(&state_clone.db, session.id, imported as i32, imported_media.len() as i32).await.ok();
            } else {
                db::imports::ImportRow::fail(
                    &state_clone.db, session.id, &format!("No posts imported. Errors: {}", errors.join("; "))).await.ok();
            }

            // ─── Store archive owner identity ─────────────────────────────────
            if let Some(ref identity) = owner_identity {
                let _ = db::archive_identities::ArchiveIdentityRow::create(
                    &state_clone.db,
                    &db::archive_identities::NewArchiveIdentity {
                        user_id,
                        platform: source_clone.clone(),
                        handle: identity.handle.clone(),
                        display_name: identity.display_name.clone(),
                        profile_url: identity.profile_url.clone(),
                        import_session_id: Some(session.id),
                    },
                ).await;
                eprintln!("[import] Stored archive identity: handle={:?}, name={:?}", identity.handle, identity.display_name);
            }

            // ─── Store contacts ──────────────────────────────────────────────
            if !contacts.is_empty() {
                let contact_count = db::imported_contacts::ImportedContactRow::count_by_session_and_user(
                    &state_clone.db, session.id, user_id,
                ).await.unwrap_or(0);

                if contact_count == 0 {
                    let new_contacts: Vec<db::imported_contacts::NewContact> = contacts.iter().map(|c| {
                        db::imported_contacts::NewContact {
                            user_id,
                            contact_name: c.contact_name.clone(),
                            contact_handle: c.contact_handle.clone(),
                            contact_platform_url: c.contact_platform_url.clone(),
                            source: c.source.clone(),
                            remote_handle: c.remote_handle.clone(),
                            direction: c.direction.clone(),
                            import_session_id: Some(session.id),
                        }
                    }).collect();
                    let stored = db::imported_contacts::ImportedContactRow::create_batch(
                        &state_clone.db, &new_contacts,
                    ).await.unwrap_or(0);

                    send(serde_json::json!({"type":"social","phase":"mapping_connections","total_contacts":stored}));

                    // ─── Run matching ────────────────────────────────────────
                    let mut handle_matched = 0i64;
                    let mut name_matched = 0i64;

                    // Handle-based matching (one-way, certain)
                    // Match contact_handle against other users' archive_identities.handle
                    let handle_matches = sqlx::query_as::<_, MatchResult>(
                        r#"
                        SELECT DISTINCT u.id as user_id, u.display_name, u.username, u.avatar_url,
                                ai.handle as match_handle, 'handle_match' as match_type, 'certain' as confidence
                        FROM imported_contacts ic
                        JOIN archive_identities ai ON LOWER(ai.handle) = LOWER(ic.contact_handle)
                        JOIN users u ON u.id = ai.user_id
                        WHERE ic.user_id = $1 AND ic.import_session_id = $2
                          AND ic.contact_handle IS NOT NULL
                          AND u.id != $1
                        "#,
                    )
                    .bind(user_id)
                    .bind(session.id)
                    .fetch_all(&state_clone.db)
                    .await
                    .unwrap_or_default();

                    for m in &handle_matches {
                        let _ = db::social_graph_matches::SocialGraphMatchRow::create(
                            &state_clone.db, user_id, m.user_id, "handle_match", &source_clone, "certain",
                        ).await;
                        // Create reverse match so the other user sees it too
                        let _ = db::social_graph_matches::SocialGraphMatchRow::create(
                            &state_clone.db, m.user_id, user_id, "handle_match", &source_clone, "certain",
                        ).await;

                        // Auto-follow back: create a follow from the matched user TO the importing user
                        // so both users end up following each other
                        let matched_actor_url = format!("https://{}/@{}", state_clone.config.domain, m.username);
                        let importer_username = sqlx::query_scalar::<_, String>(
                            "SELECT username FROM users WHERE id = $1"
                        )
                        .bind(user_id)
                        .fetch_one(&state_clone.db)
                        .await
                        .unwrap_or_default();
                        let importer_actor_url = format!("https://{}/@{}", state_clone.config.domain, importer_username);

                        // Follow from matched user → importing user
                        let _ = db::follows::FollowRow::create(
                            &state_clone.db, m.user_id, &importer_actor_url, "native", None,
                        ).await;
                        let _ = db::follows::FollowRow::set_accepted(
                            &state_clone.db, m.user_id, &importer_actor_url,
                        ).await;

                        // Also follow from importing user → matched user
                        let _ = db::follows::FollowRow::create(
                            &state_clone.db, user_id, &matched_actor_url, "native", None,
                        ).await;
                        let _ = db::follows::FollowRow::set_accepted(
                            &state_clone.db, user_id, &matched_actor_url,
                        ).await;

                        // Notify the matched user
                        let _ = db::notifications::NotificationRow::create(
                            &state_clone.db, m.user_id, "social_graph_match",
                            "New friend from your archive!",
                            Some("Someone who follows you just joined Bytescape."),
                            Some(serde_json::json!({"matched_user_id": user_id, "match_type": "handle_match", "source": source_clone})),
                        ).await;

                        handle_matched += 1;
                    }

                    // Name-based matching (two-way, likely)
                    // Match contact_name against other users' archive_identities.display_name
                    let name_match_candidates = sqlx::query_as::<_, NameMatchCandidate>(
                        r#"
                        SELECT DISTINCT u.id, u.display_name, u.username, u.avatar_url, ic.contact_name
                        FROM imported_contacts ic
                        JOIN archive_identities ai ON (
                            LOWER(ai.display_name) = LOWER(ic.contact_name)
                            OR LOWER(ai.display_name) = LOWER(split_part(ic.contact_name, ' ', 1) || ' ' || split_part(ic.contact_name, ' ', 2))
                            OR LOWER(ic.contact_name) LIKE '%' || LOWER(ai.display_name) || '%'
                            OR LOWER(ai.display_name) LIKE '%' || LOWER(ic.contact_name) || '%'
                        )
                        JOIN users u ON u.id = ai.user_id
                        WHERE ic.user_id = $1 AND ic.import_session_id = $2
                        AND u.id != $1
                        "#,
                    )
                    .bind(user_id)
                    .bind(session.id)
                    .fetch_all(&state_clone.db)
                    .await
                    .unwrap_or_default();

                    for candidate in &name_match_candidates {
                        // Check if the OTHER user ALSO has the importing user in their contacts (two-way)
                        // Use the importing user's archive identity (handle and display_name)
                        let our_identities = db::archive_identities::ArchiveIdentityRow::find_by_user(
                            &state_clone.db, user_id,
                        ).await;

                        let mut reverse_found = false;
                        for our_id in &our_identities {
                            let check_name = our_id.display_name.as_deref().unwrap_or("");
                            let check_handle = our_id.handle.as_deref().unwrap_or("");
                            let reverse_match = sqlx::query_scalar::<_, i64>(
                                r#"
                                SELECT COUNT(*) FROM imported_contacts
                                WHERE user_id = $1 AND (
                                    LOWER(contact_name) = LOWER($2)
                                    OR LOWER(contact_name) LIKE '%' || LOWER($2) || '%'
                                    OR LOWER(contact_handle) = LOWER($3)
                                )
                                "#,
                            )
                            .bind(candidate.id)
                            .bind(check_name)
                            .bind(check_handle)
                            .fetch_one(&state_clone.db)
                            .await
                            .unwrap_or(0);

                            if reverse_match > 0 {
                                reverse_found = true;
                                break;
                            }
                        }

                        if reverse_found {
                            let _ = db::social_graph_matches::SocialGraphMatchRow::create(
                                &state_clone.db, user_id, candidate.id,
                                "name_match", &source_clone, "likely",
                            ).await;
                            // Create reverse match
                            let _ = db::social_graph_matches::SocialGraphMatchRow::create(
                                &state_clone.db, candidate.id, user_id,
                                "name_match", &source_clone, "likely",
                            ).await;
                            // Notify the matched user
                            let _ = db::notifications::NotificationRow::create(
                                &state_clone.db, candidate.id, "social_graph_match",
                                "New friend from your archive!",
                                Some("Someone who follows you just joined Bytescape."),
                                Some(serde_json::json!({"matched_user_id": user_id, "match_type": "name_match", "source": source_clone})),
                            ).await;
                            name_matched += 1;
                        }
                    }

                    let total_matched = handle_matched + name_matched;

                    // Mastodon: handle/instance-aware matching on top of the
                    // generic handle matching above (spec R3).
                    if source_clone == "mastodon" {
                        match crate::api::import_mastodon::match_mastodon_contacts(
                            &state_clone.db, user_id, session.id,
                        ).await {
                            Ok(n) => eprintln!("[import] mastodon-specific matching linked {} contacts", n),
                            Err(e) => eprintln!("[import] mastodon contact matching failed: {}", e),
                        }
                    }

                    // Categorize contacts (matched, rss_candidate, unreachable)
                    if let Err(e) = db::imported_contacts::ImportedContactRow::categorize_contacts(
                        &state_clone.db, session.id, user_id,
                    ).await {
                        eprintln!("[import] Warning: failed to categorize contacts: {}", e);
                    }

                    send(serde_json::json!({"type":"social","phase":"connections_mapped","matched":total_matched,"handle_matched":handle_matched,"name_matched":name_matched}));
                    send(serde_json::json!({"type":"social","phase":"fediverse_check","friends_on_fediverse":total_matched}));
                }
            }

            send(serde_json::json!({"type":"complete","imported":imported,"errors":errors,"session_id": session.id.to_string(),"source":source_clone}));

            tokio::fs::remove_file(&temp_path_clone).await.ok();
        });
    });

    let stream = UnboundedReceiverStream::new(rx)
        .map(|json| Ok::<Event, Infallible>(Event::default().data(json)));

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// A contact entry parsed from an archive (friend, follower, following).
#[derive(Debug, Clone)]
pub struct ContactEntry {
    pub contact_name: String,
    pub contact_handle: Option<String>,
    pub contact_platform_url: Option<String>,
    pub source: String,
    /// Canonical ActivityPub handle (`@user@instance`) — Mastodon archives only.
    pub remote_handle: Option<String>,
    /// Direction in the archive: "following" / "follower" (Mastodon only).
    pub direction: Option<String>,
}

// ─── Matching result types ──────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
struct MatchResult {
    user_id: uuid::Uuid,
    display_name: String,
    username: String,
    avatar_url: Option<String>,
    match_handle: String,
    match_type: String,
    confidence: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct NameMatchCandidate {
    id: uuid::Uuid,
    display_name: String,
    username: String,
    avatar_url: Option<String>,
    contact_name: String,
}

/// Parse contacts from a downloaded archive ZIP.
/// Opens the ZIP, reads the known contact file paths, and returns all ContactEntry items.
/// Extract the archive owner's own identity (handle, name, profile_url) from the archive.
/// Threads: threads/personal_information.json → string_map_data.Username
/// Instagram: ZIP filename pattern "instagram-{handle}-..." 
/// Facebook: personal_information/profile_information/profile_information.json → profile_v2
fn parse_archive_owner_handle(temp_path: &str, source: &str, original_filename: &str) -> Option<OwnerIdentity> {
    let file = std::fs::File::open(temp_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    match source {
        "threads" => {
            for i in 0..archive.len() {
                let name = {
                    let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
                    entry.name().to_string()
                };
                // JSON format
                if name.ends_with("personal_information.json") && name.contains("threads") {
                    let mut buf = Vec::new();
                    if let Ok(mut entry) = archive.by_index(i) {
                        entry.read_to_end(&mut buf).ok();
                    }
                    if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&buf) {
                        let profile = json_val.get("text_post_app_text_post_app_profile")
                            .and_then(|v| v.as_array())
                            .and_then(|arr| arr.first());
                        if let Some(p) = profile {
                            let handle = p.get("string_map_data")
                                .and_then(|m| m.get("Username"))
                                .and_then(|u| u.get("value"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            let display_name = p.get("string_map_data")
                                .and_then(|m| m.get("Name"))
                                .and_then(|u| u.get("value"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            return Some(OwnerIdentity { handle, display_name, profile_url: None });
                        }
                    }
                }
                // HTML format
                if name.ends_with("personal_information.html") && name.contains("threads") {
                    let mut buf = Vec::new();
                    if let Ok(mut entry) = archive.by_index(i) {
                        entry.read_to_end(&mut buf).ok();
                    }
                    let html = String::from_utf8_lossy(&buf);
                    // Extract Username: "Username<div><div>handle</div></div>"
                    let handle = extract_html_field(&html, "Username");
                    let display_name = extract_html_field(&html, "Name");
                    if handle.is_some() || display_name.is_some() {
                        return Some(OwnerIdentity { handle, display_name, profile_url: None });
                    }
                }
            }
            None
        }
        "instagram" => {
            // Extract handle from original filename: instagram-{handle}-{date}-{id}.zip
            if let Some(start) = original_filename.find("instagram-") {
                let rest = &original_filename[start + 10..]; // skip "instagram-"
                // Handle is everything before the date pattern (YYYY-MM-DD)
                if let Some(date_start) = rest.find(|c: char| c.is_ascii_digit()) {
                    let handle = rest[..date_start].trim_end_matches('-').to_string();
                    if !handle.is_empty() {
                        return Some(OwnerIdentity { handle: Some(handle), display_name: None, profile_url: None });
                    }
                }
            }
            None
        }
        "facebook" => {
            for i in 0..archive.len() {
                let name = {
                    let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
                    entry.name().to_string()
                };
                // JSON format
                if name.ends_with("profile_information.json") && name.contains("personal_information") {
                    let mut buf = Vec::new();
                    if let Ok(mut entry) = archive.by_index(i) {
                        entry.read_to_end(&mut buf).ok();
                    }
                    if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&buf) {
                        let profile = json_val.get("profile_v2");
                        if let Some(p) = profile {
                            let full_name = p.get("name")
                                .and_then(|n| n.get("full_name"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            let profile_uri = p.get("profile_uri")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            return Some(OwnerIdentity { handle: None, display_name: full_name, profile_url: profile_uri });
                        }
                    }
                }
                // HTML format — extract name from profile page
                if name.ends_with("profile_information.html") && name.contains("personal_information") {
                    let mut buf = Vec::new();
                    if let Ok(mut entry) = archive.by_index(i) {
                        entry.read_to_end(&mut buf).ok();
                    }
                    let html = String::from_utf8_lossy(&buf);
                    let display_name = extract_html_field(&html, "Full Name");
                    if display_name.is_some() {
                        return Some(OwnerIdentity { handle: None, display_name, profile_url: None });
                    }
                }
            }
            None
        }
        "mastodon" => {
            for i in 0..archive.len() {
                let name = {
                    let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
                    entry.name().to_string()
                };
                if name.rsplit('/').next() == Some("actor.json") {
                    let mut buf = Vec::new();
                    if let Ok(mut entry) = archive.by_index(i) {
                        entry.read_to_end(&mut buf).ok();
                    }
                    if let Some(owner) = crate::api::import_mastodon::parse_mastodon_actor(&buf) {
                        return Some(OwnerIdentity {
                            handle: Some(owner.handle),
                            display_name: owner.display_name,
                            profile_url: owner.profile_url,
                        });
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct OwnerIdentity {
    handle: Option<String>,
    display_name: Option<String>,
    profile_url: Option<String>,
}

/// Extract a field value from HTML like: "Username<div><div>xxlovingislivingxx</div></div>"
fn extract_html_field(html: &str, field_name: &str) -> Option<String> {
    let search = format!("{}<div><div>", field_name);
    if let Some(idx) = html.find(&search) {
        let rest = &html[idx + search.len()..];
        if let Some(end) = rest.find("</div>") {
            let value = rest[..end].trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

pub(crate) fn parse_contacts_from_archive(temp_path: &str, source: &str) -> Vec<ContactEntry> {
    let file = match std::fs::File::open(temp_path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return vec![],
    };

    let mut contacts = Vec::new();

    for i in 0..archive.len() {
        let name = {
            let entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
            entry.name().to_string()
        };

        let mut buf = Vec::new();
        let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
        if entry.read_to_end(&mut buf).is_err() { continue; }
        if buf.is_empty() { continue; }

        // File component only: the `followers_and_following/` folder name
        // itself contains "followers", so match the filename, not the path.
        let fname = name.rsplit('/').next().unwrap_or("");

        match source {
            "facebook" => {
                if name.ends_with("your_friends.json") && (name.contains("connections/friends/") || name.contains("friends/")) {
                    contacts.extend(crate::api::import_facebook::parse_facebook_friends_json(&buf));
                } else if name.ends_with("who_you've_followed.json") && name.contains("followers/") {
                    contacts.extend(crate::api::import_facebook::parse_facebook_following_json(&buf));
                } else if name.ends_with("your_friends.html") && (name.contains("connections/friends/") || name.contains("friends/")) {
                    contacts.extend(crate::api::import_facebook::parse_facebook_friends_html(&buf));
                }
            }
            "instagram" => {
                if fname == "following.json" && name.contains("followers_and_following") {
                    contacts.extend(crate::api::import_instagram::parse_instagram_following_json(&buf));
                } else if fname.starts_with("followers") && fname.ends_with(".json") && name.contains("followers_and_following") {
                    // followers.json (older exports), followers_1.json, followers_2.json (shards)
                    contacts.extend(crate::api::import_instagram::parse_instagram_followers_json(&buf));
                } else if (fname == "following.html" || fname == "followers.html") && name.contains("followers_and_following") {
                    contacts.extend(crate::api::import_instagram::parse_follow_html(&buf, "instagram"));
                }
            }
            "threads" => {
                if fname == "following.json" && name.contains("threads") {
                    contacts.extend(crate::api::import_instagram::parse_threads_following_json(&buf));
                } else if fname == "followers.json" && name.contains("threads") {
                    contacts.extend(crate::api::import_instagram::parse_threads_followers_json(&buf));
                } else if (fname == "following.html" || fname == "followers.html") && name.contains("threads") {
                    contacts.extend(crate::api::import_instagram::parse_follow_html(&buf, "threads"));
                }
            }
            "mastodon" => {
                // GDPR archives keep these at the root (or one folder deep).
                if fname == "following.json" {
                    contacts.extend(crate::api::import_mastodon::parse_mastodon_following_json(&buf));
                } else if fname == "followers.json" {
                    contacts.extend(crate::api::import_mastodon::parse_mastodon_followers_json(&buf));
                }
            }
            _ => {}
        }
    }

    contacts
}

/// Adapter: tokio UnboundedSender can be used as a simple sync sender from std threads
pub struct StdSyncSender(tokio::sync::mpsc::UnboundedSender<String>);

impl StdSyncSender {
    pub fn send(&self, msg: String) {
        let _ = self.0.send(msg);
    }
}

/// Wrap a tokio sender for use from sync code
pub fn tokio_to_std_sender(tx: &tokio::sync::mpsc::UnboundedSender<String>) -> StdSyncSender {
    StdSyncSender(tx.clone())
}

// ─── Social Graph API endpoint ──────────────────────────────────────────────

#[derive(Serialize)]
struct SocialGraphResponse {
    stats: SocialGraphStats,
    matches: Vec<SocialMatchEntry>,
    rss_candidates: Vec<RssCandidate>,
    unmatched_with_handles: Vec<UnmatchedContact>,
}

#[derive(Serialize)]
struct SocialGraphStats {
    total_contacts: i64,
    matched_count: i64,
    handle_matched: i64,
    name_matched: i64,
}

#[derive(Serialize)]
struct SocialMatchEntry {
    user_id: String,
    display_name: String,
    handle: String,
    avatar: Option<String>,
    match_type: String,
    confidence: String,
    source: String,
}

#[derive(Serialize)]
struct UnmatchedContact {
    contact_name: String,
    contact_handle: Option<String>,
    source: String,
}

async fn social_graph_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<SocialGraphResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let session_uuid = Uuid::parse_str(&session_id)
        .map_err(|_| AppError::BadRequest("Invalid session ID".into()))?;

    // Verify the session belongs to this user
    let session = db::imports::ImportRow::find_by_id(&state.db, session_uuid)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Import session not found".into()))?;

    if session.user_id != user_id {
        return Err(AppError::Forbidden("This import session does not belong to you".into()));
    }

    // Count total contacts
    let total_contacts = db::imported_contacts::ImportedContactRow::count_by_session_and_user(
        &state.db, session_uuid, user_id,
    ).await.map_err(AppError::Database)?;

    // Get matches
    let matches = db::social_graph_matches::SocialGraphMatchRow::find_by_session_user(
        &state.db, session_uuid, user_id,
    ).await.map_err(AppError::Database)?;

    let mut handle_matched = 0i64;
    let mut name_matched = 0i64;
    let match_entries: Vec<SocialMatchEntry> = matches.iter().map(|m| {
        let handle = format!("@{}", m.username);
        match m.match_type.as_str() {
            "handle_match" => handle_matched += 1,
            "name_match" => name_matched += 1,
            _ => {}
        }
        SocialMatchEntry {
            user_id: m.matched_user_id.to_string(),
            display_name: m.display_name.clone(),
            handle,
            avatar: m.avatar_url.clone(),
            match_type: m.match_type.clone(),
            confidence: m.confidence.clone(),
            source: m.source.clone(),
        }
    }).collect();

    // Get unmatched contacts that have handles
    let unmatched = sqlx::query_as::<_, UnmatchedContactRow>(
        r#"
        SELECT contact_name, contact_handle, source
        FROM imported_contacts
        WHERE user_id = $1 AND import_session_id = $2
          AND contact_handle IS NOT NULL
          AND NOT EXISTS (
            SELECT 1 FROM social_graph_matches
            WHERE (user_a_id = $1 OR user_b_id = $1)
              AND source = imported_contacts.source
          )
        "#,
    )
    .bind(user_id)
    .bind(session_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let unmatched_with_handles: Vec<UnmatchedContact> = unmatched.into_iter().map(|u| {
        UnmatchedContact {
            contact_name: u.contact_name,
            contact_handle: u.contact_handle,
            source: u.source,
        }
    }).collect();

    // Get RSS candidates
    let rss_rows = sqlx::query_as::<_, (String, Option<String>, String)>(
        r#"SELECT contact_name, contact_handle, source
           FROM imported_contacts
           WHERE user_id = $1 AND import_session_id = $2 AND category = 'rss_candidate'"#,
    )
    .bind(user_id)
    .bind(session_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let rss_candidates: Vec<RssCandidate> = rss_rows.into_iter().map(|(name, handle, source)| {
        let feed_url = derive_rss_url(&source, handle.as_deref());
        RssCandidate {
            contact_name: name,
            contact_handle: handle,
            source,
            feed_url,
        }
    }).collect();

    Ok(Json(SocialGraphResponse {
        stats: SocialGraphStats {
            total_contacts,
            matched_count: matches.len() as i64,
            handle_matched,
            name_matched,
        },
        matches: match_entries,
        rss_candidates,
        unmatched_with_handles,
    }))
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct UnmatchedContactRow {
    contact_name: String,
    contact_handle: Option<String>,
    source: String,
}

#[derive(Serialize)]
struct RssCandidate {
    contact_name: String,
    contact_handle: Option<String>,
    source: String,
    feed_url: String,
}

fn derive_rss_url(source: &str, handle: Option<&str>) -> String {
    let h = handle.unwrap_or("");
    match source {
        "reddit" => format!(
            "https://www.reddit.com/user/{}/submitted/.rss",
            h.trim_start_matches('/')
        ),
        "mastodon" => {
            // Handle is typically @user@instance
            let parts: Vec<&str> = h.trim_start_matches('@').split('@').collect();
            if parts.len() == 2 {
                format!("https://{}/users/{}.atom", parts[1], parts[0])
            } else {
                String::new()
            }
        }
        // Instagram/Threads/Facebook don't have public RSS — requires RSS-Bridge integration
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::import_instagram::fixtures;

    /// Spec R3: `following.json` (3 accounts) + `followers.json` (2 accounts)
    /// → 5 `ContactEntry` records from the contact-extraction step.
    #[test]
    fn instagram_fixture_contact_extraction() {
        let fixture = fixtures::instagram_archive();
        let contacts = parse_contacts_from_archive(fixture.path().to_str().unwrap(), "instagram");

        assert_eq!(contacts.len(), 5, "3 following + 2 followers, got {contacts:?}");
        assert!(contacts.iter().all(|c| c.source == "instagram"),
            "all instagram contacts tagged instagram: {contacts:?}");

        // following rows carry name + handle
        assert_eq!(contacts[0].contact_name, "Alice Artist");
        assert_eq!(contacts[0].contact_handle.as_deref(), Some("alice_artist"));
        assert_eq!(contacts[1].contact_name, "Bob Builder");
        assert_eq!(contacts[2].contact_name, "Carol Cooks");

        // follower rows have no name — the handle doubles as the name
        assert_eq!(contacts[3].contact_name, "dave_reads");
        assert_eq!(contacts[3].contact_handle.as_deref(), Some("dave_reads"));
        assert_eq!(contacts[4].contact_name, "erin_writes");
        assert_eq!(contacts[4].contact_handle.as_deref(), Some("erin_writes"));
    }

    #[test]
    fn threads_fixture_contact_extraction() {
        let fixture = fixtures::threads_archive();
        let contacts = parse_contacts_from_archive(fixture.path().to_str().unwrap(), "threads");

        assert_eq!(contacts.len(), 5, "3 following + 2 followers, got {contacts:?}");
        assert!(contacts.iter().all(|c| c.source == "threads"),
            "all threads contacts tagged threads: {contacts:?}");
        assert_eq!(contacts[0].contact_name, "Garden Gal");
        assert_eq!(contacts[0].contact_handle.as_deref(), Some("garden_gal"));
        assert_eq!(contacts[3].contact_name, "Follower One");
    }

    /// Spec R3: HTML equivalents produce contact entries too — and an
    /// Instagram archive's HTML contacts must be tagged `instagram`
    /// (regression: they used to be hardcoded to `threads`).
    #[test]
    fn instagram_html_follow_file_produces_contacts() {
        let fixture = fixtures::instagram_follow_html_archive();
        let contacts = parse_contacts_from_archive(fixture.path().to_str().unwrap(), "instagram");

        assert_eq!(contacts.len(), 2, "2 HTML follow entries, got {contacts:?}");
        assert!(contacts.iter().all(|c| c.source == "instagram"),
            "instagram HTML contacts must be tagged instagram: {contacts:?}");
        assert_eq!(contacts[0].contact_name, "HTML Person One");
        assert_eq!(contacts[0].contact_handle.as_deref(), Some("html_person_one"));
        assert_eq!(contacts[1].contact_name, "HTML Person Two");
    }
}

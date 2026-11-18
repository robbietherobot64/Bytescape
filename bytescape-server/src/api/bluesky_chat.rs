use axum::{
    extract::{Json as AxumJson, Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use crate::error::AppError;
use crate::{auth, db, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/chat/conversations", get(list_conversations))
        .route("/api/chat/conversations", post(start_conversation))
        .route(
            "/api/chat/conversations/{id}/messages",
            get(get_messages).post(send_message),
        )
        .route("/api/chat/sync", post(sync_chat))
}

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<uuid::Uuid, AppError> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    uuid::Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

// === JSON structs ===

#[derive(Serialize)]
struct ConversationView {
    id: String,
    platform: String,
    name: String,
    avatar: Option<String>,
    last_message: Option<String>,
    last_sent_at: Option<String>,
    unread_count: u32,
    is_group: bool,
    member_count: u32,
}

#[derive(Deserialize)]
struct StartConversationReq {
    handle: String,
}

#[derive(Deserialize)]
struct MessagesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize)]
struct ChatMessageView {
    id: String,
    sender_id: String,
    sender_name: String,
    sender_avatar: Option<String>,
    content: String,
    sent_at: String,
    platform: String,
}

#[derive(Deserialize)]
struct SendMessageReq {
    text: String,
}

// === Conversation List ===

async fn list_conversations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<AxumJson<Vec<ConversationView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let accounts = db::accounts::LinkedAccountRow::find_by_provider(
        &state.db,
        user_id,
        "bluesky",
    )
    .await
    .map_err(AppError::Database)?;

    let mut conversations = vec![];

    for account in accounts {
        let client = match build_bluesky_client(&state.db, &account).await {
            Some(c) => c,
            None => continue,
        };

        let convos = match client.list_convos(50, None::<&str>).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("list_convos failed for {}: {}", account.external_id, e);
                continue;
            }
        };

        let viewer_name = account.metadata["display_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&account.external_username);

        if let Some(arr) = convos["convos"].as_array() {
            for c in arr {
                let members = c["members"].as_array().cloned().unwrap_or_default();
                let member_count = members.len() as u32;
                let empty = serde_json::json!({});
                let other = members
                    .iter()
                    .find(|m| m["did"].as_str() != Some(account.external_id.as_str()))
                    .cloned()
                    .unwrap_or_else(|| members.first().cloned().unwrap_or(empty));

                let other_did = other["did"].as_str().unwrap_or("").to_string();

                let transient =
                    serde_json::json!({ "members": c["members"].as_array().cloned().unwrap_or_default() });

                let (dm_name, avatar) =
                    resolve_name(&state.db, "bluesky", &other_did, &account.external_id, viewer_name, &transient)
                        .await;

                // Group chats keep their own identity: the Bluesky group title,
                // or "Group with <members>" — never collapsed into the first
                // other member like a DM.
                let group_title = c["name"].as_str().filter(|s| !s.is_empty());
                let is_group = member_count > 2 || group_title.is_some();
                let name = if is_group {
                    group_title.map(|s| s.to_string()).unwrap_or_else(|| {
                        let names: Vec<String> = members
                            .iter()
                            .filter(|m| m["did"].as_str() != Some(account.external_id.as_str()))
                            .filter_map(|m| {
                                m["displayName"].as_str().filter(|s| !s.is_empty())
                                    .or_else(|| m["handle"].as_str())
                            })
                            .take(3)
                            .map(|s| s.to_string())
                            .collect();
                        if names.is_empty() {
                            "Group chat".to_string()
                        } else {
                            format!("Group with {}", names.join(", "))
                        }
                    })
                } else {
                    dm_name
                };

                let last_msg: Option<String> =
                    c["lastMessage"]["text"].as_str().map(|s| s.to_string());
                let last_sent: Option<String> =
                    c["lastMessage"]["sentAt"].as_str().map(|s| s.to_string());

                conversations.push(ConversationView {
                    id: c["id"].as_str().unwrap_or("").to_string(),
                    platform: "bluesky".to_string(),
                    name,
                    avatar,
                    last_message: last_msg,
                    last_sent_at: last_sent,
                    unread_count: c["unreadCount"].as_u64().unwrap_or(0) as u32,
                    is_group,
                    member_count,
                });
            }
        }
    }

    conversations.sort_by(|a, b| {
        b.last_sent_at
            .as_ref()
            .unwrap_or(&String::new())
            .cmp(a.last_sent_at.as_ref().unwrap_or(&String::new()))
    });

    Ok(AxumJson(conversations))
}

// === Start New Conversation ===

async fn start_conversation(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumJson(body): AxumJson<StartConversationReq>,
) -> Result<AxumJson<ConversationView>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let accounts = db::accounts::LinkedAccountRow::find_by_provider(
        &state.db,
        user_id,
        "bluesky",
    )
    .await
    .map_err(AppError::Database)?;

    for account in accounts {
        let client = match build_bluesky_client(&state.db, &account).await {
            Some(c) => c,
            None => continue,
        };

        let did = match client.resolve_handle(&body.handle).await {
            Ok(d) => d,
            Err(e1) => {
                let with_suffix = format!("{}.bsky.social", body.handle);
                match client.resolve_handle(&with_suffix).await {
                    Ok(d) => d,
                    Err(e2) => {
                        return Err(AppError::Internal(format!(
                            "Failed to resolve handle: {} / {}",
                            e1, e2
                        )));
                    }
                }
            }
        };

        let result = match client
            .get_convo_for_members(&[&account.external_id, &did])
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(AppError::Internal(format!(
                    "Bluesky getConvoForMembers failed: {}",
                    e
                )));
            }
        };

        let convo = &result["convo"];
        let members = convo["members"].as_array().cloned().unwrap_or_default();
        let empty = serde_json::json!({});
        let other = members
            .iter()
            .find(|m| m["did"].as_str() != Some(account.external_id.as_str()))
            .cloned()
            .unwrap_or_else(|| members.first().cloned().unwrap_or(empty));

        let other_did = other["did"].as_str().unwrap_or("").to_string();
        let viewer_name = account.metadata["display_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&account.external_username);

        let transient =
            serde_json::json!({ "members": convo["members"].as_array().cloned().unwrap_or_default() });

        let (name, avatar) =
            resolve_name(&state.db, "bluesky", &other_did, &account.external_id, viewer_name, &transient).await;

        let last_msg: Option<String> =
            convo["lastMessage"]["text"].as_str().map(|s| s.to_string());
        let last_sent: Option<String> =
            convo["lastMessage"]["sentAt"].as_str().map(|s| s.to_string());

        return Ok(AxumJson(ConversationView {
            id: convo["id"].as_str().unwrap_or("").to_string(),
            platform: "bluesky".to_string(),
            name,
            avatar,
            last_message: last_msg,
            last_sent_at: last_sent,
            unread_count: convo["unreadCount"].as_u64().unwrap_or(0) as u32,
            is_group: false,
            member_count: convo["members"].as_array().map(|a| a.len()).unwrap_or(2) as u32,
        }));
    }

    Err(AppError::NotFound(
        "No linked Bluesky account found".into(),
    ))
}

// === Get Messages ===

async fn get_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(convo_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> Result<AxumJson<Vec<ChatMessageView>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let limit = query.limit.unwrap_or(50).min(100);

    let accounts = db::accounts::LinkedAccountRow::find_by_provider(
        &state.db,
        user_id,
        "bluesky",
    )
    .await
    .map_err(AppError::Database)?;

    let account = accounts.into_iter().next().ok_or_else(|| AppError::NotFound(
        "Conversation not found or not linked to Bluesky".into(),
    ))?;

    let client = build_bluesky_client(&state.db, &account)
        .await
        .ok_or_else(|| AppError::Internal("Bluesky client failed".into()))?;

    let msgs = client
        .get_messages(&convo_id, limit as u32, None::<&str>)
        .await
        .map_err(|e| AppError::Internal(format!("Bluesky getMessages failed: {}", e)))?;

    let viewer_name = account.metadata["display_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&account.external_username);

    let raw = msgs["messages"].as_array().cloned().unwrap_or_default();
    let mut views: Vec<ChatMessageView> = Vec::with_capacity(raw.len());

    for msg in raw.iter().rev() {
        let msg_type = msg["$type"].as_str().unwrap_or("");
        if msg_type != "chat.bsky.convo.defs#messageView" {
            continue;
        }

        let sender_did = msg["sender"]["did"].as_str().unwrap_or("");
        let text = msg["text"].as_str().unwrap_or("");
        let sent = msg["sentAt"].as_str().unwrap_or("");
        let msg_id = msg["id"].as_str().unwrap_or("");

        let (sender_name, sender_avatar) = resolve_name(
            &state.db,
            "bluesky",
            sender_did,
            &account.external_id,
            viewer_name,
            &msgs,
        )
        .await;

        views.push(ChatMessageView {
            id: msg_id.to_string(),
            sender_id: sender_did.to_string(),
            sender_name,
            sender_avatar,
            content: text.to_string(),
            sent_at: sent.to_string(),
            platform: "bluesky".to_string(),
        });
    }

    let pid = account.id.to_string();
    for view in &views {
        let sent_at = chrono::DateTime::parse_from_rfc3339(&view.sent_at)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let _ = db::chat::ChatMessageRow::upsert(
            &state.db,
            "bluesky",
            &convo_id,
            Some(&pid),
            &view.id,
            &view.sender_id,
            Some(&view.sender_name),
            view.sender_avatar.as_deref(),
            &view.content,
            sent_at,
        )
        .await;
    }

    Ok(AxumJson(views))
}

// === Send Message ===

async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(convo_id): Path<String>,
    AxumJson(body): AxumJson<SendMessageReq>,
) -> Result<AxumJson<ChatMessageView>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let accounts = db::accounts::LinkedAccountRow::find_by_provider(
        &state.db,
        user_id,
        "bluesky",
    )
    .await
    .map_err(AppError::Database)?;

    let account = accounts.into_iter().next().ok_or_else(|| AppError::NotFound(
        "No linked Bluesky account found".into(),
    ))?;

    let client = build_bluesky_client(&state.db, &account)
        .await
        .ok_or_else(|| AppError::Internal("Bluesky client failed".into()))?;

    let result: Value = client
        .send_message(&convo_id, &body.text)
        .await
        .map_err(|e| AppError::Internal(format!("Bluesky sendMessage failed: {}", e)))?;

    let msg = &result;
    let sender_did = msg["sender"]["did"]
        .as_str()
        .unwrap_or(&account.external_id);
    let sent = msg["sentAt"].as_str().unwrap_or("");
    let msg_id = msg["id"].as_str().unwrap_or("");

    let viewer_name = account.metadata["display_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&account.external_username);

    let sent_at = chrono::DateTime::parse_from_rfc3339(sent)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    let pid = account.id.to_string();
    let _ = db::chat::ChatMessageRow::upsert(
        &state.db,
        "bluesky",
        &convo_id,
        Some(&pid),
        msg_id,
        sender_did,
        Some(viewer_name),
        None,
        &body.text,
        sent_at,
    )
    .await;

    Ok(AxumJson(ChatMessageView {
        id: msg_id.to_string(),
        sender_id: sender_did.to_string(),
        sender_name: viewer_name.to_string(),
        sender_avatar: None,
        content: body.text.clone(),
        sent_at: sent.to_string(),
        platform: "bluesky".to_string(),
    }))
}

// === Sync (HTTP-triggered) ===

async fn sync_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<AxumJson<Value>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let accounts = db::accounts::LinkedAccountRow::find_by_provider(
        &state.db,
        user_id,
        "bluesky",
    )
    .await
    .map_err(AppError::Database)?;

    for account in accounts {
        let client = match build_bluesky_client(&state.db, &account).await {
            Some(c) => c,
            None => continue,
        };

        let log: Value = match client.get_log(None::<&str>).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("sync_chat getLog failed for {}: {}", account.external_id, e);
                continue;
            }
        };

        let pid = account.id.to_string();
        if let Some(logs) = log["logs"].as_array() {
            for entry in logs {
                if let Some(msg) = entry["message"].as_object() {
                    let msg_type = msg["$type"].as_str().unwrap_or("");
                    if msg_type == "chat.bsky.convo.defs#messageView" {
                        let convo_id = entry["convoId"].as_str().unwrap_or("").to_string();
                        let msg_id = msg["id"].as_str().unwrap_or("").to_string();
                        let sender = msg["sender"]["did"].as_str().unwrap_or("").to_string();
                        let text = msg["text"].as_str().unwrap_or("").to_string();
                        let sent = msg["sentAt"].as_str().unwrap_or("");
                        let ts = chrono::DateTime::parse_from_rfc3339(sent)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now());

                        let _ = db::chat::ChatMessageRow::upsert(
                            &state.db,
                            "bluesky",
                            &convo_id,
                            Some(&pid),
                            &msg_id,
                            &sender,
                            None,
                            None,
                            &text,
                            ts,
                        )
                        .await;
                    }
                }
            }
        }
    }

    Ok(AxumJson(serde_json::json!({"synced": true})))
}

// === Background Sync Loop ===

async fn process_log_entries(
    pool: &crate::db::DbPool,
    _account_id: Uuid,
    log: &Value,
    pid: &str,
) -> Option<String> {
    if let Some(logs) = log["logs"].as_array() {
        for entry in logs {
            let msg = match entry["message"].as_object() {
                Some(m) => m,
                None => continue,
            };
            let msg_type = msg["$type"].as_str().unwrap_or("");
            if msg_type != "chat.bsky.convo.defs#messageView" {
                continue;
            }
            let convo_id = entry["convoId"].as_str().unwrap_or("").to_string();
            let msg_id = msg["id"].as_str().unwrap_or("").to_string();
            let sender = msg["sender"]["did"].as_str().unwrap_or("").to_string();
            let text = msg["text"].as_str().unwrap_or("").to_string();
            let sent = msg["sentAt"].as_str().unwrap_or("");
            let ts = chrono::DateTime::parse_from_rfc3339(sent)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let _ = db::chat::ChatMessageRow::upsert(
                pool, "bluesky", &convo_id, Some(pid), &msg_id, &sender,
                None, None, &text, ts,
            )
            .await;
        }
    }
    log["cursor"].as_str().map(|s| s.to_string())
}

pub fn start_background_sync(state: AppState) {
    tokio::spawn(async move {
        let mut cursors: HashMap<Uuid, String> = HashMap::new();
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            let accounts = match db::accounts::LinkedAccountRow::find_all_by_provider(
                &state.db, "bluesky",
            )
            .await
            {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("background_sync: find_all_by_provider error: {}", e);
                    continue;
                }
            };

            for account in &accounts {
                let cursor = cursors.get(&account.id).cloned();

                let client = match build_bluesky_client(&state.db, account).await {
                    Some(c) => c,
                    None => continue,
                };

                let log: Value = match client.get_log(cursor.as_deref()).await {
                    Ok(l) => l,
                    Err(e) => {
                        let err_str = e.to_string();
                        // Treat ExpiredToken and 401 AuthenticationRequired as
                        // expired-session conditions that may be recoverable via refresh.
                        let needs_refresh = err_str.contains("ExpiredToken")
                            || err_str.contains("AuthenticationRequired")
                            || err_str.contains("unknown auth scheme");

                        if needs_refresh && account.refresh_token.is_some() {
                            tracing::info!(
                                "background_sync: refreshing token for account {}",
                                account.external_id
                            );
                            match crate::oauth::bluesky::refresh_session(
                                &reqwest::Client::new(),
                                &state.db,
                                account,
                            )
                            .await
                            {
                                Ok(_) => {
                                    let client2 = match build_bluesky_client(&state.db, account).await {
                                        Some(c) => c,
                                        None => continue,
                                    };
                                    match client2.get_log(cursor.as_deref()).await {
                                        Ok(l2) => {
                                            let pid = account.id.to_string();
                                            if let Some(new_cursor) =
                                                process_log_entries(&state.db, account.id, &l2, &pid).await
                                            {
                                                cursors.insert(account.id, new_cursor);
                                            }
                                        }
                                        Err(e2) => {
                                            tracing::debug!(
                                                "background_sync: retry still failed for {}: {}",
                                                account.external_id, e2
                                            );
                                        }
                                    }
                                }
                                Err(refresh_err) => {
                                    tracing::debug!(
                                        "background_sync: refresh failed for {}: {}",
                                        account.external_id, refresh_err
                                    );
                                }
                            }
                        } else if needs_refresh {
                            tracing::debug!(
                                "background_sync: auth expired for {} and no refresh_token",
                                account.external_id
                            );
                        } else {
                            tracing::debug!(
                                "background_sync: get_log failed for {}: {}",
                                account.external_id, e
                            );
                        }
                        continue;
                    }
                };

                let pid = account.id.to_string();
                if let Some(new_cursor) =
                    process_log_entries(&state.db, account.id, &log, &pid).await
                {
                    cursors.insert(account.id, new_cursor);
                }
            }
        }
    });
}

// === Name Resolution Helpers ===

async fn resolve_name(
    pool: &crate::db::DbPool,
    platform: &str,
    did: &str,
    viewer_did: &str,
    viewer_name: &str,
    transient: &Value,
) -> (String, Option<String>) {
    if did == viewer_did {
        return (viewer_name.to_string(), None);
    }

    if let Ok(Some((name, _handle, avatar))) =
        db::chat_profiles::ChatProfileRow::find(pool, platform, did).await
    {
        return (name, avatar);
    }

    if let Some(p) = find_in_json(transient, did) {
        let name = p["displayName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| p["handle"].as_str())
            .unwrap_or(did)
            .to_string();
        let avatar = p["avatar"].as_str().map(|s| s.to_string());
        let _ = db::chat_profiles::ChatProfileRow::upsert(
            pool,
            platform,
            did,
            p["displayName"].as_str(),
            p["handle"].as_str(),
            p["avatar"].as_str(),
        )
        .await;
        return (name, avatar);
    }

    (did.to_string(), None)
}

fn find_in_json<'a>(root: &'a Value, target_did: &str) -> Option<&'a Value> {
    for key in &["relatedProfiles", "members"] {
        if let Some(arr) = root[key].as_array() {
            for item in arr {
                if item["did"].as_str() == Some(target_did) {
                    return Some(item);
                }
            }
        }
    }
    if let Some(arr) = root["convo"]["members"].as_array() {
        for item in arr {
            if item["did"].as_str() == Some(target_did) {
                return Some(item);
            }
        }
    }
    None
}

// === Bluesky Client Builder ===

async fn build_bluesky_client(
    db: &crate::db::DbPool,
    account: &db::accounts::LinkedAccountRow,
) -> Option<crate::oauth::bluesky::BlueskyApiClient> {
    let pds_url = crate::oauth::bluesky::get_pds_url(&account.metadata);
    // Refresh-on-demand: the stored access token lives ~35 minutes; chat calls
    // must not present a stale one. fresh_token persists rotated tokens.
    let token = crate::oauth::bluesky::fresh_token(db, account).await.ok()?;

    let client = if let Some(dpop_hex) = &account.dpop_private_key {
        if !dpop_hex.is_empty() {
            let dpop_seed = hex::decode(dpop_hex).ok()?;
            crate::oauth::bluesky::BlueskyApiClient::new(
                &token,
                &dpop_seed,
                &pds_url,
            )
            .ok()?
        } else {
            crate::oauth::bluesky::BlueskyApiClient::new_bearer(
                &token,
                &pds_url,
            )
        }
    } else {
        crate::oauth::bluesky::BlueskyApiClient::new_bearer(&token, &pds_url)
    };

    Some(client)
}

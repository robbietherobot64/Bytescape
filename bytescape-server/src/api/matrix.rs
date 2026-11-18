use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{AppState, auth};
use crate::db::matrix;
use crate::error::AppError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{api_version}/login", post(login))
        .route("/{api_version}/account/whoami", get(whoami))
        .route("/{api_version}/createRoom", post(create_room))
        .route("/{api_version}/rooms/{room_id}/join", post(join_room))
        .route("/{api_version}/rooms/{room_id}/invite", post(invite))
        .route("/{api_version}/rooms/{room_id}/messages", get(room_messages))
        .route("/{api_version}/rooms/{room_id}/send/{event_type}/{txn_id}", put(send_message))
        .route("/{api_version}/sync", get(sync))
}

// === Well-Known ===

pub async fn matrix_well_known(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "m.homeserver": {
            "base_url": format!("https://{}", state.config.domain)
        },
        "m.identity_server": {
            "base_url": format!("https://{}", state.config.domain)
        }
    }))
}

// === Auth / Identity ===

#[derive(Deserialize)]
struct LoginRequest {
    #[serde(rename = "type")]
    login_type: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct LoginResponse {
    user_id: String,
    access_token: String,
    device_id: String,
    home_server: String,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    if body.login_type != "org.matrix.login.jwt" {
        return Err(AppError::BadRequest("Only org.matrix.login.jwt is supported".into()));
    }
    let token = body.token.ok_or_else(|| AppError::Auth("Missing token".into()))?;
    
    // Validate our existing JWT
    let claims = auth::validate_token(&token, &state.config.jwt_secret)
        .map_err(|_| AppError::Auth("Invalid token".into()))?;
    
    let user_id = format!("@{}:{}", claims.sub, state.config.domain);
    
    Ok(Json(LoginResponse {
        user_id: user_id.clone(),
        access_token: token, // Re-use the Bytescape JWT as the Matrix access token
        device_id: "bytescape_web".to_string(),
        home_server: state.config.domain.clone(),
    }))
}

#[derive(Serialize)]
struct WhoamiResponse {
    user_id: String,
}

async fn whoami(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<WhoamiResponse>, AppError> {
    let user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    Ok(Json(WhoamiResponse { user_id }))
}

// === Room Management ===

#[derive(Deserialize)]
struct CreateRoomRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    invite: Vec<String>,
}

#[derive(Serialize)]
struct CreateRoomResponse {
    room_id: String,
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateRoomRequest>,
) -> Result<Json<CreateRoomResponse>, AppError> {
    let user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    let room_id = format!("!{}:{}", uuid::Uuid::new_v4(), state.config.domain);
    
    matrix::create_room(&state.db, &room_id, &user_id).await?;
    
    if let Some(name) = &body.name {
        matrix::create_event(&state.db, &room_id, &format!("${}", uuid::Uuid::new_v4()), 
            &user_id, "m.room.name", Some(""), &serde_json::json!({"name": name})).await?;
    }

    for invited in &body.invite {
        matrix::join_room(&state.db, &room_id, invited).await?;
    }
    
    Ok(Json(CreateRoomResponse { room_id }))
}

async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    matrix::join_room(&state.db, &room_id, &user_id).await?;
    Ok(Json(serde_json::json!({"room_id": room_id})))
}

#[derive(Deserialize)]
struct InviteRequest {
    user_id: String,
}

async fn invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<InviteRequest>,
) -> Result<Json<Value>, AppError> {
    let _user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    matrix::join_room(&state.db, &room_id, &body.user_id).await?; // In Mini-Matrix, invite = auto-join
    Ok(Json(serde_json::json!({})))
}

// === Messages ===

#[derive(Deserialize)]
struct RoomMessagesQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    from: i64,
    #[serde(default = "default_dir")]
    dir: String,
}
fn default_limit() -> i64 { 20 }
fn default_dir() -> String { "b".to_string() }

#[derive(Serialize)]
struct RoomMessagesResponse {
    start: String,
    end: String,
    chunk: Vec<MatrixEventPayload>,
}

#[derive(Serialize)]
struct MatrixEventPayload {
    event_id: String,
    sender: String,
    #[serde(rename = "type")]
    event_type: String,
    content: Value,
    origin_server_ts: i64,
}

async fn room_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Query(query): Query<RoomMessagesQuery>,
) -> Result<Json<RoomMessagesResponse>, AppError> {
    let _user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    
    let events = matrix::get_room_messages(&state.db, &room_id, query.from, query.limit, &query.dir).await?;
    let chunk: Vec<MatrixEventPayload> = events.into_iter().map(|e| MatrixEventPayload {
        event_id: e.event_id,
        sender: e.sender,
        event_type: e.event_type,
        content: e.content,
        origin_server_ts: e.origin_server_ts,
    }).collect();

    Ok(Json(RoomMessagesResponse {
        start: query.from.to_string(),
        end: chunk.last().map(|e| e.origin_server_ts.to_string()).unwrap_or_default(),
        chunk,
    }))
}

async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((room_id, event_type, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    let event_id = format!("${}", uuid::Uuid::new_v4());
    
    matrix::create_event(&state.db, &room_id, &event_id, &user_id, &event_type, None, &body).await?;
    Ok(Json(serde_json::json!({"event_id": event_id})))
}

// === Sync (The Heart) ===

#[derive(Deserialize)]
struct SyncQuery {
    #[serde(default)]
    since: Option<String>,
    #[serde(default = "default_timeout")]
    timeout: u64,
}
fn default_timeout() -> u64 { 30000 }

#[derive(Serialize, Default)]
struct SyncResponse {
    next_batch: String,
    rooms: RoomsSync,
}

#[derive(Serialize, Default)]
struct RoomsSync {
    join: HashMap<String, JoinedRoom>,
}

#[derive(Serialize, Default)]
struct JoinedRoom {
    timeline: Timeline,
    state: StateBlock,
}

#[derive(Serialize, Default)]
struct Timeline {
    events: Vec<MatrixEventPayload>,
    #[serde(rename = "prev_batch")]
    prev_batch: String,
    limited: bool,
}

#[derive(Serialize, Default)]
struct StateBlock {
    events: Vec<MatrixEventPayload>,
}

async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SyncQuery>,
) -> Result<Json<SyncResponse>, AppError> {
    let user_id = extract_matrix_user(&headers, &state.config.jwt_secret, &state.config.domain)?;
    
    // since token is a nanosecond timestamp string
    let since_ts: i64 = query.since.and_then(|s| s.parse().ok()).unwrap_or(0);
    
    let rooms = matrix::get_user_rooms(&state.db, &user_id).await?;
    let mut joined = HashMap::new();
    
    for room_id in rooms {
        let events = matrix::get_room_events_since(&state.db, &room_id, since_ts, 100).await?;
        let timeline_events: Vec<MatrixEventPayload> = events.iter().cloned().map(|e| MatrixEventPayload {
            event_id: e.event_id,
            sender: e.sender,
            event_type: e.event_type,
            content: e.content,
            origin_server_ts: e.origin_server_ts,
        }).collect();

        joined.insert(room_id.clone(), JoinedRoom {
            timeline: Timeline {
                events: timeline_events,
                prev_batch: since_ts.to_string(),
                limited: false,
            },
            state: StateBlock::default(),
        });
    }
    
    let now = chrono::Utc::now().timestamp_millis();
    let next_batch = now.to_string();
    
    Ok(Json(SyncResponse {
        next_batch,
        rooms: RoomsSync { join: joined },
    }))
}

// === Helpers ===

fn extract_matrix_user(headers: &HeaderMap, jwt_secret: &str, domain: &str) -> Result<String, AppError> {
    let auth = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Auth("Missing Authorization header".into()))?;
    
    let token = auth.strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Auth("Invalid Authorization header format".into()))?;
    
    let claims = auth::validate_token(token, jwt_secret)
        .map_err(|_| AppError::Auth("Invalid token".into()))?;
    
    Ok(format!("@{}:{}", claims.sub, domain))
}

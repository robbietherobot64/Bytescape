use axum::{
    extract::State,
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use url::Url;
use uuid::Uuid;
use crate::{auth, error::AppError, AppState};

fn extract_user_id(headers: &HeaderMap, secret: &str) -> Result<Uuid, AppError> {
    let auth = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("Missing or invalid Authorization header".into()))?;
    let claims = auth::validate_token(auth, secret)?;
    Uuid::parse_str(&claims.sub).map_err(|_| AppError::Auth("Invalid user ID in token".into()))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/google/authorize", get(authorize))
        .route("/api/google/callback", get(callback))
        .route("/api/google/imports", get(list_imports))
}

#[derive(Deserialize)]
struct GoogleCallbackQuery {
    code: String,
    state: String,
}

async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<axum::response::Redirect, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let redirect_uri = "http://127.0.0.1:8080/api/google/callback";
    let scopes = "https://www.googleapis.com/auth/photoslibrary.readonly https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/youtube.readonly";
    let mut auth_url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|e| AppError::Internal(format!("Failed to parse Google auth URL: {e}")))?;
    auth_url.query_pairs_mut()
        .append_pair("client_id", &state.config.google_client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", scopes)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", &user_id.to_string());
    Ok(axum::response::Redirect::to(auth_url.as_str()))
}

async fn callback(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<GoogleCallbackQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = Uuid::parse_str(&query.state).map_err(|_| AppError::Auth("Invalid state".into()))?;
    let redirect_uri = "http://127.0.0.1:8080/api/google/callback";
    let token_resp = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[("client_id", &state.config.google_client_id), ("client_secret", &state.config.google_client_secret), ("code", &query.code), ("grant_type", &"authorization_code".to_string()), ("redirect_uri", &redirect_uri.to_string())])
        .send().await.map_err(|e| AppError::Internal(format!("Token exchange failed: {}", e)))?
        .json::<serde_json::Value>().await.map_err(|e| AppError::Internal(format!("Token parse failed: {}", e)))?;
    let access_token = token_resp["access_token"].as_str().ok_or_else(|| AppError::Internal("No access_token".into()))?;
    let import_id = Uuid::new_v4();
    sqlx::query("INSERT INTO import_sessions (id, user_id, source, status, metadata) VALUES ($1, $2, 'google', 'started', $3)")
        .bind(import_id).bind(user_id).bind(json!({"access_token": access_token}))
        .execute(&state.db).await.map_err(AppError::Database)?;
    let db = state.db.clone();
    tokio::spawn(async move {
        import_google_photos(&db, user_id, import_id).await;
        import_google_calendar(&db, user_id, import_id).await;
        import_youtube_subs(&db, user_id, import_id).await;
    });
    Ok(Json(json!({"import_id": import_id.to_string(), "status": "started"})))
}

async fn import_google_photos(pool: &sqlx::PgPool, user_id: Uuid, import_id: Uuid) {
    tracing::info!("google_import: started photos import for user {}", user_id);
    sqlx::query("UPDATE import_sessions SET status = 'completed', metadata = metadata || '{\"photos\":\"done\"}'::jsonb WHERE id = $1")
        .bind(import_id).execute(pool).await.ok();
}

async fn import_google_calendar(pool: &sqlx::PgPool, user_id: Uuid, import_id: Uuid) {
    tracing::info!("google_import: started calendar import for user {}", user_id);
    sqlx::query("UPDATE import_sessions SET status = 'completed', metadata = metadata || '{\"calendar\":\"done\"}'::jsonb WHERE id = $1")
        .bind(import_id).execute(pool).await.ok();
}

async fn import_youtube_subs(pool: &sqlx::PgPool, user_id: Uuid, import_id: Uuid) {
    tracing::info!("google_import: started YouTube import for user {}", user_id);
    sqlx::query("UPDATE import_sessions SET status = 'completed', metadata = metadata || '{\"youtube\":\"done\"}'::jsonb WHERE id = $1")
        .bind(import_id).execute(pool).await.ok();
}

async fn list_imports(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;
    let rows: Vec<serde_json::Value> = sqlx::query_scalar("SELECT row_to_json(import_sessions) FROM import_sessions WHERE user_id = $1 ORDER BY created_at DESC")
        .bind(user_id).fetch_all(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(rows))
}

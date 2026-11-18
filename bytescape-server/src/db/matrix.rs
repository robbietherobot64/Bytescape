//! Matrix-specific database operations.

use sqlx::{PgPool, Row};
use serde_json::Value;

pub async fn create_room(pool: &PgPool, room_id: &str, creator_id: &str) -> Result<(), sqlx::Error> {
    // Insert the room creation event
    let content = serde_json::json!({"creator": creator_id});
    create_event(pool, room_id, &format!("${}", uuid::Uuid::new_v4()), creator_id, "m.room.create", Some(""), &content).await?;
    // Auto-join creator
    join_room(pool, room_id, creator_id).await?;
    Ok(())
}

pub async fn join_room(pool: &PgPool, room_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO matrix_room_members (room_id, user_id, membership) VALUES ($1, $2, 'join')
         ON CONFLICT (room_id, user_id) DO UPDATE SET membership = 'join'"
    )
    .bind(room_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_event(
    pool: &PgPool,
    room_id: &str,
    event_id: &str,
    sender: &str,
    event_type: &str,
    state_key: Option<&str>,
    content: &Value,
) -> Result<uuid::Uuid, sqlx::Error> {
    let row = sqlx::query(
        "INSERT INTO matrix_events (room_id, event_id, sender, event_type, state_key, content) 
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
    )
    .bind(room_id)
    .bind(event_id)
    .bind(sender)
    .bind(event_type)
    .bind(state_key)
    .bind(content)
    .fetch_one(pool)
    .await?;
    Ok(row.get(0))
}

pub async fn get_room_events_since(
    pool: &PgPool,
    room_id: &str,
    since_ts: i64,
    limit: i64,
) -> Result<Vec<MatrixEventRow>, sqlx::Error> {
    sqlx::query_as::<_, MatrixEventRow>(
        "SELECT * FROM matrix_events 
         WHERE room_id = $1 AND origin_server_ts > $2 
         ORDER BY origin_server_ts ASC LIMIT $3"
    )
    .bind(room_id)
    .bind(since_ts)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn get_room_messages(
    pool: &PgPool,
    room_id: &str,
    from_ts: i64,
    limit: i64,
    dir: &str,
) -> Result<Vec<MatrixEventRow>, sqlx::Error> {
    let query = if dir == "b" {
        "SELECT * FROM matrix_events WHERE room_id = $1 AND origin_server_ts < $2 
         ORDER BY origin_server_ts DESC LIMIT $3"
    } else {
        "SELECT * FROM matrix_events WHERE room_id = $1 AND origin_server_ts > $2 
         ORDER BY origin_server_ts ASC LIMIT $3"
    };
    sqlx::query_as::<_, MatrixEventRow>(query)
        .bind(room_id)
        .bind(from_ts)
        .bind(limit)
        .fetch_all(pool)
        .await
}

pub async fn get_user_rooms(pool: &PgPool, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT room_id FROM matrix_room_members WHERE user_id = $1 AND membership = 'join'")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct MatrixEventRow {
    pub id: uuid::Uuid,
    pub room_id: String,
    pub event_id: String,
    pub sender: String,
    pub event_type: String,
    pub state_key: Option<String>,
    pub content: Value,
    pub origin_server_ts: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

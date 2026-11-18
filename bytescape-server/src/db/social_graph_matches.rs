//! Social graph matches — confirmed two-way connections.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SocialGraphMatchRow {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_b_id: Uuid,
    pub match_type: String,
    pub source: String,
    pub confidence: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub notified: bool,
}

impl SocialGraphMatchRow {
    pub async fn create(
        pool: &PgPool,
        user_a_id: Uuid,
        user_b_id: Uuid,
        match_type: &str,
        source: &str,
        confidence: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO social_graph_matches (user_a_id, user_b_id, match_type, source, confidence)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_a_id, user_b_id) DO NOTHING
            RETURNING *
            "#,
        )
        .bind(user_a_id)
        .bind(user_b_id)
        .bind(match_type)
        .bind(source)
        .bind(confidence)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_session_user(
        pool: &PgPool,
        _session_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<MatchWithUser>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT DISTINCT ON (u.id) m.id, m.user_a_id, m.user_b_id, m.match_type, m.source, m.confidence,
                   m.created_at, m.notified,
                   u.id as matched_user_id, u.display_name, u.username, u.avatar_url
            FROM social_graph_matches m
            JOIN users u ON u.id = CASE WHEN m.user_a_id = $1 THEN m.user_b_id ELSE m.user_a_id END
            WHERE (m.user_a_id = $1 OR m.user_b_id = $1)
            ORDER BY u.id, m.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_matches_for_user(
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<SocialGraphMatchRow>, sqlx::Error> {
        sqlx::query_as(
            "SELECT * FROM social_graph_matches WHERE user_a_id = $1 OR user_b_id = $1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MatchWithUser {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_b_id: Uuid,
    pub match_type: String,
    pub source: String,
    pub confidence: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub notified: bool,
    pub matched_user_id: Uuid,
    pub display_name: String,
    pub username: String,
    pub avatar_url: Option<String>,
}

use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Normalized cached post from an external service (Bluesky, Mastodon, etc.)
#[derive(Debug)]
pub struct CachedPost {
    pub id: Uuid,
    pub source: String,
    pub external_id: String,
    pub author: String,
    pub author_handle: Option<String>,
    pub avatar: Option<String>,
    pub content: String,
    pub html_content: Option<String>,
    pub media_urls: Vec<String>,
    pub source_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
}

/// Row to insert into cached_posts
#[derive(Debug, Clone)]
pub struct CachedPostInsert {
    pub external_id: String,
    pub author: String,
    pub author_handle: Option<String>,
    pub avatar: Option<String>,
    pub content: String,
    pub html_content: Option<String>,
    pub media_urls: Vec<String>,
    pub source_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl CachedPost {
/// List cached posts for a user, ordered by created_at DESC, with pagination
pub async fn list(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<CachedPost>, sqlx::Error> {
    let query_str = if before.is_some() {
        r#"
        SELECT id, source, external_id, author, author_handle, avatar,
               content, html_content, media_urls, source_url, created_at, fetched_at
        FROM cached_posts
        WHERE user_id = $1 AND created_at < $2
        ORDER BY created_at DESC
        LIMIT $3
        "#
    } else {
        r#"
        SELECT id, source, external_id, author, author_handle, avatar,
               content, html_content, media_urls, source_url, created_at, fetched_at
        FROM cached_posts
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#
    };

    let rows = if let Some(before_time) = before {
        sqlx::query(query_str)
            .bind(user_id)
            .bind(before_time)
            .bind(limit)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(query_str)
            .bind(user_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
    };

    let mut posts = Vec::with_capacity(rows.len());
    for r in rows {
        let media_arr: Vec<Option<String>> = r.try_get("media_urls").unwrap_or_default();
        posts.push(CachedPost {
            id: r.try_get("id").unwrap_or_default(),
            source: r.try_get("source").unwrap_or_default(),
            external_id: r.try_get("external_id").unwrap_or_default(),
            author: r.try_get("author").unwrap_or_default(),
            author_handle: r.try_get("author_handle").ok(),
            avatar: r.try_get("avatar").ok(),
            content: r.try_get("content").unwrap_or_default(),
            html_content: r.try_get("html_content").ok(),
            media_urls: media_arr.into_iter().flatten().collect(),
            source_url: r.try_get("source_url").ok(),
            created_at: r.try_get("created_at").unwrap_or_default(),
            fetched_at: r.try_get("fetched_at").unwrap_or_default(),
        });
    }

    Ok(posts)
}
}

/// Insert or update a single cached post
async fn upsert_one(
    pool: &PgPool,
    user_id: Uuid,
    source: &str,
    post: &CachedPostInsert,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO cached_posts
            (user_id, source, external_id, author, author_handle, avatar,
             content, html_content, media_urls, source_url, created_at, fetched_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW())
        ON CONFLICT (user_id, source, external_id)
        DO UPDATE SET
            author = EXCLUDED.author,
            author_handle = EXCLUDED.author_handle,
            avatar = EXCLUDED.avatar,
            content = EXCLUDED.content,
            html_content = EXCLUDED.html_content,
            media_urls = EXCLUDED.media_urls,
            source_url = EXCLUDED.source_url,
            created_at = EXCLUDED.created_at,
            fetched_at = NOW()
        "#
    )
    .bind(user_id)
    .bind(source)
    .bind(&post.external_id)
    .bind(&post.author)
    .bind(&post.author_handle)
    .bind(&post.avatar)
    .bind(&post.content)
    .bind(&post.html_content)
    .bind(&post.media_urls)
    .bind(&post.source_url)
    .bind(post.created_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Bulk upsert — loops over posts individually. Good for batches of 20-50.
pub async fn bulk_upsert(
    pool: &PgPool,
    user_id: Uuid,
    source: &str,
    posts: &[CachedPostInsert],
) -> Result<(), sqlx::Error> {
    if posts.is_empty() {
        return Ok(());
    }
    for post in posts {
        upsert_one(pool, user_id, source, post).await?;
    }
    Ok(())
}

impl CachedPost {
/// Delete a specific cached post belonging to a user
pub async fn delete_by_id(
    pool: &PgPool,
    id: Uuid,
    user_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM cached_posts WHERE id = $1 AND user_id = $2"
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Delete cached posts older than `days` for a user
pub async fn delete_old(
    pool: &PgPool,
    user_id: Uuid,
    days: i64,
) -> Result<u64, sqlx::Error> {
    let query = format!(
        "DELETE FROM cached_posts WHERE user_id = $1 AND fetched_at < NOW() - INTERVAL '${} days'",
        days
    );
    let result = sqlx::query(&query).bind(user_id).execute(pool).await?;
    Ok(result.rows_affected())
}

/// Count cached posts for a user
pub async fn count(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM cached_posts WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}
}

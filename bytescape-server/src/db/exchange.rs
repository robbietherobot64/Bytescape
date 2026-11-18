use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ListingRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub description: String,
    pub listing_type: String,
    pub category: String,
    pub price_cents: Option<i64>,
    pub currency: Option<String>,
    pub location: Option<String>,
    pub tags: Vec<String>,
    pub image_urls: Vec<String>,
    pub status: String,
    pub view_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct MessageRow {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub content: String,
    pub read: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ListingRow {
    pub async fn create(
        pool: &PgPool,
        user_id: Uuid,
        title: &str,
        description: &str,
        listing_type: &str,
        category: &str,
        price_cents: Option<i64>,
        currency: Option<&str>,
        location: Option<&str>,
        tags: &[String],
        image_urls: &[String],
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO exchange_listings
                (user_id, title, description, listing_type, category,
                 price_cents, currency, location, tags, image_urls)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(title)
        .bind(description)
        .bind(listing_type)
        .bind(category)
        .bind(price_cents)
        .bind(currency)
        .bind(location)
        .bind(tags)
        .bind(image_urls)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM exchange_listings WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn list(
        pool: &PgPool,
        category: Option<&str>,
        listing_type: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let mut conditions = vec!["status = 'active'".to_string()];
        let mut param_idx = 1;

        if let Some(_cat) = category {
            conditions.push(format!("category = ${}", param_idx));
            param_idx += 1;
        }
        if let Some(lt) = listing_type
            && lt != "all" {
                conditions.push(format!("listing_type = ${}", param_idx));
                param_idx += 1;
            }

        let where_clause = conditions.join(" AND ");
        let query = format!(
            "SELECT * FROM exchange_listings WHERE {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            where_clause, param_idx, param_idx + 1,
        );

        let mut q = sqlx::query_as::<_, Self>(&query);
        if let Some(cat) = category {
            q = q.bind(cat);
        }
        if let Some(lt) = listing_type
            && lt != "all" {
                q = q.bind(lt);
            }
        q = q.bind(limit).bind(offset);
        q.fetch_all(pool).await
    }

    pub async fn list_by_user(
        pool: &PgPool,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM exchange_listings
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn update_status(
        pool: &PgPool,
        id: Uuid,
        user_id: Uuid,
        status: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE exchange_listings
            SET status = $1, updated_at = NOW()
            WHERE id = $2 AND user_id = $3
            RETURNING *
            "#,
        )
        .bind(status)
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn delete(
        pool: &PgPool,
        id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            DELETE FROM exchange_listings
            WHERE id = $1 AND user_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn increment_view_count(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE exchange_listings SET view_count = view_count + 1 WHERE id = $1",
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }
}

impl MessageRow {
    pub async fn create(
        pool: &PgPool,
        listing_id: Uuid,
        sender_id: Uuid,
        recipient_id: Uuid,
        content: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as(
            r#"
            INSERT INTO exchange_messages (listing_id, sender_id, recipient_id, content)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(listing_id)
        .bind(sender_id)
        .bind(recipient_id)
        .bind(content)
        .fetch_one(pool)
        .await
    }

    pub async fn list_for_listing(
        pool: &PgPool,
        listing_id: Uuid,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM exchange_messages
            WHERE listing_id = $1 AND (sender_id = $2 OR recipient_id = $2)
            ORDER BY created_at ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(listing_id)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn list_inbox(
        pool: &PgPool,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT * FROM exchange_messages
            WHERE recipient_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    pub async fn mark_read(pool: &PgPool, id: Uuid, recipient_id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(
            r#"
            UPDATE exchange_messages SET read = true
            WHERE id = $1 AND recipient_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(recipient_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn inbox_count(pool: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM exchange_messages WHERE recipient_id = $1 AND read = false",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
    }
}

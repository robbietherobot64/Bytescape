use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;

pub fn start_rss_sync(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            if let Err(e) = poll_all_rss_feeds(&state.db).await {
                tracing::warn!("RSS sync error: {}", e);
            }
        }
    });
}

async fn poll_all_rss_feeds(pool: &PgPool) -> Result<(), String> {
    let rows = sqlx::query_as::<_, RssSubscription>(
        r#"
        SELECT id, user_id, source_id AS url
        FROM subscriptions
        WHERE source = 'rss' AND enabled = true
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("DB query failed: {}", e))?;

    for sub in &rows {
        if let Err(e) = sync_one_feed(pool, sub.id, sub.user_id, &sub.url).await {
            tracing::warn!("RSS sync failed for sub {} ({}): {}", sub.id, sub.url, e);
            sqlx::query("UPDATE subscriptions SET fetch_error = $1 WHERE id = $2")
                .bind(&e)
                .bind(sub.id)
                .execute(pool)
                .await
                .ok();
        } else {
            sqlx::query("UPDATE subscriptions SET last_fetched_at = NOW(), fetch_error = NULL WHERE id = $1")
                .bind(sub.id)
                .execute(pool)
                .await
                .ok();
        }
    }

    Ok(())
}

async fn sync_one_feed(pool: &PgPool, subscription_id: Uuid, user_id: Uuid, url: &str) -> Result<(), String> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    let body = resp.bytes()
        .await
        .map_err(|e| format!("Read error: {}", e))?;

    // Try RSS first
    if let Ok(channel) = rss::Channel::read_from(body.as_ref()) {
        for item in channel.items() {
            let guid = item.guid()
                .map(|g| g.value())
                .or_else(|| item.link())
                .unwrap_or("");

            if guid.is_empty() {
                continue;
            }

            let pub_date = item.pub_date()
                .and_then(|d| chrono::DateTime::parse_from_rfc2822(d).ok())
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            crate::db::rss::RssFeedItemRow::create(
                pool,
                subscription_id,
                user_id,
                guid,
                item.title(),
                item.link(),
                item.description(),
                item.author(),
                pub_date,
                item.content(),
            )
            .await
            .map_err(|e| format!("DB insert error: {}", e))?;
        }
    } else if let Ok(feed) = atom_syndication::Feed::read_from(body.as_ref()) {
        // Try Atom format (YouTube, some Mastodon instances)
        for entry in feed.entries() {
            let guid = entry.id();
            if guid.is_empty() {
                continue;
            }
            let pub_date = entry.published()
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| entry.updated().with_timezone(&chrono::Utc));

            let title = entry.title().value.clone();
            let link = entry.links().first().map(|l| l.href().to_string());
            let content = entry.content().and_then(|c| c.value().map(|v| v.to_string()));
            let summary = entry.summary().map(|s| s.value.clone());

            crate::db::rss::RssFeedItemRow::create(
                pool,
                subscription_id,
                user_id,
                guid,
                Some(&title),
                link.as_deref(),
                summary.as_deref(),
                None,
                pub_date,
                content.as_deref(),
            )
            .await
            .map_err(|e| format!("DB insert error: {}", e))?;
        }
    } else {
        return Err("Not a valid RSS or Atom feed".to_string());
    }

    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct RssSubscription {
    id: Uuid,
    user_id: Uuid,
    url: String,
}

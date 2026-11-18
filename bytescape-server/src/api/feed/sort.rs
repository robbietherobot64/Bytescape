//! Sort modes for the feed.
//!
//! Each sort mode is a different ORDER BY, not a different code path.

use super::FeedPost;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Popular,
    Chronological,
    New,
}

impl SortMode {
    /// Parse from query parameter.
    pub fn from_query(raw: Option<&str>) -> Self {
        match raw.unwrap_or("popular") {
            "chronological" => SortMode::Chronological,
            "new" => SortMode::New,
            _ => SortMode::Popular,
        }
    }

    /// Returns the SQL ORDER BY clause for AppView DB queries.
    pub fn order_by(&self) -> &'static str {
        match self {
            SortMode::Popular => "(COALESCE(like_count,0) * 3 + COALESCE(repost_count,0) * 2 + COALESCE(reply_count,0) * 1) DESC, created_at DESC NULLS LAST",
            SortMode::Chronological => "created_at DESC NULLS LAST",
            SortMode::New => "indexed_at DESC NULLS LAST",
        }
    }

    /// Returns the time filter SQL fragment for AppView DB queries.
    pub fn time_filter(&self) -> &'static str {
        match self {
            SortMode::Popular => "AND created_at > now() - interval '24 hours'",
            _ => "",
        }
    }

    /// Sort posts in-memory by the selected mode.
    pub fn sort_posts(&self, posts: &mut [FeedPost]) {
        match self {
            SortMode::Popular => {
                posts.sort_by(|a, b| {
                    let score_a = a.likes_count * 3 + a.shares_count * 2 + a.replies_count;
                    let score_b = b.likes_count * 3 + b.shares_count * 2 + b.replies_count;
                    score_b
                        .cmp(&score_a)
                        .then_with(|| b.created_at.cmp(&a.created_at))
                });
            }
            SortMode::Chronological | SortMode::New => {
                posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        }
    }
}

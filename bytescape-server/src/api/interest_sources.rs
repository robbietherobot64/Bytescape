use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub bluesky_search: Option<String>,
    pub mastodon_hashtags: Vec<String>,
    pub bluesky_feed: Option<String>,
    pub community: Option<String>,
}

/// Expand interest-picker ids into their topic sources. The topic catalog in
/// `feedgen` is the single vocabulary: an interest IS a topic — picking one
/// subscribes that topic's generator feed plus its hashtags. Picker ids that
/// predate topic rkeys resolve through `feedgen::INTEREST_ALIASES`.
///
/// This replaces the old hand-maintained per-interest side map, where an
/// interest's tags diverged from the topic catalog and "picked interests get
/// ignored" was born.
pub fn get_all_sources_for_interests(ids: &[String]) -> Vec<(String, SourceConfig)> {
    ids.iter()
        .filter_map(|id| {
            let topic = crate::api::feedgen::topic_for_interest(id)?;
            Some((
                id.clone(),
                SourceConfig {
                    bluesky_search: Some(topic.keywords.join(" ")),
                    mastodon_hashtags: topic.tags.iter().map(|s| s.to_string()).collect(),
                    bluesky_feed: Some(crate::api::feedgen::topic_feed_uri(topic)),
                    community: None,
                },
            ))
        })
        .collect()
}

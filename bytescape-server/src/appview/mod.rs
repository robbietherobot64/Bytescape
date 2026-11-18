//! AT Protocol AppView — relay firehose consumer + local search index.
//!
//! Subscribes to the Bluesky relay firehose (wss://bsky.network),
//! indexes all posts in the `appview_posts` table, and serves
//! local XRPC endpoints for search and feed queries.

pub mod consumer;
pub mod embed_backfill;
pub mod endpoints;
pub mod handle_resolver;
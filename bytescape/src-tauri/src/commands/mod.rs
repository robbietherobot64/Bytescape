//! Tauri Command Modules
//!
//! Central registration point for every Tauri IPC command in Bytescape.
//! Each submodule groups commands by concern area; glob re-exports (`pub use *`)
//! flatten them so `lib.rs` can register them in a single `invoke_handler` call.
//!
//! ## Submodule Map
//!
//! | Module             | Responsibility                                          |
//! |--------------------|---------------------------------------------------------|
//! | `node`             | Iroh node lifecycle (setup, shutdown, status)           |
//! | `vault`            | BytescapeItem CRUD, paged queries, search               |
//! | `auth`             | Bluesky OAuth session management                        |
//! | `import`           | Archive ingest (Facebook, Instagram ZIP → vault)        |
//! | `media`            | Blob storage, thumbnail generation, media serving       |
//! | `feed_aggregation` | Unified feed assembly across sources & lenses           |
//! | `syndicate`        | Cross-post vault items to external platforms             |
//! | `posting`          | Compose & publish new posts (Bluesky, local)            |
//! | `interactions`     | Likes, reposts, bookmarks, thread views                 |
//! | `peers`            | P2P peer discovery, connection, trust management        |
//! | `collab`           | CRDT collaborative documents (Yjs over redb)            |
//! | `ap`               | ActivityPub actor, outbox, WebFinger resolution          |
//! | `profile`          | User profile reads/writes (avatar, display name)        |
//! | `unfurl`           | URL preview / link-card metadata extraction              |
//! | `classify`         | Bulk re-classification of vault item types               |
//! | `export`           | Data export (JSON, ZIP archive)                          |
//! | `browser`          | In-app browser helpers                                   |
//! | `mastodon`         | Mastodon-compatible API adapter                          |
//! | `relay`            | Iroh relay / rendezvous configuration                    |
//! | `groups`           | Group membership and group-scoped feeds                  |
//!
//! ## RELATED FILES
//!
//! - `lib.rs`  — Tauri `Builder` where all commands are registered via `invoke_handler`
//! - `models/` — Shared data types consumed by many command modules
//! - `db/`     — Persistence layer called by most commands

// ── Core infrastructure ─────────────────────────────────────────────────────
pub mod node;       // Iroh node lifecycle
pub mod vault;      // BytescapeItem CRUD & queries
pub mod auth;       // Bluesky OAuth sessions

// ── Data ingest & media ─────────────────────────────────────────────────────
pub mod import;     // Archive import (Facebook, Instagram)
pub mod media;      // Blob storage & thumbnails
pub mod classify;   // Bulk item re-classification
pub mod export;     // Data export

// ── Feed & content ──────────────────────────────────────────────────────────
pub mod feed_aggregation; // Unified feed assembly
pub mod posting;          // Compose & publish
pub mod interactions;     // Likes, reposts, bookmarks
pub mod unfurl;           // URL preview cards
pub mod browser;          // In-app browser helpers
pub mod blog_feed;        // RSS/Atom blog feed discovery & parsing

// ── Social & federation ─────────────────────────────────────────────────────
pub mod syndicate;  // Cross-posting to external platforms
pub mod peers;      // P2P peer management
pub mod collab;     // CRDT collaborative editing
pub mod ap;         // ActivityPub federation
pub mod profile;    // User profile management
pub mod mastodon;   // Mastodon-compatible API
pub mod relay;      // Iroh relay configuration
pub mod groups;     // Group membership & feeds

// Re-export all commands for easy registration in lib.rs
pub use node::*;
pub use vault::*;
pub use auth::*;
pub use import::*;
pub use media::*;
pub use syndicate::*;
pub use posting::*;
pub use interactions::*;
pub use peers::*;
pub use collab::*;
pub use ap::*;
pub use profile::*;
pub use unfurl::*;
pub use browser::*;
pub use mastodon::*;
pub use relay::*;
pub use groups::*;
pub use blog_feed::*;

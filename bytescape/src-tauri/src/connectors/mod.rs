//! # connectors/mod.rs — Platform Connector Registry
//!
//! This module defines the `Source` enum and re-exports all platform connectors.
//! Each connector knows how to fetch or parse data from one social media platform
//! and normalize it into Bytescape's universal `BytescapeItem` format.
//!
//! ## Connector Types
//! - **Live connectors** (Bluesky, Mastodon): Pull data in real-time via API calls.
//!   Require the user to authenticate with their platform credentials.
//! - **Archive connectors** (Facebook, Instagram): Parse offline data exports (ZIP files)
//!   that the user has downloaded from the platform's "Download Your Data" feature.
//!
//! ## How to Add a New Connector
//! 1. Create a new file: `connectors/{platform}.rs`
//! 2. Add `pub mod {platform};` below
//! 3. Add a new variant to the `Source` enum
//! 4. Implement the parsing/fetching logic that produces `BytescapeItem` instances
//! 5. Register any new Tauri commands in `lib.rs`
//!
//! ## Related Files
//! - `models/mod.rs` — The `BytescapeItem` type that all connectors produce
//! - `commands/import.rs` — The import pipeline that calls archive connectors
//! - `commands/feed_aggregation.rs` — The feed pipeline that calls live connectors

pub mod bluesky;
pub mod facebook;
pub mod instagram;
pub mod mastodon;

use serde::{Deserialize, Serialize};

/// Every platform connector normalizes data into BytescapeItem.
/// This enum identifies which platform a piece of content came from.
/// Used for source badges in the UI, for routing API calls to the
/// right connector, and for tracking data provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Source {
    /// Bluesky (AT Protocol) — live API connector
    Bluesky,
    /// Mastodon (ActivityPub) — live API connector
    Mastodon,
    /// Facebook — archive import connector (parses HTML data exports)
    Facebook,
    /// Instagram — archive import connector (parses JSON data exports)
    Instagram,
    /// Google Takeout — archive import connector (planned)
    Google,
    /// Twitter/X — archive import connector (planned)
    Twitter,
    /// Generic ActivityPub federation (non-Mastodon fediverse servers)
    ActivityPub,
    /// Locally composed in Bytescape (no external platform)
    Manual,
}

impl Source {
    /// Returns the lowercase string identifier used in database storage and IPC.
    /// This is how source is stored in BytescapeItem.source and sent to the frontend.
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Bluesky => "bluesky",
            Source::Mastodon => "mastodon",
            Source::Facebook => "facebook",
            Source::Instagram => "instagram",
            Source::Google => "google",
            Source::Twitter => "twitter",
            Source::ActivityPub => "activitypub",
            Source::Manual => "bytescape",
        }
    }

    /// Parses a source string back into a Source enum.
    /// Unknown strings default to Manual (locally-created content).
    pub fn from_str(s: &str) -> Self {
        match s {
            "bluesky" => Source::Bluesky,
            "mastodon" => Source::Mastodon,
            "facebook" => Source::Facebook,
            "instagram" => Source::Instagram,
            "google" => Source::Google,
            "twitter" => Source::Twitter,
            "activitypub" => Source::ActivityPub,
            "bytescape" => Source::Manual,
            _ => Source::Manual,
        }
    }

    /// Human-readable display name for the source (shown in UI).
    pub fn label(&self) -> &'static str {
        match self {
            Source::Bluesky => "Bluesky",
            Source::Mastodon => "Mastodon",
            Source::Facebook => "Facebook",
            Source::Instagram => "Instagram",
            Source::Google => "Google",
            Source::Twitter => "Twitter/X",
            Source::ActivityPub => "ActivityPub",
            Source::Manual => "Bytescape",
        }
    }

    /// Emoji icon for the source (shown in source badges and UI labels).
    pub fn emoji(&self) -> &'static str {
        match self {
            Source::Bluesky => "🦋",
            Source::Mastodon => "🐘",
            Source::Facebook => "📘",
            Source::Instagram => "📸",
            Source::Google => "🔍",
            Source::Twitter => "🐦",
            Source::ActivityPub => "🌐",
            Source::Manual => "✨",
        }
    }
}

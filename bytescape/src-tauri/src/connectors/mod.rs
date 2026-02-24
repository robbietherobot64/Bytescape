pub mod bluesky;
pub mod facebook;
pub mod instagram;

use serde::{Deserialize, Serialize};

/// Every platform connector normalizes data into BytescapeItem.
/// Live connectors (Bluesky, Mastodon) pull via API.
/// Archive connectors (Facebook, Google, Twitter) parse export files.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Source {
    Bluesky,
    Mastodon,
    Facebook,
    Instagram,
    Google,
    Twitter,
    Manual,
}

impl Source {
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Bluesky => "bluesky",
            Source::Mastodon => "mastodon",
            Source::Facebook => "facebook",
            Source::Instagram => "instagram",
            Source::Google => "google",
            Source::Twitter => "twitter",
            Source::Manual => "manual",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "bluesky" => Source::Bluesky,
            "mastodon" => Source::Mastodon,
            "facebook" => Source::Facebook,
            "instagram" => Source::Instagram,
            "google" => Source::Google,
            "twitter" => Source::Twitter,
            _ => Source::Manual,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Source::Bluesky => "Bluesky",
            Source::Mastodon => "Mastodon",
            Source::Facebook => "Facebook",
            Source::Instagram => "Instagram",
            Source::Google => "Google",
            Source::Twitter => "Twitter/X",
            Source::Manual => "Manual",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Source::Bluesky => "🦋",
            Source::Mastodon => "🐘",
            Source::Facebook => "📘",
            Source::Instagram => "📸",
            Source::Google => "🔍",
            Source::Twitter => "🐦",
            Source::Manual => "📝",
        }
    }
}

//! # models/groups.rs — Group & Topic Data Models
//!
//! This module defines the data structures for Bytescape's hierarchical
//! community/forum system. It's organized into:
//!
//! - **Topics**: Categories that form a navigable tree hierarchy.
//!   Example: Health → Mental Health → Anxiety
//! - **Groups**: User-created communities filed under topics.
//!   Each group has a visibility mode, member list, and post feed.
//! - **Members**: Users belonging to a group, with roles (Owner/Mod/Member)
//! - **Group Posts**: References linking BytescapeItems to groups
//!
//! ## Visibility Modes
//! - **Public**: Anyone can find and join automatically
//! - **Closed**: Searchable, but joining requires moderator approval
//! - **Private**: Not listed in search; invite-only
//!
//! ## TypeScript Mirror
//! All types here have corresponding interfaces in `src/types/index.ts`
//! (Section 16: Groups & Topics).
//!
//! ## Related Files
//! - `db/groups.rs` — Database CRUD operations for these types
//! - `commands/groups.rs` — Tauri commands exposing group operations to frontend
//! - `activitypub/group_actor.rs` — ActivityPub federation for groups

use serde::{Deserialize, Serialize};

// ─── Enums ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum GroupVisibility {
    Public,  // Searchable, auto-join on Follow
    Closed,  // Searchable, join requires mod approval
    Private, // Not listed, invite-only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MemberRole {
    Owner,
    Moderator,
    Member,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MemberStatus {
    Active,
    Pending, // Awaiting approval (closed groups)
    Invited, // Invited but hasn't accepted (private groups)
    Banned,
}

// ─── Topic ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Topic {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub parent_id: Option<String>,
    pub description: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub created_by: String,
    pub created_at: u64,
}

/// Recursive tree node returned by `get_topic_tree`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TopicNode {
    #[serde(flatten)]
    pub topic: Topic,
    pub children: Vec<TopicNode>,
    pub groups: Vec<GroupSummary>,
    pub subscribed: bool,
}

// ─── Group ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalGroup {
    pub group_id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub visibility: GroupVisibility,
    pub topic_id: String,
    pub owner_did: String,
    pub icon_blob: Option<String>,
    pub banner_blob: Option<String>,
    pub rules: Option<String>,
    pub posting_restricted_to_mods: bool,
    pub member_count: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Lightweight group summary for topic tree listings.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupSummary {
    pub group_id: String,
    pub name: String,
    pub slug: String,
    pub visibility: GroupVisibility,
    pub member_count: u32,
    pub icon_blob: Option<String>,
}

impl From<&LocalGroup> for GroupSummary {
    fn from(g: &LocalGroup) -> Self {
        Self {
            group_id: g.group_id.clone(),
            name: g.name.clone(),
            slug: g.slug.clone(),
            visibility: g.visibility,
            member_count: g.member_count,
            icon_blob: g.icon_blob.clone(),
        }
    }
}

// ─── Membership ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupMember {
    pub group_id: String,
    pub member_did: String,
    pub role: MemberRole,
    pub status: MemberStatus,
    pub joined_at: u64,
}

// ─── Group Posts ───────────────────────────────────────────────────

/// Links an existing BytescapeItem to a group. The item itself lives in ITEMS_TABLE.
/// This is a tag/reference, not a copy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupPost {
    pub group_id: String,
    pub item_id: String,
    pub pinned: bool,
    pub shared_by: Option<String>,
    pub posted_at: u64,
}

// ─── Topic Subscription ───────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TopicSubscription {
    pub topic_id: String,
    pub user_did: String,
    pub subscribed_at: u64,
}

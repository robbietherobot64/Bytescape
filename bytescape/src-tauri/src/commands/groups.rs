//! Tauri commands for Topics, Groups, Membership, and Group Feeds.
//!
//! Sprint v12 — hierarchical forum system with nested topic trees, group
//! CRUD, membership management, moderation tools, and aggregated group feeds.
//! Sprint v13 added Lemmy/PieFed compatibility stubs.
//!
//! ## HOW IT WORKS
//!
//! 1. **Topic Tree** — Topics form an N-ary tree (parent_id links). The frontend
//!    calls `get_topic_tree` which loads *all* topics into memory and recursively
//!    builds `TopicNode` structs (tech debt: no cycle detection, loads entire table).
//! 2. **Groups** — Each group belongs to exactly one topic via `topic_id`. Groups
//!    have three visibility levels: Public (open join), Closed (requires approval),
//!    and Private (invite-only). CRUD operations enforce owner/mod authorization.
//! 3. **Membership** — `join_group` handles the Public/Closed/Private flow;
//!    `invite_member` + `accept_invite` handle the Private path. Banned members
//!    are blocked from re-joining. Owner/Moderator roles gate moderation actions.
//! 4. **Group Feeds** — `post_to_group` / `share_to_group` link `BytescapeItem`
//!    records into groups. `get_groups_feed` aggregates across direct memberships
//!    *and* topic subscriptions (BFS-expanding sub-topics).
//! 5. **Lemmy Compatibility** — `discover_lemmy_communities` and
//!    `follow_remote_community` fetch remote AP actors and create local placeholder
//!    groups. Signed HTTP delivery is deferred until relay deployment.
//!
//! ## RELATED FILES
//!
//! - `src-tauri/src/models/groups.rs` — struct definitions: Topic, LocalGroup,
//!   GroupMember, GroupPost, TopicNode, GroupSummary, enums
//! - `src-tauri/src/db/groups.rs` — redb persistence layer for all group tables
//! - `src-tauri/src/activitypub/lemmy.rs` — Lemmy AP fetching and Follow building
//! - `src-tauri/src/lib.rs` — command registration in `invoke_handler`
//! - `src/screens/GroupsScreen.tsx` — primary frontend consumer of these commands

use tauri::State;
use crate::AppState;
use crate::models::groups::*;
use crate::db::DbManager;

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Returns the current wall-clock time as milliseconds since the Unix epoch.
/// Used to stamp `created_at`, `updated_at`, `joined_at`, and `posted_at` fields.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Extracts the `DbManager` from shared app state, failing with a user-facing
/// error if the database has not been initialized yet.
async fn require_db(state: &State<'_, AppState>) -> Result<DbManager, String> {
    state
        .db_manager
        .read()
        .await
        .clone()
        .ok_or_else(|| "Database not initialized".to_string())
}

/// Retrieves the current vault owner's DID from the keychain.
/// Returns an error if no identity is configured (user must log in first).
async fn current_user_did(state: &State<'_, AppState>) -> Result<String, String> {
    state
        .keychain
        .get_owner_did()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No identity — log in or create a local vault first".to_string())
}

// ═══════════════════════════════════════════════════════════════════
// Topics — Tree CRUD
// ═══════════════════════════════════════════════════════════════════

/// Creates a new topic node in the hierarchical topic tree.
///
/// # IPC
/// Tauri command `create_topic` — called from topic management UI.
///
/// # Params
/// - `name` — human-readable display name (e.g. "Programming")
/// - `slug` — URL-safe identifier (e.g. "programming")
/// - `parent_id` — if `Some`, the new topic becomes a child of this parent;
///   `None` creates a root-level topic. The parent must exist.
/// - `description` — short blurb shown in the topic tree sidebar
/// - `icon` — optional emoji or icon identifier
/// - `sort_order` — integer controlling display order among siblings
///
/// # Returns
/// The newly created `Topic` with a generated UUID.
#[tauri::command]
pub async fn create_topic(
    state: State<'_, AppState>,
    name: String,
    slug: String,
    parent_id: Option<String>,
    description: String,
    icon: Option<String>,
    sort_order: i32,
) -> Result<Topic, String> {
    let db = require_db(&state).await?;
    let owner_did = current_user_did(&state).await?;

    // Validate parent exists if provided
    if let Some(ref pid) = parent_id {
        if db.get_topic(pid).await?.is_none() {
            return Err(format!("Parent topic '{}' not found", pid));
        }
    }

    let topic = Topic {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        slug,
        parent_id,
        description,
        icon,
        sort_order,
        created_by: owner_did,
        created_at: now_ms(),
    };

    db.save_topic(&topic).await?;
    Ok(topic)
}

/// Updates an existing topic's metadata (name, description, icon, sort order,
/// or parent). Supports reparenting: `parent_id = Some(Some(id))` moves the
/// topic, `Some(None)` promotes it to root, `None` leaves it unchanged.
///
/// # IPC
/// Tauri command `update_topic`.
///
/// # Errors
/// - Topic not found
/// - Parent topic not found (if reparenting)
/// - Self-parenting (`parent_id == topic_id`)
///
/// # Tech Debt
/// No cycle detection — reparenting A→B when B is already under A creates a
/// cycle that `get_topic_tree` will silently skip (orphaned subtree).
#[tauri::command]
pub async fn update_topic(
    state: State<'_, AppState>,
    topic_id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    sort_order: Option<i32>,
    parent_id: Option<Option<String>>,
) -> Result<Topic, String> {
    let db = require_db(&state).await?;
    let mut topic = db
        .get_topic(&topic_id)
        .await?
        .ok_or_else(|| format!("Topic '{}' not found", topic_id))?;

    if let Some(n) = name {
        topic.name = n;
    }
    if let Some(d) = description {
        topic.description = d;
    }
    if let Some(i) = icon {
        topic.icon = Some(i);
    }
    if let Some(s) = sort_order {
        topic.sort_order = s;
    }
    // Some(None) = move to root, Some(Some(id)) = reparent, None = don't change
    if let Some(new_parent) = parent_id {
        if let Some(ref pid) = new_parent {
            if db.get_topic(pid).await?.is_none() {
                return Err(format!("Parent topic '{}' not found", pid));
            }
            // Prevent self-parenting
            if pid == &topic_id {
                return Err("A topic cannot be its own parent".to_string());
            }
        }
        topic.parent_id = new_parent;
    }

    db.save_topic(&topic).await?;
    Ok(topic)
}

/// Deletes a leaf topic from the tree.
///
/// # IPC
/// Tauri command `delete_topic_cmd`.
///
/// # Constraints
/// - Refuses to delete a topic that has child topics (delete children first).
/// - Refuses to delete a topic that has groups assigned to it (move/delete groups first).
///
/// # Tech Debt
/// Loads *all* topics via `get_all_topics()` to check for children — could use a
/// targeted query like `get_children_of(topic_id)` instead.
#[tauri::command]
pub async fn delete_topic_cmd(
    state: State<'_, AppState>,
    topic_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;

    // Fail if child topics exist
    let all_topics = db.get_all_topics().await?;
    if all_topics.iter().any(|t| t.parent_id.as_deref() == Some(&topic_id)) {
        return Err("Cannot delete topic with child topics — delete children first".to_string());
    }

    // Fail if groups are assigned to this topic
    let groups = db.get_groups_by_topic(&topic_id).await?;
    if !groups.is_empty() {
        return Err(format!(
            "Cannot delete topic with {} groups — move or delete groups first",
            groups.len()
        ));
    }

    db.delete_topic(&topic_id).await?;
    Ok(())
}

/// Builds and returns the full topic tree with nested groups and subscription flags.
///
/// # IPC
/// Tauri command `get_topic_tree` — primary data source for the Groups sidebar.
///
/// # Algorithm
/// 1. Loads **all** topics and **all** groups into memory.
/// 2. Filters private groups the current user is not a member of.
/// 3. Builds a `HashMap<topic_id, Vec<GroupSummary>>` index.
/// 4. Recursively constructs `TopicNode` trees starting from root topics
///    (`parent_id == None`), sorted by `sort_order`.
///
/// # Returns
/// A `Vec<TopicNode>` representing root-level topics, each with nested
/// `children` and attached `groups`.
///
/// # Tech Debt
/// Loads the entire topics and groups tables into memory on every call.
/// Fine for hundreds of topics, but will need pagination or caching at scale.
#[tauri::command]
pub async fn get_topic_tree(
    state: State<'_, AppState>,
) -> Result<Vec<TopicNode>, String> {
    let db = require_db(&state).await?;
    let all_topics = db.get_all_topics().await?;
    let all_groups = db.get_all_groups().await?;

    // Get current user DID for subscription checks and private group filtering
    let user_did = state
        .keychain
        .get_owner_did()
        .ok()
        .flatten()
        .unwrap_or_default();

    // Get user's subscribed topic IDs
    let subscribed_ids: std::collections::HashSet<String> = if !user_did.is_empty() {
        db.get_topic_subscriptions(&user_did)
            .await?
            .into_iter()
            .map(|s| s.topic_id)
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    // Get user's group memberships for private group visibility
    let user_group_ids: std::collections::HashSet<String> = if !user_did.is_empty() {
        db.get_user_groups(&user_did)
            .await?
            .into_iter()
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    // Build groups-by-topic map, filtering private groups
    let mut groups_by_topic: std::collections::HashMap<String, Vec<GroupSummary>> =
        std::collections::HashMap::new();
    for group in &all_groups {
        // Private groups only visible if user is a member
        if group.visibility == GroupVisibility::Private
            && !user_group_ids.contains(&group.group_id)
        {
            continue;
        }
        groups_by_topic
            .entry(group.topic_id.clone())
            .or_default()
            .push(GroupSummary::from(group));
    }

    // Build topic tree recursively
    fn build_children(
        parent_id: Option<&str>,
        topics: &[Topic],
        groups_by_topic: &std::collections::HashMap<String, Vec<GroupSummary>>,
        subscribed_ids: &std::collections::HashSet<String>,
    ) -> Vec<TopicNode> {
        let mut children: Vec<TopicNode> = topics
            .iter()
            .filter(|t| t.parent_id.as_deref() == parent_id)
            .map(|t| {
                let kids = build_children(
                    Some(&t.id),
                    topics,
                    groups_by_topic,
                    subscribed_ids,
                );
                TopicNode {
                    topic: t.clone(),
                    children: kids,
                    groups: groups_by_topic
                        .get(&t.id)
                        .cloned()
                        .unwrap_or_default(),
                    subscribed: subscribed_ids.contains(&t.id),
                }
            })
            .collect();
        children.sort_by_key(|n| n.topic.sort_order);
        children
    }

    let tree = build_children(None, &all_topics, &groups_by_topic, &subscribed_ids);
    Ok(tree)
}

// ═══════════════════════════════════════════════════════════════════
// Topics — Subscriptions
// ═══════════════════════════════════════════════════════════════════

/// Subscribes the current user to a topic, so its groups appear in
/// the aggregated groups feed (`get_groups_feed`).
///
/// # IPC
/// Tauri command `subscribe_topic`.
///
/// # Errors
/// Returns an error if the topic does not exist.
#[tauri::command]
pub async fn subscribe_topic(
    state: State<'_, AppState>,
    topic_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    // Validate topic exists
    if db.get_topic(&topic_id).await?.is_none() {
        return Err(format!("Topic '{}' not found", topic_id));
    }

    let sub = TopicSubscription {
        topic_id,
        user_did,
        subscribed_at: now_ms(),
    };
    db.save_topic_subscription(&sub).await?;
    Ok(())
}

/// Removes the current user's subscription to a topic.
///
/// # IPC
/// Tauri command `unsubscribe_topic`.
#[tauri::command]
pub async fn unsubscribe_topic(
    state: State<'_, AppState>,
    topic_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;
    db.delete_topic_subscription(&user_did, &topic_id).await?;
    Ok(())
}

/// Returns the list of topic IDs the current user is subscribed to.
///
/// # IPC
/// Tauri command `get_subscribed_topics`.
#[tauri::command]
pub async fn get_subscribed_topics(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;
    let subs = db.get_topic_subscriptions(&user_did).await?;
    Ok(subs.into_iter().map(|s| s.topic_id).collect())
}

// ═══════════════════════════════════════════════════════════════════
// Groups — CRUD
// ═══════════════════════════════════════════════════════════════════

/// Creates a new group under the specified topic.
///
/// # IPC
/// Tauri command `create_group` — called from the "New Group" dialog.
///
/// # Params
/// - `visibility` — `Public` (anyone can join), `Closed` (join requires approval),
///   or `Private` (invite-only; hidden from non-members in topic tree).
/// - `topic_id` — must reference an existing topic.
/// - `rules` — optional markdown string displayed on the group info page.
/// - `icon_blob` / `banner_blob` — optional blob references for group branding.
///
/// # Side Effects
/// - Generates a UUID for `group_id`.
/// - Auto-creates a `GroupMember` record with `Owner` role for the calling user.
/// - Sets initial `member_count = 1`.
#[tauri::command]
pub async fn create_group(
    state: State<'_, AppState>,
    name: String,
    slug: String,
    description: String,
    visibility: GroupVisibility,
    topic_id: String,
    rules: Option<String>,
    icon_blob: Option<String>,
    banner_blob: Option<String>,
) -> Result<LocalGroup, String> {
    let db = require_db(&state).await?;
    let owner_did = current_user_did(&state).await?;

    // Validate topic exists
    if db.get_topic(&topic_id).await?.is_none() {
        return Err(format!("Topic '{}' not found", topic_id));
    }

    let now = now_ms();
    let group = LocalGroup {
        group_id: uuid::Uuid::new_v4().to_string(),
        name,
        slug,
        description,
        visibility,
        topic_id,
        owner_did: owner_did.clone(),
        icon_blob,
        banner_blob,
        rules,
        posting_restricted_to_mods: false,
        member_count: 1,
        created_at: now,
        updated_at: now,
    };

    db.save_group(&group).await?;

    // Auto-create the owner as a member
    let owner_member = GroupMember {
        group_id: group.group_id.clone(),
        member_did: owner_did,
        role: MemberRole::Owner,
        status: MemberStatus::Active,
        joined_at: now,
    };
    db.save_group_member(&owner_member).await?;

    Ok(group)
}

/// Updates a group's settings. Only the group `Owner` or a `Moderator` can
/// perform this operation.
///
/// # IPC
/// Tauri command `update_group`.
///
/// # Params
/// All fields are optional — only provided fields are updated.
/// - `icon_blob` / `banner_blob` — use `Some(None)` to clear, `Some(Some(url))`
///   to set, `None` to leave unchanged (same triple-option pattern as `update_topic`).
/// - `posting_restricted_to_mods` — when `true`, only Owner/Moderator can post.
///
/// # Errors
/// - Group not found
/// - Caller is not Owner or Moderator
/// - New `topic_id` does not exist
#[tauri::command]
pub async fn update_group(
    state: State<'_, AppState>,
    group_id: String,
    name: Option<String>,
    description: Option<String>,
    visibility: Option<GroupVisibility>,
    topic_id: Option<String>,
    rules: Option<String>,
    icon_blob: Option<Option<String>>,
    banner_blob: Option<Option<String>>,
    posting_restricted_to_mods: Option<bool>,
) -> Result<LocalGroup, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let mut group = db
        .get_group(&group_id)
        .await?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // Only owner or moderator can update
    let membership = db.get_group_member(&group_id, &user_did).await?;
    match membership {
        Some(ref m) if m.role == MemberRole::Owner || m.role == MemberRole::Moderator => {}
        _ => return Err("Only group owner or moderator can update group settings".to_string()),
    }

    if let Some(n) = name {
        group.name = n;
    }
    if let Some(d) = description {
        group.description = d;
    }
    if let Some(v) = visibility {
        group.visibility = v;
    }
    if let Some(tid) = topic_id {
        if db.get_topic(&tid).await?.is_none() {
            return Err(format!("Topic '{}' not found", tid));
        }
        group.topic_id = tid;
    }
    if let Some(r) = rules {
        group.rules = Some(r);
    }
    if let Some(ib) = icon_blob {
        group.icon_blob = ib;
    }
    if let Some(bb) = banner_blob {
        group.banner_blob = bb;
    }
    if let Some(p) = posting_restricted_to_mods {
        group.posting_restricted_to_mods = p;
    }

    group.updated_at = now_ms();
    db.save_group(&group).await?;
    Ok(group)
}

/// Permanently deletes a group and all associated data (members, posts).
/// Only the group owner can perform this action.
///
/// # IPC
/// Tauri command `delete_group_cmd`.
///
/// # Side Effects
/// Cascading delete: removes all `GroupMember` and `GroupPost` records
/// for this group before deleting the group record itself.
#[tauri::command]
pub async fn delete_group_cmd(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let group = db
        .get_group(&group_id)
        .await?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // Only owner can delete
    if group.owner_did != user_did {
        return Err("Only the group owner can delete the group".to_string());
    }

    // Clean up all associated data
    db.delete_all_group_members(&group_id).await?;
    db.delete_all_group_posts(&group_id).await?;
    db.delete_group(&group_id).await?;

    Ok(())
}

/// Retrieves a single group by ID. Returns `None` for private groups if the
/// caller is not a member (prevents information leakage).
///
/// # IPC
/// Tauri command `get_group_cmd`.
#[tauri::command]
pub async fn get_group_cmd(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<Option<LocalGroup>, String> {
    let db = require_db(&state).await?;
    let group = db.get_group(&group_id).await?;

    // If private and user is not a member, return None (no info leak)
    if let Some(ref g) = group {
        if g.visibility == GroupVisibility::Private {
            let user_did = current_user_did(&state).await.unwrap_or_default();
            if db.get_group_member(&group_id, &user_did).await?.is_none() {
                return Ok(None);
            }
        }
    }

    Ok(group)
}

/// Lists all groups under a specific topic.
///
/// # IPC
/// Tauri command `list_groups_by_topic`.
///
/// Note: Does not filter private groups — the caller may need to check
/// visibility or membership on the frontend side.
#[tauri::command]
pub async fn list_groups_by_topic(
    state: State<'_, AppState>,
    topic_id: String,
) -> Result<Vec<LocalGroup>, String> {
    let db = require_db(&state).await?;
    db.get_groups_by_topic(&topic_id).await
}

// ═══════════════════════════════════════════════════════════════════
// Groups — Search
// ═══════════════════════════════════════════════════════════════════

/// Searches groups by name/description substring match.
///
/// # IPC
/// Tauri command `search_groups_cmd`.
///
/// # Tech Debt
/// Delegates to `DbManager::search_groups` which performs a full table scan
/// with case-insensitive `contains()` — acceptable for local-first use but
/// will need an index or FTS if group counts grow significantly.
#[tauri::command]
pub async fn search_groups_cmd(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<LocalGroup>, String> {
    let db = require_db(&state).await?;
    db.search_groups(&query).await
}

// ═══════════════════════════════════════════════════════════════════
// Membership — Join / Leave / Approve / Reject / Invite / Ban / Role
// ═══════════════════════════════════════════════════════════════════

/// Requests to join a group. Behavior varies by group visibility:
///
/// | Visibility | Result |
/// |------------|--------|
/// | `Public`   | Immediately `Active`, `member_count` incremented |
/// | `Closed`   | `Pending` — awaits `approve_member` by a mod |
/// | `Private`  | Rejected — private groups are invite-only |
///
/// If the user was previously `Invited`, calling `join_group` accepts the
/// invitation and promotes to `Active`.
///
/// # IPC
/// Tauri command `join_group`.
///
/// # Returns
/// The resulting `MemberStatus` (`Active` or `Pending`).
///
/// # Errors
/// - Already a member / already pending / banned
/// - Group is private (must use `invite_member` + `accept_invite`)
#[tauri::command]
pub async fn join_group(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<MemberStatus, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let group = db
        .get_group(&group_id)
        .await?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // Check for duplicate joins
    if let Some(existing) = db.get_group_member(&group_id, &user_did).await? {
        return match existing.status {
            MemberStatus::Active => Err("Already a member of this group".to_string()),
            MemberStatus::Pending => Err("Join request already pending".to_string()),
            MemberStatus::Invited => {
                // Accept the invitation — promote to Active
                let mut updated = existing;
                updated.status = MemberStatus::Active;
                updated.joined_at = now_ms();
                db.save_group_member(&updated).await?;
                db.update_group_member_count(&group_id, 1).await?;
                Ok(MemberStatus::Active)
            }
            MemberStatus::Banned => Err("You have been banned from this group".to_string()),
        };
    }

    let (status, count_delta) = match group.visibility {
        GroupVisibility::Public => (MemberStatus::Active, 1),
        GroupVisibility::Closed => (MemberStatus::Pending, 0), // no count until approved
        GroupVisibility::Private => {
            return Err("This group is invite-only".to_string());
        }
    };

    let member = GroupMember {
        group_id: group_id.clone(),
        member_did: user_did,
        role: MemberRole::Member,
        status,
        joined_at: now_ms(),
    };
    db.save_group_member(&member).await?;

    if count_delta > 0 {
        db.update_group_member_count(&group_id, count_delta).await?;
    }

    Ok(status)
}

/// Removes the current user from a group. The group owner cannot leave —
/// they must transfer ownership or delete the group.
///
/// # IPC
/// Tauri command `leave_group`.
///
/// # Side Effects
/// Decrements `member_count` if the departing member was `Active`.
#[tauri::command]
pub async fn leave_group(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let member = db
        .get_group_member(&group_id, &user_did)
        .await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;

    // Owner cannot leave — must transfer ownership or delete group
    if member.role == MemberRole::Owner {
        return Err(
            "Owner cannot leave the group — transfer ownership or delete the group instead"
                .to_string(),
        );
    }

    let was_active = member.status == MemberStatus::Active;
    db.delete_group_member(&group_id, &user_did).await?;

    if was_active {
        db.update_group_member_count(&group_id, -1).await?;
    }

    Ok(())
}

/// Returns the members of a group, optionally filtered by status.
///
/// # IPC
/// Tauri command `get_group_members_cmd`.
///
/// # Authorization
/// - Owner/Moderator: can see all statuses (Active, Pending, Invited, Banned).
/// - Regular user: only sees `Active` members (Pending/Invited are hidden).
/// - The `status_filter` param is applied *after* the authorization filter.
#[tauri::command]
pub async fn get_group_members_cmd(
    state: State<'_, AppState>,
    group_id: String,
    status_filter: Option<MemberStatus>,
) -> Result<Vec<GroupMember>, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let mut members = db.get_group_members(&group_id).await?;

    // Only owner/mod can see Pending and Invited members
    let my_membership = db.get_group_member(&group_id, &user_did).await?;
    let is_privileged = my_membership
        .map(|m| m.role == MemberRole::Owner || m.role == MemberRole::Moderator)
        .unwrap_or(false);

    if !is_privileged {
        members.retain(|m| m.status == MemberStatus::Active);
    }

    if let Some(filter) = status_filter {
        members.retain(|m| m.status == filter);
    }

    Ok(members)
}

/// Returns all groups the current user is a member of (any status).
/// Used to populate the "My Groups" sidebar section.
///
/// # IPC
/// Tauri command `get_user_group_memberships`.
#[tauri::command]
pub async fn get_user_group_memberships(
    state: State<'_, AppState>,
) -> Result<Vec<LocalGroup>, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;
    let group_ids = db.get_user_groups(&user_did).await?;

    let mut groups = Vec::new();
    for gid in group_ids {
        if let Some(group) = db.get_group(&gid).await? {
            groups.push(group);
        }
    }
    Ok(groups)
}

// ═══════════════════════════════════════════════════════════════════
// Group Feeds — Post, Share, Read
// ═══════════════════════════════════════════════════════════════════

/// Posts an existing `BytescapeItem` into a group's feed.
///
/// # IPC
/// Tauri command `post_to_group`.
///
/// # Authorization
/// - Caller must be an `Active` member.
/// - If `posting_restricted_to_mods` is enabled, caller must be Owner or Moderator.
///
/// # Errors
/// - Group or item not found
/// - Not a member, or membership not active
/// - Posting restricted and caller lacks mod/owner role
#[tauri::command]
pub async fn post_to_group(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let group = db
        .get_group(&group_id)
        .await?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // Verify user is an active member
    let member = db
        .get_group_member(&group_id, &user_did)
        .await?
        .ok_or_else(|| "You must be a member to post in this group".to_string())?;

    if member.status != MemberStatus::Active {
        return Err("Your membership is not active".to_string());
    }

    // If posting restricted to mods, verify role
    if group.posting_restricted_to_mods
        && member.role != MemberRole::Owner
        && member.role != MemberRole::Moderator
    {
        return Err("Posting in this group is restricted to moderators".to_string());
    }

    // Verify item exists
    if db.get_item_by_id(&item_id).await?.is_none() {
        return Err(format!("Item '{}' not found", item_id));
    }

    let post = GroupPost {
        group_id,
        item_id,
        pinned: false,
        shared_by: None,
        posted_at: now_ms(),
    };
    db.save_group_post(&post).await?;
    Ok(())
}

/// Shares (cross-posts) an item to a different group, recording the sharer's DID.
/// Same authorization rules as `post_to_group`, but sets `shared_by` on the
/// `GroupPost` record so the UI can display "shared by @user".
///
/// # IPC
/// Tauri command `share_to_group`.
#[tauri::command]
pub async fn share_to_group(
    state: State<'_, AppState>,
    item_id: String,
    target_group_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    // Verify user is an active member of target group
    let member = db
        .get_group_member(&target_group_id, &user_did)
        .await?
        .ok_or_else(|| "You must be a member to share to this group".to_string())?;

    if member.status != MemberStatus::Active {
        return Err("Your membership is not active".to_string());
    }

    let group = db
        .get_group(&target_group_id)
        .await?
        .ok_or_else(|| format!("Group '{}' not found", target_group_id))?;

    if group.posting_restricted_to_mods
        && member.role != MemberRole::Owner
        && member.role != MemberRole::Moderator
    {
        return Err("Sharing to this group is restricted to moderators".to_string());
    }

    // Verify item exists
    if db.get_item_by_id(&item_id).await?.is_none() {
        return Err(format!("Item '{}' not found", item_id));
    }

    let post = GroupPost {
        group_id: target_group_id,
        item_id,
        pinned: false,
        shared_by: Some(user_did),
        posted_at: now_ms(),
    };
    db.save_group_post(&post).await?;
    Ok(())
}

/// Returns the paginated feed for a single group, hydrated with full
/// `BytescapeItem` records.
///
/// # IPC
/// Tauri command `get_group_feed`.
///
/// # Params
/// - `limit` — maximum number of items to return.
/// - `cursor` — opaque pagination cursor (timestamp string from a previous page).
#[tauri::command]
pub async fn get_group_feed(
    state: State<'_, AppState>,
    group_id: String,
    limit: usize,
    cursor: Option<String>,
) -> Result<Vec<crate::models::BytescapeItem>, String> {
    let db = require_db(&state).await?;

    let group_posts = db.get_group_posts(&group_id, limit, cursor).await?;

    // Hydrate with BytescapeItem data
    let mut items = Vec::new();
    for gp in group_posts {
        if let Some(item) = db.get_item_by_id(&gp.item_id).await? {
            items.push(item);
        }
    }
    Ok(items)
}

/// Returns an aggregated feed across **all** groups the user is connected to,
/// combining direct group memberships with groups under subscribed topics
/// (recursively expanding sub-topics via BFS).
///
/// # IPC
/// Tauri command `get_groups_feed` — powers the "Groups" tab in the main feed.
///
/// # Algorithm
/// 1. Collect group IDs from the user's direct memberships.
/// 2. Collect subscribed topic IDs, then BFS-expand to include all sub-topics.
/// 3. Collect all groups under the expanded topic set.
/// 4. Union the two group sets.
/// 5. Fetch recent `GroupPost` entries from each group (up to `limit * 2` per group).
/// 6. Merge, sort by `posted_at` descending, truncate to `limit`, hydrate items.
///
/// # Tech Debt
/// - BFS iterates the full `all_topics` array on each frontier pop — O(topics × depth).
/// - Fetches `limit * 2` posts per group then merges — could over-fetch significantly
///   with many groups. A proper merge-sort with per-group cursors would be more efficient.
/// - No cycle detection in BFS (safe because tree structure prevents cycles, but
///   a malformed `parent_id` loop would cause infinite expansion).
#[tauri::command]
pub async fn get_groups_feed(
    state: State<'_, AppState>,
    limit: usize,
    cursor: Option<String>,
) -> Result<Vec<crate::models::BytescapeItem>, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    // 1. Get all group_ids from direct memberships
    let direct_group_ids: std::collections::HashSet<String> =
        db.get_user_groups(&user_did).await?.into_iter().collect();

    // 2. Get subscribed topic_ids, recursively collect groups under each topic + sub-topics
    let subscribed_subs = db.get_topic_subscriptions(&user_did).await?;
    let subscribed_topic_ids: std::collections::HashSet<String> =
        subscribed_subs.into_iter().map(|s| s.topic_id).collect();

    let mut topic_group_ids = std::collections::HashSet::new();
    if !subscribed_topic_ids.is_empty() {
        let all_topics = db.get_all_topics().await?;

        // Recursively expand subscribed topics to include all sub-topics
        fn collect_subtopic_ids(
            parent_ids: &std::collections::HashSet<String>,
            all_topics: &[Topic],
        ) -> std::collections::HashSet<String> {
            let mut result = parent_ids.clone();
            let mut frontier: Vec<String> = parent_ids.iter().cloned().collect();
            while let Some(tid) = frontier.pop() {
                for t in all_topics {
                    if t.parent_id.as_deref() == Some(&tid) && result.insert(t.id.clone()) {
                        frontier.push(t.id.clone());
                    }
                }
            }
            result
        }

        let expanded_topic_ids = collect_subtopic_ids(&subscribed_topic_ids, &all_topics);

        // Collect groups under those topics
        for topic_id in &expanded_topic_ids {
            let groups = db.get_groups_by_topic(topic_id).await?;
            for g in groups {
                topic_group_ids.insert(g.group_id);
            }
        }
    }

    // 3. Union direct + topic-derived group sets
    let all_group_ids: std::collections::HashSet<String> = direct_group_ids
        .union(&topic_group_ids)
        .cloned()
        .collect();

    if all_group_ids.is_empty() {
        return Ok(Vec::new());
    }

    // 4. Fetch recent GroupPost entries from all groups
    let cursor_ts = cursor
        .as_ref()
        .and_then(|c| c.parse::<u64>().ok())
        .unwrap_or(u64::MAX);

    let mut all_posts: Vec<GroupPost> = Vec::new();
    for gid in &all_group_ids {
        // Fetch more than `limit` per group to ensure we have enough after merge
        let posts = db.get_group_posts(gid, limit * 2, None).await?;
        for p in posts {
            if p.posted_at < cursor_ts {
                all_posts.push(p);
            }
        }
    }

    // 5. Sort by posted_at desc, apply limit
    all_posts.sort_by(|a, b| b.posted_at.cmp(&a.posted_at));
    all_posts.truncate(limit);

    // 6. Hydrate with BytescapeItem data
    let mut items = Vec::new();
    for gp in all_posts {
        if let Some(item) = db.get_item_by_id(&gp.item_id).await? {
            items.push(item);
        }
    }
    Ok(items)
}

// ═══════════════════════════════════════════════════════════════════
// Moderation — Approve / Reject / Invite / Ban / Roles / Pin / Remove
// ═══════════════════════════════════════════════════════════════════

/// Approve a pending member (Closed groups). Owner or Moderator only.
///
/// # IPC
/// Tauri command `approve_member`.
///
/// # Side Effects
/// Flips member status from `Pending` → `Active` and increments `member_count`.
#[tauri::command]
pub async fn approve_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    // Caller must be Owner or Moderator
    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        return Err("Only Owner or Moderator can approve members".to_string());
    }

    let mut member = db.get_group_member(&group_id, &member_did).await?
        .ok_or_else(|| format!("Member '{}' not found in group", member_did))?;

    if member.status != MemberStatus::Pending {
        return Err(format!("Member is {:?}, not Pending", member.status));
    }

    member.status = MemberStatus::Active;
    member.joined_at = now_ms();
    db.save_group_member(&member).await?;
    db.update_group_member_count(&group_id, 1).await?;
    Ok(())
}

/// Reject a pending member (deletes the record). Owner or Moderator only.
///
/// # IPC
/// Tauri command `reject_member`.
///
/// # Errors
/// Returns an error if the member is not `Pending` — use `ban_member` for active members.
#[tauri::command]
pub async fn reject_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        return Err("Only Owner or Moderator can reject members".to_string());
    }

    let member = db.get_group_member(&group_id, &member_did).await?
        .ok_or_else(|| format!("Member '{}' not found in group", member_did))?;

    if member.status != MemberStatus::Pending {
        return Err(format!("Member is {:?}, not Pending — use ban_member for active members", member.status));
    }

    db.delete_group_member(&group_id, &member_did).await?;
    Ok(())
}

/// Invite a member to a group. Owner or Moderator only.
/// For Private groups, this is the only way to add members.
///
/// # IPC
/// Tauri command `invite_member`.
///
/// # Side Effects
/// Creates a `GroupMember` record with `Invited` status. The invitee must
/// call `accept_invite` to become `Active`.
#[tauri::command]
pub async fn invite_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        return Err("Only Owner or Moderator can invite members".to_string());
    }

    // Check for existing membership
    if let Some(existing) = db.get_group_member(&group_id, &member_did).await? {
        return Err(format!("Member already exists with status {:?}", existing.status));
    }

    let member = GroupMember {
        group_id,
        member_did,
        role: MemberRole::Member,
        status: MemberStatus::Invited,
        joined_at: now_ms(),
    };
    db.save_group_member(&member).await?;
    Ok(())
}

/// Accept an invitation. Current user must have status Invited.
///
/// # IPC
/// Tauri command `accept_invite`.
///
/// # Side Effects
/// Promotes status from `Invited` → `Active` and increments `member_count`.
#[tauri::command]
pub async fn accept_invite(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let mut member = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You have not been invited to this group".to_string())?;

    if member.status != MemberStatus::Invited {
        return Err(format!("Your status is {:?}, not Invited", member.status));
    }

    member.status = MemberStatus::Active;
    member.joined_at = now_ms();
    db.save_group_member(&member).await?;
    db.update_group_member_count(&group_id, 1).await?;
    Ok(())
}

/// Ban a member from a group. Owner only. Cannot ban the Owner.
/// Banned members cannot re-join (enforced in join_group).
///
/// # IPC
/// Tauri command `ban_member`.
///
/// # Side Effects
/// Sets status to `Banned` and decrements `member_count` if the member was `Active`.
/// The ban is enforced on subsequent `join_group` calls (early return with error).
#[tauri::command]
pub async fn ban_member(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner {
        return Err("Only the Owner can ban members".to_string());
    }

    let mut member = db.get_group_member(&group_id, &member_did).await?
        .ok_or_else(|| format!("Member '{}' not found in group", member_did))?;

    if member.role == MemberRole::Owner {
        return Err("Cannot ban the group Owner".to_string());
    }

    let was_active = member.status == MemberStatus::Active;
    member.status = MemberStatus::Banned;
    db.save_group_member(&member).await?;

    if was_active {
        db.update_group_member_count(&group_id, -1).await?;
    }
    Ok(())
}

/// Change a member's role. Owner only. Cannot change own role.
///
/// # IPC
/// Tauri command `set_member_role`.
///
/// # Params
/// - `role` — the new `MemberRole` (Member, Moderator, or Owner).
///
/// # Constraints
/// - Only the Owner can change roles.
/// - Cannot change your own role (use a transfer_ownership flow instead).
/// - Target member must be `Active`.
#[tauri::command]
pub async fn set_member_role(
    state: State<'_, AppState>,
    group_id: String,
    member_did: String,
    role: MemberRole,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner {
        return Err("Only the Owner can change member roles".to_string());
    }

    if member_did == user_did {
        return Err("Cannot change your own role — use a transfer_ownership flow instead".to_string());
    }

    let mut member = db.get_group_member(&group_id, &member_did).await?
        .ok_or_else(|| format!("Member '{}' not found in group", member_did))?;

    if member.status != MemberStatus::Active {
        return Err(format!("Member must be Active to change role (current: {:?})", member.status));
    }

    member.role = role;
    db.save_group_member(&member).await?;
    Ok(())
}

/// Pin a post in a group. Owner or Moderator only.
///
/// # IPC
/// Tauri command `pin_group_post`.
#[tauri::command]
pub async fn pin_group_post(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        return Err("Only Owner or Moderator can pin posts".to_string());
    }

    db.update_group_post_pin(&group_id, &item_id, true).await
}

/// Unpin a post in a group. Owner or Moderator only.
///
/// # IPC
/// Tauri command `unpin_group_post`.
#[tauri::command]
pub async fn unpin_group_post(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        return Err("Only Owner or Moderator can unpin posts".to_string());
    }

    db.update_group_post_pin(&group_id, &item_id, false).await
}

/// Remove a post from a group. Owner/Moderator can remove any post.
/// Regular members can remove their own posts.
///
/// # IPC
/// Tauri command `remove_group_post`.
///
/// # Authorization
/// - Owner/Moderator: can remove any post in the group.
/// - Member: can only remove posts whose underlying `BytescapeItem.author_did`
///   matches the caller's DID.
#[tauri::command]
pub async fn remove_group_post(
    state: State<'_, AppState>,
    group_id: String,
    item_id: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;

    // Mods/Owners can remove any post; regular members can only remove their own
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        // Check if the underlying item belongs to the caller
        if let Some(item) = db.get_item_by_id(&item_id).await? {
            if item.author_did.as_deref() != Some(&user_did) {
                return Err("You can only remove your own posts".to_string());
            }
        } else {
            return Err(format!("Item '{}' not found", item_id));
        }
    }

    db.delete_group_post(&group_id, &item_id).await
}

/// Get all pending members for a group. Owner or Moderator only.
///
/// # IPC
/// Tauri command `get_pending_members`.
///
/// # Returns
/// A filtered `Vec<GroupMember>` containing only members with `Pending` status.
/// Used by the moderation panel to show the approval queue.
#[tauri::command]
pub async fn get_pending_members(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<Vec<GroupMember>, String> {
    let db = require_db(&state).await?;
    let user_did = current_user_did(&state).await?;

    let caller = db.get_group_member(&group_id, &user_did).await?
        .ok_or_else(|| "You are not a member of this group".to_string())?;
    if caller.role != MemberRole::Owner && caller.role != MemberRole::Moderator {
        return Err("Only Owner or Moderator can view pending members".to_string());
    }

    let members = db.get_group_members(&group_id).await?;
    Ok(members.into_iter().filter(|m| m.status == MemberStatus::Pending).collect())
}

// ═══════════════════════════════════════════════════════════════════
// Seed Defaults
// ═══════════════════════════════════════════════════════════════════

/// Seeds the default topic hierarchy on first launch (when `TOPICS_TABLE` is empty).
///
/// Creates a three-level tree: root categories (Fediverse, Technology, …),
/// sub-categories (Bytescape, Lemmy, Programming, …), and leaf topics
/// (Announcements, Feature Requests, Bug Reports, …).
///
/// Topic IDs are deterministic (`topic:{slug}`) so that seed data is idempotent
/// on a fresh database. However, if topics are manually deleted and this function
/// runs again, it will re-create them.
///
/// # Tech Debt
/// No deduplication check beyond `topics_count() > 0` — if a single custom topic
/// exists, the seed is skipped entirely, even if the defaults are missing.
/// Conversely, calling this on a wiped-then-partially-repopulated DB could
/// create duplicates if a manual topic already has a matching slug.
///
/// # Called From
/// `lib.rs` startup sequence (not a Tauri command — called directly).
pub async fn seed_default_topics(db: &DbManager) -> Result<(), String> {
    // Only seed if the table is empty
    if db.topics_count().await? > 0 {
        return Ok(());
    }

    let now = now_ms();
    let system_did = "did:system:bytescape".to_string();

    // Helper to create topics with deterministic slug-based IDs
    let make = |name: &str, slug: &str, parent: Option<&str>, sort: i32, desc: &str| -> Topic {
        Topic {
            id: format!("topic:{}", slug),
            name: name.to_string(),
            slug: slug.to_string(),
            parent_id: parent.map(|p| format!("topic:{}", p)),
            description: desc.to_string(),
            icon: None,
            sort_order: sort,
            created_by: system_did.clone(),
            created_at: now,
        }
    };

    let topics = vec![
        // Top-level
        make("Fediverse", "fediverse", None, 0, "Federation, decentralization, and the open social web"),
        make("Technology", "technology", None, 1, "Programming, hardware, FOSS, and digital tools"),
        make("Health & Wellness", "health-wellness", None, 2, "Physical health, mental health, fitness, and self-care"),
        make("Art & Creative", "art-creative", None, 3, "Visual art, music, writing, film, and creative expression"),
        make("Gaming", "gaming", None, 4, "Video games, tabletop, esports, and game development"),
        make("Science", "science", None, 5, "Natural sciences, research, space, and discovery"),
        make("News & Politics", "news-politics", None, 6, "Current events, policy, and civic discussion"),
        // Fediverse children
        make("Bytescape", "bytescape", Some("fediverse"), 0, "Bytescape app discussion, feedback, and development"),
        make("Lemmy", "lemmy", Some("fediverse"), 1, "Lemmy instance discussion and community coordination"),
        make("Mastodon", "mastodon", Some("fediverse"), 2, "Mastodon instances, clients, and migration"),
        make("PieFed", "piefed", Some("fediverse"), 3, "PieFed platform discussion"),
        // Bytescape children
        make("Announcements", "announcements", Some("bytescape"), 0, "Official Bytescape announcements"),
        make("Feature Requests", "feature-requests", Some("bytescape"), 1, "Suggest new features and improvements"),
        make("Bug Reports", "bug-reports", Some("bytescape"), 2, "Report bugs and issues"),
        make("Groups", "groups-meta", Some("bytescape"), 3, "Suggest new topics and categories here"),
        // Technology children
        make("Programming", "programming", Some("technology"), 0, "Languages, frameworks, tools, and coding discussion"),
        make("Linux & FOSS", "linux-foss", Some("technology"), 1, "Linux distributions, open source software, and digital freedom"),
        make("Security", "security", Some("technology"), 2, "Infosec, privacy, cryptography, and threat modeling"),
    ];

    for topic in &topics {
        db.save_topic(topic).await?;
    }

    println!("Startup: Seeded {} default topics", topics.len());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Lemmy Integration — AP Community Discovery & Follow
// ═══════════════════════════════════════════════════════════════════

/// Fetches the list of communities from a remote Lemmy instance.
///
/// # IPC
/// Tauri command `discover_lemmy_communities`.
///
/// # Params
/// - `instance_url` — base URL of the Lemmy instance (e.g. `https://lemmy.ml`).
///
/// # Returns
/// A `Vec<RemoteCommunity>` with name, description, subscriber count, etc.
/// The frontend displays these in a discovery dialog for the user to follow.
#[tauri::command]
pub async fn discover_lemmy_communities(
    state: State<'_, AppState>,
    instance_url: String,
) -> Result<Vec<crate::activitypub::lemmy::RemoteCommunity>, String> {
    crate::activitypub::lemmy::discover_communities(
        &state.http_client,
        &instance_url,
    ).await
}

/// Follows a remote Lemmy community by its ActivityPub actor URL.
///
/// # IPC
/// Tauri command `follow_remote_community`.
///
/// # Algorithm
/// 1. Fetches the remote community's AP actor document.
/// 2. Creates a local `LocalGroup` placeholder mirroring the community.
/// 3. Auto-joins the current user as Owner of the local placeholder.
/// 4. Builds a signed `Follow` activity (delivery deferred until relay deployment).
///
/// # Side Effects
/// - Persists a new `LocalGroup` and `GroupMember` record.
/// - Logs the Follow activity to stderr (not yet delivered over HTTP).
///
/// # Limitations
/// Signed HTTP delivery is not implemented — the Follow activity is built but
/// not sent. Federation requires the Bytescape relay to be deployed first.
#[tauri::command]
pub async fn follow_remote_community(
    state: State<'_, AppState>,
    community_actor_url: String,
) -> Result<(), String> {
    let db = require_db(&state).await?;
    let owner_did = current_user_did(&state).await?;

    // Fetch remote community's AP actor document
    let actor = crate::activitypub::lemmy::fetch_community_actor(
        &state.http_client,
        &community_actor_url,
    ).await?;

    // Extract instance from URL
    let instance = url::Url::parse(&community_actor_url)
        .map(|u| u.host_str().unwrap_or("unknown").to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Create local placeholder group entry
    let group = crate::activitypub::lemmy::remote_community_to_local_group(&actor, &instance)?;
    db.save_group(&group).await?;

    // Auto-join the local user as Owner of the placeholder
    let member = crate::models::groups::GroupMember {
        group_id: group.group_id.clone(),
        member_did: owner_did.clone(),
        role: crate::models::groups::MemberRole::Owner,
        status: crate::models::groups::MemberStatus::Active,
        joined_at: now_ms(),
    };
    db.save_group_member(&member).await?;

    // Build the Follow activity (for future signed delivery)
    let local_actor_id = format!("https://bytescape.social/@{}", owner_did);
    let (follow_activity, inbox_url) = crate::activitypub::lemmy::build_community_follow(
        &local_actor_id,
        &actor,
    )?;

    eprintln!(
        "Follow activity built for {} → inbox: {} (delivery pending relay deployment)",
        community_actor_url, inbox_url
    );
    eprintln!("Activity: {}", serde_json::to_string_pretty(&follow_activity).unwrap_or_default());

    Ok(())
}

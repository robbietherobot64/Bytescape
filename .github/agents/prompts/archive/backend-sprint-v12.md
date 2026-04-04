# Backend Sprint v12 — Groups & Topic Hierarchy Foundation (Track A)

**Sprint Goal:** Build the data model, storage layer, and Tauri commands for the hierarchical topic/group system — Bytescape's forum-style community structure. This sprint focuses on local operations. AP federation with Lemmy/PieFed comes in sprint v13.

**Context:** The user wants to defer server/PDS deployment until hardware is ready. The new priority is **groups** — a hierarchical forum system inspired by PieFed's topic model and Lemmy's `Group` actor pattern. Three visibility types: **Public** (searchable, auto-join), **Closed** (searchable, join requires mod approval), **Private** (invite-only, not listed). Topics organize groups into a tree (like old-style forums). Users subscribe at the topic level (get all groups under it) or the individual group level.

**Design context from user:**
- Topics are a hierarchical tree (Health → Mental Health → Anxiety). Admin-created.
- Groups are user-created communities filed under topics. Group creator requests topic placement; admins adjust.
- New topic suggestions: no complex proposal workflow — users post suggestions to a built-in "Fediverse > Bytescape > Groups" forum topic.
- Posts can be shared to another group, to the user's timeline, or to another user via DM.
- The UI is an old-style forum landing page with collapsible topic sections, subscribe buttons, and nested groups. (Frontend sprint handles the UI.)
- Future sprint (v13): AP `Group` actor federation with Lemmy/PieFed. This sprint builds the local foundation.

**HEAD:** `b782bef` on `main` (clean tree). **Build:** 0 tsc errors, 272/272 frontend tests, 207 Rust tests pass.

---

## P1: Data Model — Structs & Enums (🔥 Critical)

**Priority:** 🔥 Critical — everything else depends on these types.

**Task:** Define the core types in a new module `src-tauri/src/models/groups.rs` (and re-export from `models/mod.rs`). All structs must derive `Serialize, Deserialize, Debug, Clone`.

### 1.1 GroupVisibility enum

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum GroupVisibility {
    Public,  // Searchable, auto-join on Follow
    Closed,  // Searchable, join requires mod approval
    Private, // Not listed, invite-only
}
```

### 1.2 MemberRole enum

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MemberRole {
    Owner,
    Moderator,
    Member,
}
```

### 1.3 MemberStatus enum

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MemberStatus {
    Active,
    Pending,  // Awaiting approval (closed groups)
    Invited,  // Invited but hasn't accepted (private groups)
    Banned,
}
```

### 1.4 Topic struct

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Topic {
    pub id: String,           // UUID
    pub name: String,         // "Mental Health"
    pub slug: String,         // "mental-health"
    pub parent_id: Option<String>, // null = top-level topic
    pub description: String,
    pub icon: Option<String>, // emoji or icon identifier
    pub sort_order: i32,      // display order among siblings
    pub created_by: String,   // DID of admin who created it
    pub created_at: u64,      // unix timestamp ms
}
```

### 1.5 TopicNode struct (for tree queries)

```rust
/// Recursive tree node returned by get_topic_tree.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TopicNode {
    #[serde(flatten)]
    pub topic: Topic,
    pub children: Vec<TopicNode>,
    pub groups: Vec<GroupSummary>,  // groups filed under this topic
    pub subscribed: bool,          // whether the current user subscribes to this topic
}
```

### 1.6 LocalGroup struct

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalGroup {
    pub group_id: String,          // UUID
    pub name: String,              // "Mindfulness Daily"
    pub slug: String,              // "mindfulness-daily"
    pub description: String,
    pub visibility: GroupVisibility,
    pub topic_id: String,          // which topic node this group is filed under
    pub owner_did: String,         // DID of group creator
    pub icon_blob: Option<String>, // blob hash or URL for group icon
    pub banner_blob: Option<String>,
    pub rules: Option<String>,     // markdown rules/guidelines
    pub posting_restricted_to_mods: bool,
    pub member_count: u32,         // denormalized count for display
    pub created_at: u64,
    pub updated_at: u64,
}
```

### 1.7 GroupSummary struct (lightweight, for listings)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupSummary {
    pub group_id: String,
    pub name: String,
    pub slug: String,
    pub visibility: GroupVisibility,
    pub member_count: u32,
    pub icon_blob: Option<String>,
}
```

### 1.8 GroupMember struct

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupMember {
    pub group_id: String,
    pub member_did: String,
    pub role: MemberRole,
    pub status: MemberStatus,
    pub joined_at: u64,
}
```

### 1.9 GroupPost struct (tags a BytescapeItem to a group)

```rust
/// Links an existing BytescapeItem to a group. The item itself lives in ITEMS_TABLE.
/// This is a tag/reference, not a copy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupPost {
    pub group_id: String,
    pub item_id: String,
    pub pinned: bool,
    pub shared_by: Option<String>,  // DID if this is a cross-post/share (not the original author)
    pub posted_at: u64,
}
```

### 1.10 TopicSubscription struct

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TopicSubscription {
    pub topic_id: String,
    pub user_did: String,
    pub subscribed_at: u64,
}
```

**Verification:** `cargo check` passes. All types are `Serialize + Deserialize` for redb JSON storage and Tauri command returns.

---

## P2: Redb Tables & CRUD Operations (🔥 Critical)

**Priority:** 🔥 Critical — storage layer for all group data.

**Task:** Add 5 new redb tables to `src-tauri/src/db/mod.rs` and implement CRUD operations in a new `src-tauri/src/db/groups.rs` module.

### 2.1 Table Definitions

Add to `db/mod.rs`:

```rust
pub(crate) const TOPICS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("topics");
pub(crate) const TOPIC_SUBSCRIPTIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("topic_subscriptions");
pub(crate) const GROUPS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("groups");
pub(crate) const GROUP_MEMBERS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("group_members");
pub(crate) const GROUP_POSTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("group_posts");
```

Add table initialization in `DbManager::new()` alongside the existing 13 tables.

### 2.2 Key Design

| Table | Key Format | Value |
|-------|-----------|-------|
| `topics` | `{topic_id}` | JSON `Topic` |
| `topic_subscriptions` | `{user_did}:{topic_id}` | JSON `TopicSubscription` |
| `groups` | `{group_id}` | JSON `LocalGroup` |
| `group_members` | `{group_id}:{member_did}` | JSON `GroupMember` |
| `group_posts` | `{group_id}:{posted_at_zero_padded}:{item_id}` | JSON `GroupPost` |

**Key rationale:**
- `group_posts` key includes zero-padded `posted_at` timestamp so iterating a group's posts yields chronological order (redb iterates keys lexicographically). Use `format!("{}:{:020}:{}", group_id, posted_at, item_id)`.
- `group_members` key includes both IDs for efficient lookup of "is user X a member of group Y?" and prefix-scan of "all members of group Y".
- `topic_subscriptions` key includes `user_did` first for efficient "all topics user X subscribes to" queries.

### 2.3 CRUD Operations

Create `src-tauri/src/db/groups.rs` with `impl DbManager` block. Required methods:

**Topics:**
- `save_topic(&self, topic: &Topic) -> Result<(), String>`
- `get_topic(&self, topic_id: &str) -> Result<Option<Topic>, String>`
- `get_all_topics(&self) -> Result<Vec<Topic>, String>`
- `delete_topic(&self, topic_id: &str) -> Result<(), String>`
- `get_topic_subscriptions(&self, user_did: &str) -> Result<Vec<TopicSubscription>, String>`
- `save_topic_subscription(&self, sub: &TopicSubscription) -> Result<(), String>`
- `delete_topic_subscription(&self, user_did: &str, topic_id: &str) -> Result<(), String>`

**Groups:**
- `save_group(&self, group: &LocalGroup) -> Result<(), String>`
- `get_group(&self, group_id: &str) -> Result<Option<LocalGroup>, String>`
- `get_groups_by_topic(&self, topic_id: &str) -> Result<Vec<LocalGroup>, String>`
- `search_groups(&self, query: &str) -> Result<Vec<LocalGroup>, String>` — case-insensitive name/description search across all Public + Closed groups
- `delete_group(&self, group_id: &str) -> Result<(), String>`
- `update_group_member_count(&self, group_id: &str, delta: i32) -> Result<(), String>` — increment/decrement denormalized count

**Members:**
- `save_group_member(&self, member: &GroupMember) -> Result<(), String>`
- `get_group_member(&self, group_id: &str, member_did: &str) -> Result<Option<GroupMember>, String>`
- `get_group_members(&self, group_id: &str) -> Result<Vec<GroupMember>, String>`
- `get_user_groups(&self, user_did: &str) -> Result<Vec<String>, String>` — all group_ids where user is Active member
- `delete_group_member(&self, group_id: &str, member_did: &str) -> Result<(), String>`

**Group Posts:**
- `save_group_post(&self, post: &GroupPost) -> Result<(), String>`
- `get_group_posts(&self, group_id: &str, limit: usize, cursor: Option<String>) -> Result<Vec<GroupPost>, String>` — paginated, newest first
- `delete_group_post(&self, group_id: &str, item_id: &str) -> Result<(), String>`
- `is_item_in_group(&self, group_id: &str, item_id: &str) -> Result<bool, String>`

**Pattern:** Follow the same JSON serialization pattern used in `db/vault.rs` and `db/contacts.rs` — `serde_json::to_string()` for writes, `serde_json::from_str()` for reads.

**Verification:** `cargo check` + `cargo test`. Add at least 5 unit tests in `db/groups.rs` covering save/get/delete for topics, groups, and members.

---

## P3: Topic Tree Commands (🔥 Critical)

**Priority:** 🔥 Critical — the frontend needs the tree to render the ForumScreen.

**Task:** Create `src-tauri/src/commands/groups.rs` and register commands in `lib.rs`.

### 3.1 Commands

```rust
#[tauri::command]
async fn create_topic(
    state: State<'_, AppState>,
    name: String,
    slug: String,
    parent_id: Option<String>,
    description: String,
    icon: Option<String>,
    sort_order: i32,
) -> Result<Topic, String>
```
- Generate UUID for `id`
- Set `created_by` to `get_owner_did()`
- Set `created_at` to current timestamp
- Save via `db.save_topic()`
- Return the created `Topic`

```rust
#[tauri::command]
async fn update_topic(
    state: State<'_, AppState>,
    topic_id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    sort_order: Option<i32>,
    parent_id: Option<Option<String>>,
) -> Result<Topic, String>
```
- `parent_id`: `Some(None)` = move to root, `Some(Some(id))` = reparent, `None` = don't change

```rust
#[tauri::command]
async fn delete_topic(state: State<'_, AppState>, topic_id: String) -> Result<(), String>
```
- Fail if any groups are still assigned to this topic
- Fail if any child topics exist (must delete children first)

```rust
#[tauri::command]
async fn get_topic_tree(state: State<'_, AppState>) -> Result<Vec<TopicNode>, String>
```
- Fetch all topics from `TOPICS_TABLE`
- Build tree in memory: collect roots (parent_id = None), recursively attach children
- For each topic node, fetch groups via `get_groups_by_topic()` and build `GroupSummary` list
- Check `topic_subscriptions` to set `subscribed` flag
- **Important:** For Private groups, only include in the `groups` list if the current user is a member. Public and Closed groups always appear.
- Sort siblings by `sort_order`

```rust
#[tauri::command]
async fn subscribe_topic(state: State<'_, AppState>, topic_id: String) -> Result<(), String>

#[tauri::command]
async fn unsubscribe_topic(state: State<'_, AppState>, topic_id: String) -> Result<(), String>

#[tauri::command]
async fn get_subscribed_topics(state: State<'_, AppState>) -> Result<Vec<String>, String>
```

### 3.2 Registration

Add all 7 commands to the `.invoke_handler(tauri::generate_handler![...])` in `lib.rs`.

**Verification:** `cargo check` + all existing tests pass. On fresh database, `get_topic_tree` returns the seeded tree with correct hierarchy.

---

## P4: Group CRUD & Search Commands (🔥 Critical)

**Priority:** 🔥 Critical — users need to create and browse groups.

### 4.1 Commands

```rust
#[tauri::command]
async fn create_group(
    state: State<'_, AppState>,
    name: String, slug: String, description: String,
    visibility: GroupVisibility, topic_id: String,
    rules: Option<String>, icon_blob: Option<String>, banner_blob: Option<String>,
) -> Result<LocalGroup, String>
```
- Validate topic_id exists
- Generate UUID for `group_id`
- Set `owner_did` to `get_owner_did()`, `posting_restricted_to_mods = false`, `member_count = 1`
- Save group
- **Auto-create the owner as a member** with `role: Owner, status: Active`
- Return the created group

```rust
#[tauri::command]
async fn update_group(
    state: State<'_, AppState>,
    group_id: String,
    name: Option<String>, description: Option<String>,
    visibility: Option<GroupVisibility>, topic_id: Option<String>,
    rules: Option<String>,
    icon_blob: Option<Option<String>>, banner_blob: Option<Option<String>>,
    posting_restricted_to_mods: Option<bool>,
) -> Result<LocalGroup, String>
```
- Only group owner or moderator can update (check membership)

```rust
#[tauri::command]
async fn delete_group(state: State<'_, AppState>, group_id: String) -> Result<(), String>
```
- Only group owner can delete. Remove all group_members, group_posts, and group itself.

```rust
#[tauri::command]
async fn get_group(state: State<'_, AppState>, group_id: String) -> Result<Option<LocalGroup>, String>
```
- If Private and user is not a member, return None (no info leak)

```rust
#[tauri::command]
async fn list_groups_by_topic(state: State<'_, AppState>, topic_id: String) -> Result<Vec<LocalGroup>, String>
```

```rust
#[tauri::command]
async fn search_groups(state: State<'_, AppState>, query: String) -> Result<Vec<LocalGroup>, String>
```
- Case-insensitive on name + description. Public + Closed only. Limit 50 results.

### 4.2 Registration

Add all 6 commands to `lib.rs` invoke handler.

---

## P5: Membership & Group Feed Commands (High)

**Priority:** High — needed for the frontend subscribe/join UX and group content display.

### 5.1 Membership Commands

```rust
#[tauri::command]
async fn join_group(state: State<'_, AppState>, group_id: String) -> Result<MemberStatus, String>
```
- **Public** → status = `Active`, increment member_count
- **Closed** → status = `Pending` (no count increment until approved)
- **Private** → return error "This group is invite-only"
- Check for duplicate joins

```rust
#[tauri::command]
async fn leave_group(state: State<'_, AppState>, group_id: String) -> Result<(), String>
```
- Cannot leave if you're the owner (must transfer ownership or delete group)

```rust
#[tauri::command]
async fn get_group_members(
    state: State<'_, AppState>, group_id: String, status_filter: Option<MemberStatus>,
) -> Result<Vec<GroupMember>, String>
```
- Only owner/mod can see `Pending` and `Invited` members

```rust
#[tauri::command]
async fn get_user_group_memberships(state: State<'_, AppState>) -> Result<Vec<LocalGroup>, String>
```
- Return all groups where the current user has `status: Active`

### 5.2 Group Feed Commands

```rust
#[tauri::command]
async fn post_to_group(state: State<'_, AppState>, group_id: String, item_id: String) -> Result<(), String>
```
- Verify Active member. If `posting_restricted_to_mods`, verify Owner/Mod role.

```rust
#[tauri::command]
async fn share_to_group(state: State<'_, AppState>, item_id: String, target_group_id: String) -> Result<(), String>
```
- Cross-post: creates a GroupPost with `shared_by: Some(current_user_did)`

```rust
#[tauri::command]
async fn get_group_feed(
    state: State<'_, AppState>, group_id: String, limit: usize, cursor: Option<String>,
) -> Result<Vec<BytescapeItem>, String>
```
- Get GroupPost entries (paginated, newest first), hydrate with BytescapeItem from ITEMS_TABLE

```rust
#[tauri::command]
async fn get_groups_feed(
    state: State<'_, AppState>, limit: usize, cursor: Option<String>,
) -> Result<Vec<BytescapeItem>, String>
```
- **Aggregated** Groups feed:
  1. Get all group_ids from direct memberships
  2. Get all subscribed topic_ids, recursively collect groups under each topic + sub-topics
  3. Union the two group sets
  4. Fetch recent GroupPost entries from all those groups
  5. Merge, sort by `posted_at` desc, apply limit + cursor
  6. Hydrate with BytescapeItem data
- This powers the frontend "Groups" feed tab

### 5.3 Registration

Add all 7 commands to `lib.rs`.

**Verification:** `cargo check` + `cargo test`. Integration test: create topic → create group → join → post → verify `get_group_feed` and `get_groups_feed` return the item.

---

## P6: Seed Data & Default Topic Tree (Medium)

**Priority:** Medium — provides the initial forum structure on fresh install.

**Task:** On first launch (when `TOPICS_TABLE` is empty), seed the default topic tree.

### 6.1 Seed Structure

```
Fediverse                     (sort: 0)
├── Bytescape                 (sort: 0)
│   ├── Announcements         (sort: 0)
│   ├── Feature Requests      (sort: 1)
│   ├── Bug Reports           (sort: 2)
│   └── Groups                (sort: 3)  ← "Suggest new topics/categories here"
├── Lemmy                     (sort: 1)
├── Mastodon                  (sort: 2)
└── PieFed                    (sort: 3)

Technology                    (sort: 1)
├── Programming               (sort: 0)
├── Linux & FOSS              (sort: 1)
└── Security                  (sort: 2)

Health & Wellness             (sort: 2)

Art & Creative                (sort: 3)

Gaming                        (sort: 4)

Science                       (sort: 5)

News & Politics               (sort: 6)
```

### 6.2 When to Seed

Call `seed_default_topics()` during `DbManager::new()` initialization, **only if** `TOPICS_TABLE` is empty (no rows). Existing installations are not affected.

### 6.3 The "Groups" Meta-Topic

"Fediverse > Bytescape > Groups" is where users suggest new topics. It's a regular forum topic — no special workflow. Admins review suggestions and create new top-level topics as needed.

**Verification:** `cargo check` + `cargo test`. On fresh database, `get_topic_tree` returns the seeded tree.

---

## Constraints

- **Track boundary:** Only modify files in `src-tauri/src/`. Do NOT touch `src/` (frontend files).
- **New dependencies:** None required — all operations use existing `serde_json`, `uuid`, `redb` patterns already in the codebase.
- **Build verification:** Run `cargo check` + `cargo test` before EVERY commit. Zero errors.
- **Shared files:** Before editing `lib.rs`, run `git log --oneline -5 -- src-tauri/src/lib.rs` to check for recent cross-agent modifications.
- **Commit format:** `feat(track-a): description`. One logical change per commit.
- **No TODOs:** Implement features fully, don't leave placeholder comments.
- **UUID generation:** Use `uuid::Uuid::new_v4().to_string()` (already in `Cargo.toml`).
- **Timestamps:** Use `std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64` (existing pattern).
- **Update `progress.md`** with your deliverables when done.

---

## What This Sprint Does NOT Include (Future Sprints)

- **AP Group actor federation** (sprint v13) — serving Group actors, handling Follow/Accept/Announce, subscribing to remote Lemmy/PieFed communities
- **Group moderation** (sprint v13) — approve/reject/invite/ban members, pin posts, remove content
- **RSS connector** (sprint v14) — `feed-rs` crate, `connectors/rss.rs`
- **Events & Calendar** (sprint v14+) — `Event` BytescapeType variant
- **Deep profile customization** (sprint v14+)
- **Geolocation features** (B4.1–B4.3) — separate from the topic/group system

//! Redb CRUD operations for Topics, Groups, Members, GroupPosts, and TopicSubscriptions.

use super::*;
use redb::ReadableTable;
use crate::models::groups::*;

impl DbManager {
    // ─── Topics ────────────────────────────────────────────────────

    pub async fn save_topic(&self, topic: &Topic) -> Result<(), String> {
        let serialized = serde_json::to_string(topic).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
            table.insert(topic.id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_topic(&self, topic_id: &str) -> Result<Option<Topic>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
        match table.get(topic_id) {
            Ok(Some(value)) => {
                let topic: Topic = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                Ok(Some(topic))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn get_all_topics(&self) -> Result<Vec<Topic>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
        let mut topics = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = item.map_err(|e| e.to_string())?;
            let topic: Topic = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            topics.push(topic);
        }
        Ok(topics)
    }

    pub async fn delete_topic(&self, topic_id: &str) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
            table.remove(topic_id).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn topics_count(&self) -> Result<usize, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(TOPICS_TABLE).map_err(|e| e.to_string())?;
        Ok(table.len().map_err(|e| e.to_string())? as usize)
    }

    // ─── Topic Subscriptions ───────────────────────────────────────

    pub async fn save_topic_subscription(&self, sub: &TopicSubscription) -> Result<(), String> {
        let key = format!("{}:{}", sub.user_did, sub.topic_id);
        let serialized = serde_json::to_string(sub).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_topic_subscriptions(&self, user_did: &str) -> Result<Vec<TopicSubscription>, String> {
        let prefix = format!("{}:", user_did);
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
        let mut subs = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, value) = item.map_err(|e| e.to_string())?;
            if key.value().starts_with(&prefix) {
                let sub: TopicSubscription = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                subs.push(sub);
            }
        }
        Ok(subs)
    }

    pub async fn delete_topic_subscription(&self, user_did: &str, topic_id: &str) -> Result<(), String> {
        let key = format!("{}:{}", user_did, topic_id);
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
            table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn is_topic_subscribed(&self, user_did: &str, topic_id: &str) -> Result<bool, String> {
        let key = format!("{}:{}", user_did, topic_id);
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(TOPIC_SUBSCRIPTIONS_TABLE).map_err(|e| e.to_string())?;
        match table.get(key.as_str()) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }

    // ─── Groups ────────────────────────────────────────────────────

    pub async fn save_group(&self, group: &LocalGroup) -> Result<(), String> {
        let serialized = serde_json::to_string(group).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
            table.insert(group.group_id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_group(&self, group_id: &str) -> Result<Option<LocalGroup>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
        match table.get(group_id) {
            Ok(Some(value)) => {
                let group: LocalGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                Ok(Some(group))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn get_groups_by_topic(&self, topic_id: &str) -> Result<Vec<LocalGroup>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
        let mut groups = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = item.map_err(|e| e.to_string())?;
            let group: LocalGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            if group.topic_id == topic_id {
                groups.push(group);
            }
        }
        Ok(groups)
    }

    pub async fn get_all_groups(&self) -> Result<Vec<LocalGroup>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
        let mut groups = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = item.map_err(|e| e.to_string())?;
            let group: LocalGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            groups.push(group);
        }
        Ok(groups)
    }

    pub async fn search_groups(&self, query: &str) -> Result<Vec<LocalGroup>, String> {
        let query_lower = query.to_lowercase();
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
        let mut results = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = item.map_err(|e| e.to_string())?;
            let group: LocalGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            // Only search Public + Closed groups (Private groups are not discoverable)
            if group.visibility == GroupVisibility::Private {
                continue;
            }
            if group.name.to_lowercase().contains(&query_lower)
                || group.description.to_lowercase().contains(&query_lower)
            {
                results.push(group);
            }
            if results.len() >= 50 {
                break;
            }
        }
        Ok(results)
    }

    pub async fn delete_group(&self, group_id: &str) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
            table.remove(group_id).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn update_group_member_count(&self, group_id: &str, delta: i32) -> Result<(), String> {
        let db = self.db.write().await;
        // Read current group
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
        let mut group: LocalGroup = match table.get(group_id) {
            Ok(Some(value)) => serde_json::from_str(value.value()).map_err(|e| e.to_string())?,
            Ok(None) => return Err(format!("Group {} not found", group_id)),
            Err(e) => return Err(e.to_string()),
        };
        drop(table);
        drop(read_txn);

        // Update count
        group.member_count = if delta >= 0 {
            group.member_count.saturating_add(delta as u32)
        } else {
            group.member_count.saturating_sub((-delta) as u32)
        };

        // Write back
        let serialized = serde_json::to_string(&group).map_err(|e| e.to_string())?;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUPS_TABLE).map_err(|e| e.to_string())?;
            table.insert(group.group_id.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    // ─── Group Members ─────────────────────────────────────────────

    pub async fn save_group_member(&self, member: &GroupMember) -> Result<(), String> {
        let key = format!("{}:{}", member.group_id, member.member_did);
        let serialized = serde_json::to_string(member).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_group_member(&self, group_id: &str, member_did: &str) -> Result<Option<GroupMember>, String> {
        let key = format!("{}:{}", group_id, member_did);
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
        match table.get(key.as_str()) {
            Ok(Some(value)) => {
                let member: GroupMember = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                Ok(Some(member))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn get_group_members(&self, group_id: &str) -> Result<Vec<GroupMember>, String> {
        let prefix = format!("{}:", group_id);
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
        let mut members = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, value) = item.map_err(|e| e.to_string())?;
            if key.value().starts_with(&prefix) {
                let member: GroupMember = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                members.push(member);
            }
        }
        Ok(members)
    }

    pub async fn get_user_groups(&self, user_did: &str) -> Result<Vec<String>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
        let mut group_ids = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = item.map_err(|e| e.to_string())?;
            let member: GroupMember = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            if member.member_did == user_did && member.status == MemberStatus::Active {
                group_ids.push(member.group_id);
            }
        }
        Ok(group_ids)
    }

    pub async fn delete_group_member(&self, group_id: &str, member_did: &str) -> Result<(), String> {
        let key = format!("{}:{}", group_id, member_did);
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
            table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete all members of a group (used when deleting the group).
    pub async fn delete_all_group_members(&self, group_id: &str) -> Result<usize, String> {
        let prefix = format!("{}:", group_id);
        let db = self.db.write().await;

        // Collect keys to delete
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
        let mut keys_to_delete = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, _) = item.map_err(|e| e.to_string())?;
            if key.value().starts_with(&prefix) {
                keys_to_delete.push(key.value().to_string());
            }
        }
        drop(table);
        drop(read_txn);

        let count = keys_to_delete.len();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUP_MEMBERS_TABLE).map_err(|e| e.to_string())?;
            for key in &keys_to_delete {
                table.remove(key.as_str()).map_err(|e| e.to_string())?;
            }
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }

    // ─── Group Posts ───────────────────────────────────────────────

    pub async fn save_group_post(&self, post: &GroupPost) -> Result<(), String> {
        let key = format!("{}:{:020}:{}", post.group_id, post.posted_at, post.item_id);
        let serialized = serde_json::to_string(post).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get_group_posts(&self, group_id: &str, limit: usize, cursor: Option<String>) -> Result<Vec<GroupPost>, String> {
        let prefix = format!("{}:", group_id);
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;

        // Collect all posts for this group
        let mut posts = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, value) = item.map_err(|e| e.to_string())?;
            if key.value().starts_with(&prefix) {
                let post: GroupPost = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
                posts.push(post);
            }
        }

        // Sort: pinned first, then newest-first within each group
        posts.sort_by(|a, b| {
            b.pinned.cmp(&a.pinned)
                .then_with(|| b.posted_at.cmp(&a.posted_at))
        });

        // Apply cursor: skip entries until we pass the cursor timestamp
        let posts = if let Some(ref cursor_str) = cursor {
            if let Ok(cursor_ts) = cursor_str.parse::<u64>() {
                posts.into_iter().filter(|p| p.posted_at < cursor_ts).collect()
            } else {
                posts
            }
        } else {
            posts
        };

        Ok(posts.into_iter().take(limit).collect())
    }

    pub async fn delete_group_post(&self, group_id: &str, item_id: &str) -> Result<(), String> {
        let prefix = format!("{}:", group_id);
        let suffix = format!(":{}", item_id);
        let db = self.db.write().await;

        // Find the exact key (includes timestamp)
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
        let mut key_to_delete = None;
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, _) = item.map_err(|e| e.to_string())?;
            let k = key.value();
            if k.starts_with(&prefix) && k.ends_with(&suffix) {
                key_to_delete = Some(k.to_string());
                break;
            }
        }
        drop(table);
        drop(read_txn);

        if let Some(key) = key_to_delete {
            let write_txn = db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = write_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
                table.remove(key.as_str()).map_err(|e| e.to_string())?;
            }
            write_txn.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Delete all posts linked to a group (used when deleting the group).
    pub async fn delete_all_group_posts(&self, group_id: &str) -> Result<usize, String> {
        let prefix = format!("{}:", group_id);
        let db = self.db.write().await;

        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
        let mut keys_to_delete = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, _) = item.map_err(|e| e.to_string())?;
            if key.value().starts_with(&prefix) {
                keys_to_delete.push(key.value().to_string());
            }
        }
        drop(table);
        drop(read_txn);

        let count = keys_to_delete.len();
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
            for key in &keys_to_delete {
                table.remove(key.as_str()).map_err(|e| e.to_string())?;
            }
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub async fn is_item_in_group(&self, group_id: &str, item_id: &str) -> Result<bool, String> {
        let prefix = format!("{}:", group_id);
        let suffix = format!(":{}", item_id);
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, _) = item.map_err(|e| e.to_string())?;
            let k = key.value();
            if k.starts_with(&prefix) && k.ends_with(&suffix) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Update the pinned status of a group post.
    pub async fn update_group_post_pin(&self, group_id: &str, item_id: &str, pinned: bool) -> Result<(), String> {
        let prefix = format!("{}:", group_id);
        let suffix = format!(":{}", item_id);
        let db = self.db.write().await;

        // Find the matching key and current value
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
        let mut found_key = None;
        let mut found_post = None;
        for item in table.iter().map_err(|e| e.to_string())? {
            let (key, value) = item.map_err(|e| e.to_string())?;
            let k = key.value();
            if k.starts_with(&prefix) && k.ends_with(&suffix) {
                found_key = Some(k.to_string());
                found_post = Some(serde_json::from_str::<GroupPost>(value.value()).map_err(|e| e.to_string())?);
                break;
            }
        }
        drop(table);
        drop(read_txn);

        let key = found_key.ok_or_else(|| format!("Post '{}' not found in group '{}'", item_id, group_id))?;
        let mut post = found_post.unwrap();
        post.pinned = pinned;

        let serialized = serde_json::to_string(&post).map_err(|e| e.to_string())?;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(GROUP_POSTS_TABLE).map_err(|e| e.to_string())?;
            table.insert(key.as_str(), serialized.as_str()).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn make_db() -> DbManager {
        let tmp = NamedTempFile::new().unwrap();
        DbManager::new(tmp.path().to_path_buf()).unwrap()
    }

    fn make_topic(id: &str, parent: Option<&str>) -> Topic {
        Topic {
            id: id.to_string(),
            name: format!("Topic {}", id),
            slug: format!("topic-{}", id),
            parent_id: parent.map(|s| s.to_string()),
            description: "A test topic".to_string(),
            icon: None,
            sort_order: 0,
            created_by: "did:key:test".to_string(),
            created_at: 1000,
        }
    }

    fn make_group(id: &str, topic_id: &str, vis: GroupVisibility) -> LocalGroup {
        LocalGroup {
            group_id: id.to_string(),
            name: format!("Group {}", id),
            slug: format!("group-{}", id),
            description: "A test group".to_string(),
            visibility: vis,
            topic_id: topic_id.to_string(),
            owner_did: "did:key:owner".to_string(),
            icon_blob: None,
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 1,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn test_topic_crud() {
        let db = make_db().await;
        let topic = make_topic("t1", None);

        // Save
        db.save_topic(&topic).await.unwrap();

        // Get
        let fetched = db.get_topic("t1").await.unwrap().unwrap();
        assert_eq!(fetched.name, "Topic t1");

        // List all
        let all = db.get_all_topics().await.unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        db.delete_topic("t1").await.unwrap();
        assert!(db.get_topic("t1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_topic_hierarchy() {
        let db = make_db().await;
        let root = make_topic("root", None);
        let child = make_topic("child", Some("root"));

        db.save_topic(&root).await.unwrap();
        db.save_topic(&child).await.unwrap();

        let all = db.get_all_topics().await.unwrap();
        assert_eq!(all.len(), 2);

        let child_fetched = db.get_topic("child").await.unwrap().unwrap();
        assert_eq!(child_fetched.parent_id, Some("root".to_string()));
    }

    #[tokio::test]
    async fn test_group_crud() {
        let db = make_db().await;
        let topic = make_topic("t1", None);
        db.save_topic(&topic).await.unwrap();

        let group = make_group("g1", "t1", GroupVisibility::Public);
        db.save_group(&group).await.unwrap();

        let fetched = db.get_group("g1").await.unwrap().unwrap();
        assert_eq!(fetched.name, "Group g1");
        assert_eq!(fetched.topic_id, "t1");

        let by_topic = db.get_groups_by_topic("t1").await.unwrap();
        assert_eq!(by_topic.len(), 1);

        db.delete_group("g1").await.unwrap();
        assert!(db.get_group("g1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_group_search_excludes_private() {
        let db = make_db().await;

        let public = make_group("g1", "t1", GroupVisibility::Public);
        let closed = make_group("g2", "t1", GroupVisibility::Closed);
        let private = make_group("g3", "t1", GroupVisibility::Private);

        db.save_group(&public).await.unwrap();
        db.save_group(&closed).await.unwrap();
        db.save_group(&private).await.unwrap();

        let results = db.search_groups("Group").await.unwrap();
        assert_eq!(results.len(), 2); // Public + Closed only
        assert!(results.iter().all(|g| g.visibility != GroupVisibility::Private));
    }

    #[tokio::test]
    async fn test_member_crud() {
        let db = make_db().await;
        let member = GroupMember {
            group_id: "g1".to_string(),
            member_did: "did:key:alice".to_string(),
            role: MemberRole::Owner,
            status: MemberStatus::Active,
            joined_at: 1000,
        };

        db.save_group_member(&member).await.unwrap();

        let fetched = db.get_group_member("g1", "did:key:alice").await.unwrap().unwrap();
        assert_eq!(fetched.role, MemberRole::Owner);

        let members = db.get_group_members("g1").await.unwrap();
        assert_eq!(members.len(), 1);

        let user_groups = db.get_user_groups("did:key:alice").await.unwrap();
        assert_eq!(user_groups, vec!["g1"]);

        db.delete_group_member("g1", "did:key:alice").await.unwrap();
        assert!(db.get_group_member("g1", "did:key:alice").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_group_posts_chronological() {
        let db = make_db().await;

        let post1 = GroupPost {
            group_id: "g1".to_string(),
            item_id: "item-a".to_string(),
            pinned: false,
            shared_by: None,
            posted_at: 1000,
        };
        let post2 = GroupPost {
            group_id: "g1".to_string(),
            item_id: "item-b".to_string(),
            pinned: false,
            shared_by: None,
            posted_at: 2000,
        };
        let post3 = GroupPost {
            group_id: "g1".to_string(),
            item_id: "item-c".to_string(),
            pinned: false,
            shared_by: Some("did:key:bob".to_string()),
            posted_at: 3000,
        };

        db.save_group_post(&post1).await.unwrap();
        db.save_group_post(&post2).await.unwrap();
        db.save_group_post(&post3).await.unwrap();

        // Newest first
        let posts = db.get_group_posts("g1", 10, None).await.unwrap();
        assert_eq!(posts.len(), 3);
        assert_eq!(posts[0].item_id, "item-c");
        assert_eq!(posts[1].item_id, "item-b");
        assert_eq!(posts[2].item_id, "item-a");

        // Cursor-based pagination
        let page2 = db.get_group_posts("g1", 2, Some("3000".to_string())).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].item_id, "item-b");

        // Delete
        db.delete_group_post("g1", "item-b").await.unwrap();
        let remaining = db.get_group_posts("g1", 10, None).await.unwrap();
        assert_eq!(remaining.len(), 2);

        // is_item_in_group
        assert!(db.is_item_in_group("g1", "item-a").await.unwrap());
        assert!(!db.is_item_in_group("g1", "item-b").await.unwrap());
    }

    #[tokio::test]
    async fn test_topic_subscriptions() {
        let db = make_db().await;
        let sub = TopicSubscription {
            topic_id: "t1".to_string(),
            user_did: "did:key:alice".to_string(),
            subscribed_at: 1000,
        };

        db.save_topic_subscription(&sub).await.unwrap();
        assert!(db.is_topic_subscribed("did:key:alice", "t1").await.unwrap());
        assert!(!db.is_topic_subscribed("did:key:alice", "t2").await.unwrap());

        let subs = db.get_topic_subscriptions("did:key:alice").await.unwrap();
        assert_eq!(subs.len(), 1);

        db.delete_topic_subscription("did:key:alice", "t1").await.unwrap();
        assert!(!db.is_topic_subscribed("did:key:alice", "t1").await.unwrap());
    }

    #[tokio::test]
    async fn test_member_count_update() {
        let db = make_db().await;
        let group = make_group("g1", "t1", GroupVisibility::Public);
        db.save_group(&group).await.unwrap();
        assert_eq!(db.get_group("g1").await.unwrap().unwrap().member_count, 1);

        db.update_group_member_count("g1", 3).await.unwrap();
        assert_eq!(db.get_group("g1").await.unwrap().unwrap().member_count, 4);

        db.update_group_member_count("g1", -2).await.unwrap();
        assert_eq!(db.get_group("g1").await.unwrap().unwrap().member_count, 2);

        // Saturating subtraction — can't go below 0
        db.update_group_member_count("g1", -100).await.unwrap();
        assert_eq!(db.get_group("g1").await.unwrap().unwrap().member_count, 0);
    }

    #[tokio::test]
    async fn test_delete_all_group_members() {
        let db = make_db().await;
        for i in 0..3 {
            let m = GroupMember {
                group_id: "g1".to_string(),
                member_did: format!("did:key:user{}", i),
                role: MemberRole::Member,
                status: MemberStatus::Active,
                joined_at: 1000,
            };
            db.save_group_member(&m).await.unwrap();
        }
        assert_eq!(db.get_group_members("g1").await.unwrap().len(), 3);

        let deleted = db.delete_all_group_members("g1").await.unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(db.get_group_members("g1").await.unwrap().len(), 0);
    }

    // ── Sprint v13 P1: Moderation tests ──

    #[tokio::test]
    async fn test_approve_member_flow() {
        let db = make_db().await;
        let group = make_group("g1", "t1", GroupVisibility::Closed);
        db.save_group(&group).await.unwrap();

        // Create a pending member
        let pending = GroupMember {
            group_id: "g1".to_string(),
            member_did: "did:key:alice".to_string(),
            role: MemberRole::Member,
            status: MemberStatus::Pending,
            joined_at: 1000,
        };
        db.save_group_member(&pending).await.unwrap();

        // Verify pending
        let fetched = db.get_group_member("g1", "did:key:alice").await.unwrap().unwrap();
        assert_eq!(fetched.status, MemberStatus::Pending);

        // "Approve" by updating status
        let mut approved = fetched;
        approved.status = MemberStatus::Active;
        approved.joined_at = 2000;
        db.save_group_member(&approved).await.unwrap();
        db.update_group_member_count("g1", 1).await.unwrap();

        let result = db.get_group_member("g1", "did:key:alice").await.unwrap().unwrap();
        assert_eq!(result.status, MemberStatus::Active);
        assert_eq!(db.get_group("g1").await.unwrap().unwrap().member_count, 2);
    }

    #[tokio::test]
    async fn test_reject_member_removes_record() {
        let db = make_db().await;

        let pending = GroupMember {
            group_id: "g1".to_string(),
            member_did: "did:key:bob".to_string(),
            role: MemberRole::Member,
            status: MemberStatus::Pending,
            joined_at: 1000,
        };
        db.save_group_member(&pending).await.unwrap();
        assert!(db.get_group_member("g1", "did:key:bob").await.unwrap().is_some());

        // Reject = delete
        db.delete_group_member("g1", "did:key:bob").await.unwrap();
        assert!(db.get_group_member("g1", "did:key:bob").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_invite_accept_flow() {
        let db = make_db().await;
        let group = make_group("g1", "t1", GroupVisibility::Private);
        db.save_group(&group).await.unwrap();

        // Invite
        let invited = GroupMember {
            group_id: "g1".to_string(),
            member_did: "did:key:carol".to_string(),
            role: MemberRole::Member,
            status: MemberStatus::Invited,
            joined_at: 1000,
        };
        db.save_group_member(&invited).await.unwrap();

        let fetched = db.get_group_member("g1", "did:key:carol").await.unwrap().unwrap();
        assert_eq!(fetched.status, MemberStatus::Invited);

        // Accept
        let mut accepted = fetched;
        accepted.status = MemberStatus::Active;
        accepted.joined_at = 2000;
        db.save_group_member(&accepted).await.unwrap();
        db.update_group_member_count("g1", 1).await.unwrap();

        let result = db.get_group_member("g1", "did:key:carol").await.unwrap().unwrap();
        assert_eq!(result.status, MemberStatus::Active);
        assert_eq!(db.get_group("g1").await.unwrap().unwrap().member_count, 2);
    }

    #[tokio::test]
    async fn test_ban_prevents_rejoin() {
        let db = make_db().await;

        // Create banned member
        let banned = GroupMember {
            group_id: "g1".to_string(),
            member_did: "did:key:dave".to_string(),
            role: MemberRole::Member,
            status: MemberStatus::Banned,
            joined_at: 1000,
        };
        db.save_group_member(&banned).await.unwrap();

        // Verify banned status persists (join_group would check this before allowing)
        let fetched = db.get_group_member("g1", "did:key:dave").await.unwrap().unwrap();
        assert_eq!(fetched.status, MemberStatus::Banned);

        // get_user_groups should NOT include banned memberships (Active filter)
        let groups = db.get_user_groups("did:key:dave").await.unwrap();
        assert!(groups.is_empty());
    }

    #[tokio::test]
    async fn test_pin_unpin_post() {
        let db = make_db().await;

        let post = GroupPost {
            group_id: "g1".to_string(),
            item_id: "item-x".to_string(),
            pinned: false,
            shared_by: None,
            posted_at: 1000,
        };
        db.save_group_post(&post).await.unwrap();

        // Pin
        db.update_group_post_pin("g1", "item-x", true).await.unwrap();
        let posts = db.get_group_posts("g1", 10, None).await.unwrap();
        assert_eq!(posts[0].pinned, true);

        // Unpin
        db.update_group_post_pin("g1", "item-x", false).await.unwrap();
        let posts = db.get_group_posts("g1", 10, None).await.unwrap();
        assert_eq!(posts[0].pinned, false);
    }

    #[tokio::test]
    async fn test_role_change() {
        let db = make_db().await;

        let member = GroupMember {
            group_id: "g1".to_string(),
            member_did: "did:key:eve".to_string(),
            role: MemberRole::Member,
            status: MemberStatus::Active,
            joined_at: 1000,
        };
        db.save_group_member(&member).await.unwrap();

        // Promote to Moderator
        let mut updated = db.get_group_member("g1", "did:key:eve").await.unwrap().unwrap();
        assert_eq!(updated.role, MemberRole::Member);
        updated.role = MemberRole::Moderator;
        db.save_group_member(&updated).await.unwrap();

        let result = db.get_group_member("g1", "did:key:eve").await.unwrap().unwrap();
        assert_eq!(result.role, MemberRole::Moderator);
    }

    #[tokio::test]
    async fn test_pinned_posts_sort_first() {
        let db = make_db().await;

        let post1 = GroupPost { group_id: "g1".to_string(), item_id: "old".to_string(), pinned: false, shared_by: None, posted_at: 1000 };
        let post2 = GroupPost { group_id: "g1".to_string(), item_id: "new".to_string(), pinned: false, shared_by: None, posted_at: 3000 };
        let post3 = GroupPost { group_id: "g1".to_string(), item_id: "pinned-old".to_string(), pinned: true, shared_by: None, posted_at: 500 };

        db.save_group_post(&post1).await.unwrap();
        db.save_group_post(&post2).await.unwrap();
        db.save_group_post(&post3).await.unwrap();

        let posts = db.get_group_posts("g1", 10, None).await.unwrap();
        // Pinned first, then newest
        assert_eq!(posts[0].item_id, "pinned-old"); // pinned=true
        assert_eq!(posts[1].item_id, "new");        // pinned=false, newest
        assert_eq!(posts[2].item_id, "old");         // pinned=false, oldest
    }
}

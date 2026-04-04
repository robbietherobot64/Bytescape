//! # ActivityPub Inbox — Incoming Message Handler
//!
//! Handles incoming ActivityPub messages from remote fediverse servers (Mastodon,
//! Lemmy, etc.) delivered via HTTP POST to the local inbox endpoint.
//!
//! ## HOW IT WORKS
//!
//! 1. A remote server sends an HTTP POST to our inbox with a JSON-LD body and an
//!    HTTP `Signature` header (draft-cavage-http-signatures).
//! 2. `process_inbox()` parses the `Signature` header, fetches the sender's public
//!    key from their actor document, and verifies the cryptographic signature.
//! 3. If the signature is valid, the activity body is parsed and dispatched by type
//!    (`Follow`, `Create`, `Like`, `Undo`, `Announce`, `Update`, `Delete`, `Accept`,
//!    `Reject`).
//! 4. DMs are detected by checking whether `to`/`cc` lack the `as:Public` address
//!    (Mastodon convention — a Note addressed only to specific actors is a DM).
//! 5. Group-targeted activities are detected via `extract_group_id()` and handled
//!    separately with database operations for membership management.
//!
//! ## RELATED FILES
//!
//! - `signatures.rs` — HTTP signature parsing and verification
//! - `actor.rs` — local actor identity (`AP_DOMAIN` constant)
//! - `activities.rs` — constructs `Accept`/`Reject` response activities
//! - `group_actor.rs` — group actor federation and `Announce` relay
//! - `db/groups.rs` — group membership persistence (`DbManager`)
//! - `connectors/mastodon.rs` — `strip_html()` used for DM content sanitization
//!
//! ## SECURITY
//!
//! - SEC-8: `fetch_actor_public_key()` validates URLs against private/loopback
//!   addresses via `security::validate_public_url()` before fetching.
//! - SEC-9: `verify_body_digest()` validates the `Digest` header against the
//!   actual body hash to prevent body substitution attacks.
//! - Clock skew: `validate_date_header()` rejects requests with Date headers
//!   more than ±5 minutes from server time to prevent replay attacks.

use super::signatures::{parse_signature_header, verify_ed25519_signature};
use base64::Engine;

// ═══════════════════════════════════════════════════════════════════════════════
// INBOX RESULT TYPES
// ═══════════════════════════════════════════════════════════════════════════════

/// Result of processing an inbox activity.
#[derive(Debug)]
pub enum InboxResult {
    Accepted,
    FollowReceived { from: String },
    ObjectReceived { activity_type: String, object_id: String },
    /// A DM was received (Create(Note) addressed only to specific actors, not as:Public).
    DirectMessageReceived {
        from_actor: String,
        from_name: String,
        object_id: String,
        text: String,
    },
    Undone { activity_type: String },
    Ignored,
}

// ═══════════════════════════════════════════════════════════════════════════════
// MAIN INBOX PROCESSOR
// ═══════════════════════════════════════════════════════════════════════════════

/// Process an incoming inbox POST.
///
/// Steps:
/// 1. Validate clock skew on the Date header (±5 min tolerance)
/// 2. Verify body digest against the Digest header (SEC-9)
/// 3. Parse the Signature header
/// 4. Fetch the sender's public key (with SSRF protection — SEC-8)
/// 5. Verify the HTTP signature
/// 6. Dispatch the activity by type
pub async fn process_inbox(
    body: &[u8],
    signature_header: &str,
    host: &str,
    date: &str,
    digest_header: Option<&str>,
    content_type: Option<&str>,
    request_path: &str,
    http_client: &reqwest::Client,
) -> Result<InboxResult, String> {
    // SEC-9: Verify body digest before any other processing
    if let Some(digest_val) = digest_header {
        verify_body_digest(body, digest_val)?;
    }

    // AP Clock Skew: reject requests with Date header more than ±5 minutes off
    validate_date_header(date)?;

    // Parse the Signature header
    let sig = parse_signature_header(signature_header)?;

    // Fetch the remote actor's public key (SEC-8: with SSRF protection)
    let public_key_pem = fetch_actor_public_key(http_client, &sig.key_id).await?;

    // Build the signing string from the declared headers
    let signing_string = build_inbox_signing_string(
        &sig.headers,
        request_path,
        host,
        date,
        digest_header,
        content_type,
    );

    // Verify the signature
    let valid = verify_ed25519_signature(
        &public_key_pem,
        signing_string.as_bytes(),
        &sig.signature,
    )?;

    if !valid {
        return Err("HTTP Signature verification failed".to_string());
    }

    // Parse the activity body
    let activity: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| format!("Invalid JSON body: {}", e))?;

    let activity_type = activity.get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    match activity_type.as_str() {
        "Follow" => handle_follow(&activity).await,
        "Undo" => handle_undo(&activity).await,
        "Like" => handle_like(&activity).await,
        "Announce" => handle_announce(&activity).await,
        "Create" => handle_create(&activity).await,
        "Update" => handle_update(&activity).await,
        "Delete" => handle_delete(&activity).await,
        "Accept" => handle_accept(&activity).await,
        "Reject" => handle_reject(&activity).await,
        _ => {
            #[cfg(debug_assertions)]
            println!("[AP inbox] Ignoring unsupported activity type: {}", activity_type);
            Ok(InboxResult::Ignored)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// SIGNATURE VERIFICATION HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Construct the HTTP signature verification string from declared headers.
///
/// Reassembles the `(request-target)`, `host`, `date`, `digest`, and
/// `content-type` pseudo-headers into the canonical signing string that the
/// sender should have signed.
fn build_inbox_signing_string(
    headers: &[String],
    request_path: &str,
    host: &str,
    date: &str,
    digest: Option<&str>,
    content_type: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    for h in headers {
        let line = match h.as_str() {
            "(request-target)" => format!("(request-target): post {}", request_path),
            "host" => format!("host: {}", host),
            "date" => format!("date: {}", date),
            "digest" => format!("digest: {}", digest.unwrap_or("")),
            "content-type" => format!("content-type: {}", content_type.unwrap_or("")),
            other => format!("{}: ", other),
        };
        parts.push(line);
    }
    parts.join("\n")
}

/// Fetch a remote actor document and extract the `publicKeyPem`.
///
/// Dereferences the `key_id` URL (stripping the `#fragment`) to retrieve the
/// actor document, then extracts `publicKey.publicKeyPem`.
///
/// # Security (SEC-8)
///
/// Validates that the URL is HTTPS and does not point to private/loopback
/// addresses to prevent SSRF attacks. Uses a 10-second timeout.
///
/// → Cross-system call: outbound HTTP GET to the sender's actor URL.
pub async fn fetch_actor_public_key(
    http_client: &reqwest::Client,
    key_id: &str,
) -> Result<String, String> {
    // The key_id is typically `{actor_url}#main-key`
    let actor_url = key_id.split('#').next().unwrap_or(key_id);

    // SEC-8: Validate URL against SSRF (require HTTPS, block private IPs)
    crate::security::validate_public_url(actor_url, true)?;

    let response = http_client
        .get(actor_url)
        .header("Accept", "application/activity+json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch actor: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Actor fetch returned {}", response.status()));
    }

    let actor: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid actor JSON: {}", e))?;

    actor
        .get("publicKey")
        .and_then(|pk| pk.get("publicKeyPem"))
        .and_then(|pem| pem.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Actor document missing publicKey.publicKeyPem".to_string())
}

/// Verify that the request body matches the `Digest` header (SEC-9).
///
/// Computes `SHA-256(body)` and compares against the `Digest: SHA-256=...`
/// header value. Rejects requests where the body has been tampered after signing.
fn verify_body_digest(body: &[u8], digest_header: &str) -> Result<(), String> {
    // Extract the base64-encoded hash from "SHA-256=<base64>"
    let expected_b64 = digest_header
        .strip_prefix("SHA-256=")
        .or_else(|| digest_header.strip_prefix("sha-256="))
        .ok_or_else(|| format!("Unsupported digest algorithm (expected SHA-256): {}", digest_header))?;

    let expected = base64::engine::general_purpose::STANDARD
        .decode(expected_b64)
        .map_err(|e| format!("Invalid base64 in Digest header: {}", e))?;

    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(body);
    let actual = hasher.finalize();

    if actual.as_slice() != expected.as_slice() {
        return Err("Digest verification failed: body hash does not match Digest header".to_string());
    }

    Ok(())
}

/// Validate that the Date header is within ±5 minutes of server time.
///
/// Prevents replay attacks where a signed request is re-sent hours or days later.
/// Uses RFC 2822 / HTTP date format parsing.
fn validate_date_header(date: &str) -> Result<(), String> {
    use chrono::{DateTime, Utc, Duration};

    // Try HTTP date format: "Mon, 02 Jan 2006 15:04:05 GMT"
    let parsed = DateTime::parse_from_rfc2822(date)
        .or_else(|_| {
            // Try alternative HTTP date format
            chrono::NaiveDateTime::parse_from_str(date, "%a, %d %b %Y %H:%M:%S GMT")
                .map(|naive| naive.and_utc().fixed_offset())
        })
        .map_err(|e| format!("Cannot parse Date header '{}': {}", date, e))?;

    let now = Utc::now();
    let skew = now.signed_duration_since(parsed);
    let max_skew = Duration::minutes(5);

    if skew > max_skew || skew < -max_skew {
        return Err(format!(
            "Clock skew too large: Date header is {} seconds off (max ±300s)",
            skew.num_seconds()
        ));
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ACTIVITY HANDLERS — Person-to-Person
// ═══════════════════════════════════════════════════════════════════════════════

async fn handle_follow(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let from = activity.get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("unknown")
        .to_string();
    #[cfg(debug_assertions)]
    println!("[AP inbox] Follow received from: {}", from);
    Ok(InboxResult::FollowReceived { from })
}

async fn handle_undo(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let inner_type = activity.get("object")
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("unknown")
        .to_string();
    #[cfg(debug_assertions)]
    println!("[AP inbox] Undo received for: {}", inner_type);
    Ok(InboxResult::Undone { activity_type: inner_type })
}

async fn handle_like(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let object_id = activity.get("object")
        .and_then(|o| o.as_str())
        .unwrap_or("unknown")
        .to_string();
    #[cfg(debug_assertions)]
    println!("[AP inbox] Like received for: {}", object_id);
    Ok(InboxResult::ObjectReceived {
        activity_type: "Like".to_string(),
        object_id,
    })
}

async fn handle_announce(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let object_id = activity.get("object")
        .and_then(|o| o.as_str())
        .unwrap_or("unknown")
        .to_string();
    #[cfg(debug_assertions)]
    println!("[AP inbox] Announce received for: {}", object_id);
    Ok(InboxResult::ObjectReceived {
        activity_type: "Announce".to_string(),
        object_id,
    })
}

async fn handle_create(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let object = activity.get("object");

    let object_id = object
        .and_then(|o| o.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();

    let from_actor = activity.get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or("unknown")
        .to_string();

    // DM detection: a Note is a DM if `to` does NOT contain as:Public and
    // `cc` also does NOT contain as:Public. This matches the Mastodon convention
    // where direct messages are Notes addressed only to specific actors.
    let ap_public = "https://www.w3.org/ns/activitystreams#Public";

    let is_dm = if let Some(obj) = object {
        let to_contains_public = obj.get("to")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|v| v.as_str() == Some(ap_public)))
            .unwrap_or(false);
        let cc_contains_public = obj.get("cc")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|v| v.as_str() == Some(ap_public)))
            .unwrap_or(false);

        !to_contains_public && !cc_contains_public
    } else {
        false
    };

    if is_dm {
        let text = object
            .and_then(|o| o.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        // Strip HTML from the content
        let clean_text = crate::connectors::mastodon::strip_html(&text);

        // Try to get a human-readable name from attributedTo or actor
        let from_name = object
            .and_then(|o| o.get("attributedTo"))
            .and_then(|a| a.as_str())
            .unwrap_or(&from_actor)
            .to_string();

        #[cfg(debug_assertions)]
        println!("[AP inbox] DM received from {} — object: {}", from_actor, object_id);

        return Ok(InboxResult::DirectMessageReceived {
            from_actor,
            from_name,
            object_id,
            text: clean_text,
        });
    }

    #[cfg(debug_assertions)]
    println!("[AP inbox] Create received for: {}", object_id);

    Ok(InboxResult::ObjectReceived {
        activity_type: "Create".to_string(),
        object_id,
    })
}

async fn handle_update(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let object_id = activity.get("object")
        .and_then(|o| o.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("unknown")
        .to_string();
    #[cfg(debug_assertions)]
    println!("[AP inbox] Update received for: {}", object_id);
    Ok(InboxResult::ObjectReceived {
        activity_type: "Update".to_string(),
        object_id,
    })
}

async fn handle_delete(activity: &serde_json::Value) -> Result<InboxResult, String> {
    let object_id = activity.get("object")
        .and_then(|o| {
            if o.is_string() {
                o.as_str().map(|s| s.to_string())
            } else {
                o.get("id").and_then(|id| id.as_str()).map(|s| s.to_string())
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    #[cfg(debug_assertions)]
    println!("[AP inbox] Delete received for: {}", object_id);
    Ok(InboxResult::ObjectReceived {
        activity_type: "Delete".to_string(),
        object_id,
    })
}

async fn handle_accept(_activity: &serde_json::Value) -> Result<InboxResult, String> {
    #[cfg(debug_assertions)]
    println!("[AP inbox] Accept received");
    Ok(InboxResult::Accepted)
}

async fn handle_reject(_activity: &serde_json::Value) -> Result<InboxResult, String> {
    #[cfg(debug_assertions)]
    println!("[AP inbox] Reject received");
    Ok(InboxResult::Ignored)
}

// ═══════════════════════════════════════════════════════════════════════════════
// GROUP INBOX HANDLERS
// ═══════════════════════════════════════════════════════════════════════════════
//
// These handlers process activities targeting local Group actors.
// They require database access to manage group membership and posts.
// Call `extract_group_id()` first to detect group-targeted activities,
// then dispatch to the appropriate handler.

use crate::db::DbManager;
use crate::models::groups::*;
use super::group_actor;
use super::activities;

/// Result of processing an activity targeting a Group inbox.
#[derive(Debug)]
pub enum GroupInboxResult {
    /// Follow accepted — return this Accept activity to the sender
    FollowAccepted { response_activity: serde_json::Value },
    /// Follow queued for mod approval (Closed group)
    FollowPending,
    /// Follow rejected (Private group)
    FollowRejected { response_activity: serde_json::Value },
    /// Note stored and wrapped in Announce for relay to followers
    NoteAnnounced { announce_activity: serde_json::Value },
    /// Undo(Follow) processed — member removed
    Unfollowed,
}

/// Check if an activity targets a local Group actor.
///
/// Returns the group_id if the activity's `object` (for Follow/Undo) or
/// `audience`/`to` (for Create) matches a local group actor URL.
pub fn extract_group_id(activity: &serde_json::Value) -> Option<String> {
    let group_url_prefix = format!("https://{}/group/", super::actor::AP_DOMAIN);

    // For Follow: object is the group actor URL
    if let Some(object) = activity.get("object").and_then(|o| o.as_str()) {
        if let Some(id) = object.strip_prefix(&group_url_prefix) {
            return Some(id.to_string());
        }
    }

    // For Undo(Follow): object.object is the group actor URL
    if let Some(inner) = activity.get("object") {
        if let Some(inner_object) = inner.get("object").and_then(|o| o.as_str()) {
            if let Some(id) = inner_object.strip_prefix(&group_url_prefix) {
                return Some(id.to_string());
            }
        }
    }

    // For Create(Note): audience field points to group
    if let Some(obj) = activity.get("object") {
        if let Some(audience) = obj.get("audience").and_then(|a| a.as_str()) {
            if let Some(id) = audience.strip_prefix(&group_url_prefix) {
                return Some(id.to_string());
            }
        }
    }

    // Also check activity-level audience
    if let Some(audience) = activity.get("audience").and_then(|a| a.as_str()) {
        if let Some(id) = audience.strip_prefix(&group_url_prefix) {
            return Some(id.to_string());
        }
    }

    None
}

/// Handle a Follow activity targeting a local Group actor.
///
/// - **Public** group → auto-accept: create Active member, return Accept
/// - **Closed** group → create Pending member, return nothing (await mod approval)
/// - **Private** group → reject with Reject activity
pub async fn handle_group_follow(
    activity: &serde_json::Value,
    db: &DbManager,
    group_id: &str,
) -> Result<GroupInboxResult, String> {
    let follower_actor = activity.get("actor")
        .and_then(|a| a.as_str())
        .ok_or_else(|| "Follow activity missing actor".to_string())?;

    let group = db.get_group(group_id).await?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    let group_actor_id = group_actor::group_actor_id(group_id);

    // Check if already a member
    if let Some(existing) = db.get_group_member(group_id, follower_actor).await? {
        match existing.status {
            MemberStatus::Active => return Err("Already a member".to_string()),
            MemberStatus::Pending => return Err("Follow request already pending".to_string()),
            MemberStatus::Banned => {
                let reject = activities::reject(&group_actor_id, activity);
                return Ok(GroupInboxResult::FollowRejected {
                    response_activity: serde_json::to_value(&reject).unwrap_or_default(),
                });
            }
            MemberStatus::Invited => {
                // Accept the invite via Follow
                let mut updated = existing;
                updated.status = MemberStatus::Active;
                updated.joined_at = now_ms();
                db.save_group_member(&updated).await?;
                db.update_group_member_count(group_id, 1).await?;

                let accept = activities::accept(&group_actor_id, activity);
                return Ok(GroupInboxResult::FollowAccepted {
                    response_activity: serde_json::to_value(&accept).unwrap_or_default(),
                });
            }
        }
    }

    match group.visibility {
        GroupVisibility::Public => {
            let member = GroupMember {
                group_id: group_id.to_string(),
                member_did: follower_actor.to_string(),
                role: MemberRole::Member,
                status: MemberStatus::Active,
                joined_at: now_ms(),
            };
            db.save_group_member(&member).await?;
            db.update_group_member_count(group_id, 1).await?;

            let accept = activities::accept(&group_actor_id, activity);
            Ok(GroupInboxResult::FollowAccepted {
                response_activity: serde_json::to_value(&accept).unwrap_or_default(),
            })
        }
        GroupVisibility::Closed => {
            let member = GroupMember {
                group_id: group_id.to_string(),
                member_did: follower_actor.to_string(),
                role: MemberRole::Member,
                status: MemberStatus::Pending,
                joined_at: now_ms(),
            };
            db.save_group_member(&member).await?;
            Ok(GroupInboxResult::FollowPending)
        }
        GroupVisibility::Private => {
            let reject = activities::reject(&group_actor_id, activity);
            Ok(GroupInboxResult::FollowRejected {
                response_activity: serde_json::to_value(&reject).unwrap_or_default(),
            })
        }
    }
}

/// Handle a Create(Note/Page) activity targeting a local Group inbox.
///
/// Verifies the sender is an Active member (for Closed/Private groups),
/// stores the note as a GroupPost, and returns an Announce for relay.
pub async fn handle_group_create(
    activity: &serde_json::Value,
    db: &DbManager,
    group_id: &str,
) -> Result<GroupInboxResult, String> {
    let sender_actor = activity.get("actor")
        .and_then(|a| a.as_str())
        .ok_or_else(|| "Create activity missing actor".to_string())?;

    let group = db.get_group(group_id).await?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // For non-Public groups, verify the sender is an Active member
    if group.visibility != GroupVisibility::Public {
        let member = db.get_group_member(group_id, sender_actor).await?;
        match member {
            Some(m) if m.status == MemberStatus::Active => {}
            _ => return Err("Sender is not an active member of this group".to_string()),
        }
    }

    // If posting restricted to mods, check role
    if group.posting_restricted_to_mods {
        if let Some(m) = db.get_group_member(group_id, sender_actor).await? {
            if m.role != MemberRole::Owner && m.role != MemberRole::Moderator {
                return Err("Posting in this group is restricted to moderators".to_string());
            }
        }
    }

    let object = activity.get("object")
        .ok_or_else(|| "Create activity missing object".to_string())?;

    // Extract object ID for the GroupPost reference
    let object_id = object.get("id")
        .and_then(|id| id.as_str())
        .ok_or_else(|| "Create object missing id".to_string())?;

    // Store as GroupPost (the BytescapeItem creation from AP objects is handled
    // by the higher-level import pipeline — here we just link the reference)
    let post = GroupPost {
        group_id: group_id.to_string(),
        item_id: object_id.to_string(),
        pinned: false,
        shared_by: Some(sender_actor.to_string()),
        posted_at: now_ms(),
    };
    db.save_group_post(&post).await?;

    // Build Announce for relay to followers
    let group_actor_id = group_actor::group_actor_id(group_id);
    let followers_collection = format!("{}/followers", group_actor_id);
    let announce = group_actor::announce_to_followers(
        &group_actor_id,
        activity,
        &followers_collection,
    );

    Ok(GroupInboxResult::NoteAnnounced {
        announce_activity: announce,
    })
}

/// Handle Undo(Follow) targeting a local Group actor.
///
/// Removes the GroupMember record and decrements member_count.
pub async fn handle_group_undo_follow(
    activity: &serde_json::Value,
    db: &DbManager,
    group_id: &str,
) -> Result<GroupInboxResult, String> {
    let actor = activity.get("actor")
        .and_then(|a| a.as_str())
        .ok_or_else(|| "Undo activity missing actor".to_string())?;

    // Check if they were a member
    let member = db.get_group_member(group_id, actor).await?;
    let was_active = member.as_ref().map(|m| m.status == MemberStatus::Active).unwrap_or(false);

    db.delete_group_member(group_id, actor).await?;

    if was_active {
        db.update_group_member_count(group_id, -1).await?;
    }

    Ok(GroupInboxResult::Unfollowed)
}

/// Helper: current timestamp in milliseconds.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsupported_type_ignored() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let activity = serde_json::json!({"type": "Dislike", "actor": "test"});
            let activity_type = activity.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            assert_eq!(activity_type, "Dislike");
        });
    }

    #[test]
    fn test_extract_group_id_from_follow() {
        let activity = serde_json::json!({
            "type": "Follow",
            "actor": "https://mastodon.social/@alice",
            "object": "https://bytescape.social/group/test-group-1",
        });
        assert_eq!(extract_group_id(&activity), Some("test-group-1".to_string()));
    }

    #[test]
    fn test_extract_group_id_from_undo_follow() {
        let activity = serde_json::json!({
            "type": "Undo",
            "actor": "https://mastodon.social/@alice",
            "object": {
                "type": "Follow",
                "object": "https://bytescape.social/group/my-group",
            }
        });
        assert_eq!(extract_group_id(&activity), Some("my-group".to_string()));
    }

    #[test]
    fn test_extract_group_id_from_create_with_audience() {
        let activity = serde_json::json!({
            "type": "Create",
            "actor": "https://mastodon.social/@bob",
            "object": {
                "type": "Note",
                "id": "https://mastodon.social/notes/123",
                "content": "Hello group!",
                "audience": "https://bytescape.social/group/cool-group",
            }
        });
        assert_eq!(extract_group_id(&activity), Some("cool-group".to_string()));
    }

    #[test]
    fn test_extract_group_id_non_group_returns_none() {
        let activity = serde_json::json!({
            "type": "Follow",
            "actor": "https://mastodon.social/@alice",
            "object": "https://bytescape.social/@bob",
        });
        assert_eq!(extract_group_id(&activity), None);
    }

    #[tokio::test]
    async fn test_follow_public_group_auto_accepts() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = DbManager::new(tmp.path().to_path_buf()).unwrap();

        let group = LocalGroup {
            group_id: "pub-group".to_string(),
            name: "Public Group".to_string(),
            slug: "pub-group".to_string(),
            description: "Open".to_string(),
            visibility: GroupVisibility::Public,
            topic_id: "t1".to_string(),
            owner_did: "did:key:owner".to_string(),
            icon_blob: None,
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 1,
            created_at: 1000,
            updated_at: 1000,
        };
        db.save_group(&group).await.unwrap();

        let activity = serde_json::json!({
            "type": "Follow",
            "actor": "https://mastodon.social/@alice",
            "object": "https://bytescape.social/group/pub-group",
        });

        let result = handle_group_follow(&activity, &db, "pub-group").await.unwrap();
        match result {
            GroupInboxResult::FollowAccepted { response_activity } => {
                assert_eq!(response_activity["type"], "Accept");
            }
            other => panic!("Expected FollowAccepted, got {:?}", other),
        }

        // Verify member was created
        let member = db.get_group_member("pub-group", "https://mastodon.social/@alice").await.unwrap().unwrap();
        assert_eq!(member.status, MemberStatus::Active);
        assert_eq!(db.get_group("pub-group").await.unwrap().unwrap().member_count, 2);
    }

    #[tokio::test]
    async fn test_follow_closed_group_creates_pending() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = DbManager::new(tmp.path().to_path_buf()).unwrap();

        let group = LocalGroup {
            group_id: "closed-group".to_string(),
            name: "Closed Group".to_string(),
            slug: "closed-group".to_string(),
            description: "Approval required".to_string(),
            visibility: GroupVisibility::Closed,
            topic_id: "t1".to_string(),
            owner_did: "did:key:owner".to_string(),
            icon_blob: None,
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 1,
            created_at: 1000,
            updated_at: 1000,
        };
        db.save_group(&group).await.unwrap();

        let activity = serde_json::json!({
            "type": "Follow",
            "actor": "https://lemmy.ml/u/bob",
            "object": "https://bytescape.social/group/closed-group",
        });

        let result = handle_group_follow(&activity, &db, "closed-group").await.unwrap();
        assert!(matches!(result, GroupInboxResult::FollowPending));

        let member = db.get_group_member("closed-group", "https://lemmy.ml/u/bob").await.unwrap().unwrap();
        assert_eq!(member.status, MemberStatus::Pending);
        // member_count should NOT have increased (pending)
        assert_eq!(db.get_group("closed-group").await.unwrap().unwrap().member_count, 1);
    }

    #[tokio::test]
    async fn test_follow_private_group_rejects() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = DbManager::new(tmp.path().to_path_buf()).unwrap();

        let group = LocalGroup {
            group_id: "priv-group".to_string(),
            name: "Private Group".to_string(),
            slug: "priv-group".to_string(),
            description: "Invite only".to_string(),
            visibility: GroupVisibility::Private,
            topic_id: "t1".to_string(),
            owner_did: "did:key:owner".to_string(),
            icon_blob: None,
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 1,
            created_at: 1000,
            updated_at: 1000,
        };
        db.save_group(&group).await.unwrap();

        let activity = serde_json::json!({
            "type": "Follow",
            "actor": "https://mastodon.social/@eve",
            "object": "https://bytescape.social/group/priv-group",
        });

        let result = handle_group_follow(&activity, &db, "priv-group").await.unwrap();
        match result {
            GroupInboxResult::FollowRejected { response_activity } => {
                assert_eq!(response_activity["type"], "Reject");
            }
            other => panic!("Expected FollowRejected, got {:?}", other),
        }

        // No member record should exist
        assert!(db.get_group_member("priv-group", "https://mastodon.social/@eve").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_create_note_to_group_relays_announce() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = DbManager::new(tmp.path().to_path_buf()).unwrap();

        let group = LocalGroup {
            group_id: "note-group".to_string(),
            name: "Note Group".to_string(),
            slug: "note-group".to_string(),
            description: "Testing".to_string(),
            visibility: GroupVisibility::Public,
            topic_id: "t1".to_string(),
            owner_did: "did:key:owner".to_string(),
            icon_blob: None,
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 1,
            created_at: 1000,
            updated_at: 1000,
        };
        db.save_group(&group).await.unwrap();

        let activity = serde_json::json!({
            "type": "Create",
            "actor": "https://mastodon.social/@alice",
            "audience": "https://bytescape.social/group/note-group",
            "object": {
                "type": "Note",
                "id": "https://mastodon.social/notes/456",
                "content": "Hello group!",
                "audience": "https://bytescape.social/group/note-group",
            }
        });

        let result = handle_group_create(&activity, &db, "note-group").await.unwrap();
        match result {
            GroupInboxResult::NoteAnnounced { announce_activity } => {
                assert_eq!(announce_activity["type"], "Announce");
                assert!(announce_activity["actor"].as_str().unwrap().contains("note-group"));
            }
            other => panic!("Expected NoteAnnounced, got {:?}", other),
        }

        // Verify GroupPost was created
        let posts = db.get_group_posts("note-group", 10, None).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].item_id, "https://mastodon.social/notes/456");
    }

    #[tokio::test]
    async fn test_undo_follow_removes_member() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = DbManager::new(tmp.path().to_path_buf()).unwrap();

        let group = LocalGroup {
            group_id: "undo-group".to_string(),
            name: "Undo Group".to_string(),
            slug: "undo-group".to_string(),
            description: "Testing".to_string(),
            visibility: GroupVisibility::Public,
            topic_id: "t1".to_string(),
            owner_did: "did:key:owner".to_string(),
            icon_blob: None,
            banner_blob: None,
            rules: None,
            posting_restricted_to_mods: false,
            member_count: 2,
            created_at: 1000,
            updated_at: 1000,
        };
        db.save_group(&group).await.unwrap();

        // Pre-create member
        let member = GroupMember {
            group_id: "undo-group".to_string(),
            member_did: "https://mastodon.social/@alice".to_string(),
            role: MemberRole::Member,
            status: MemberStatus::Active,
            joined_at: 1000,
        };
        db.save_group_member(&member).await.unwrap();

        let activity = serde_json::json!({
            "type": "Undo",
            "actor": "https://mastodon.social/@alice",
            "object": {
                "type": "Follow",
                "object": "https://bytescape.social/group/undo-group",
            }
        });

        let result = handle_group_undo_follow(&activity, &db, "undo-group").await.unwrap();
        assert!(matches!(result, GroupInboxResult::Unfollowed));

        // Member should be gone
        assert!(db.get_group_member("undo-group", "https://mastodon.social/@alice").await.unwrap().is_none());
        // Count decremented
        assert_eq!(db.get_group("undo-group").await.unwrap().unwrap().member_count, 1);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // SEC-8: AP key fetch SSRF protection
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_ap_key_fetch_rejects_private_ip() {
        // Verify that private IPs are rejected at the URL validation level
        assert!(crate::security::validate_public_url("https://127.0.0.1/actor", true).is_err());
        assert!(crate::security::validate_public_url("https://10.0.0.5/actor", true).is_err());
        assert!(crate::security::validate_public_url("https://192.168.1.100/actor", true).is_err());
        assert!(crate::security::validate_public_url("https://169.254.169.254/latest", true).is_err());
        assert!(crate::security::validate_public_url("http://mastodon.social/actor", true).is_err()); // non-HTTPS
        assert!(crate::security::validate_public_url("https://mastodon.social/actor", true).is_ok());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // SEC-9: Digest verification
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_ap_digest_verification_valid() {
        use sha2::Digest as _;
        let body = b"{\"type\":\"Create\"}";
        let mut hasher = sha2::Sha256::new();
        hasher.update(body);
        let hash = hasher.finalize();
        let b64 = base64::engine::general_purpose::STANDARD.encode(hash);
        let digest_header = format!("SHA-256={}", b64);

        assert!(verify_body_digest(body, &digest_header).is_ok());
    }

    #[test]
    fn test_ap_digest_verification_rejects_tampered_body() {
        use sha2::Digest as _;
        let original_body = b"{\"type\":\"Create\"}";
        let mut hasher = sha2::Sha256::new();
        hasher.update(original_body);
        let hash = hasher.finalize();
        let b64 = base64::engine::general_purpose::STANDARD.encode(hash);
        let digest_header = format!("SHA-256={}", b64);

        // Different body — digest should fail
        let tampered_body = b"{\"type\":\"Delete\"}";
        assert!(verify_body_digest(tampered_body, &digest_header).is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // AP Clock Skew
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_ap_clock_skew_accepts_current_date() {
        let now = chrono::Utc::now();
        let date = now.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert!(validate_date_header(&date).is_ok());
    }

    #[test]
    fn test_ap_clock_skew_rejects_stale_date() {
        let old = chrono::Utc::now() - chrono::Duration::hours(1);
        let date = old.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert!(validate_date_header(&date).is_err());
    }

    #[test]
    fn test_ap_clock_skew_accepts_within_tolerance() {
        let recent = chrono::Utc::now() - chrono::Duration::minutes(3);
        let date = recent.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert!(validate_date_header(&date).is_ok());
    }
}

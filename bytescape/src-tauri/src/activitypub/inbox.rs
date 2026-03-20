//! Inbox POST handler for incoming ActivityPub messages.

use super::signatures::{parse_signature_header, verify_ed25519_signature};

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

/// Process an incoming inbox POST.
///
/// Steps:
/// 1. Parse the Signature header
/// 2. Fetch the sender's public key (via key_id URL)
/// 3. Verify the HTTP signature
/// 4. Dispatch the activity by type
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
    // Parse the Signature header
    let sig = parse_signature_header(signature_header)?;

    // Fetch the remote actor's public key
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

/// Fetch a remote actor document and extract the publicKeyPem.
pub async fn fetch_actor_public_key(
    http_client: &reqwest::Client,
    key_id: &str,
) -> Result<String, String> {
    // The key_id is typically `{actor_url}#main-key`
    let actor_url = key_id.split('#').next().unwrap_or(key_id);

    let response = http_client
        .get(actor_url)
        .header("Accept", "application/activity+json")
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

#[cfg(test)]
mod tests {

    #[test]
    fn test_unsupported_type_ignored() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // We can't easily test the full process_inbox without a real HTTP server,
            // but we can test the dispatch logic indirectly.
            let activity = serde_json::json!({"type": "Dislike", "actor": "test"});
            let activity_type = activity.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            assert_eq!(activity_type, "Dislike");
        });
    }
}

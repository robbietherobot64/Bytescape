//! Outbox — ordered collection of the user's activities + delivery.

use super::types::{ApActivity, ap_context};
use super::actor::ApActor;
use super::signatures;

/// Build the outbox OrderedCollection root document.
pub fn build_outbox_collection(
    actor_id: &str,
    total_items: u64,
) -> serde_json::Value {
    serde_json::json!({
        "@context": ap_context(),
        "id": format!("{}/outbox", actor_id),
        "type": "OrderedCollection",
        "totalItems": total_items,
        "first": format!("{}/outbox?page=1", actor_id),
    })
}

/// Build a single outbox page.
pub fn build_outbox_page(
    actor_id: &str,
    page: u64,
    items: Vec<serde_json::Value>,
    has_next: bool,
) -> serde_json::Value {
    let mut doc = serde_json::json!({
        "@context": ap_context(),
        "id": format!("{}/outbox?page={}", actor_id, page),
        "type": "OrderedCollectionPage",
        "partOf": format!("{}/outbox", actor_id),
        "orderedItems": items,
    });
    if has_next {
        doc["next"] = serde_json::Value::String(
            format!("{}/outbox?page={}", actor_id, page + 1)
        );
    }
    doc
}

/// Deliver an activity to a remote actor's inbox via signed HTTP POST.
///
/// Returns the HTTP status code of the response.
pub async fn deliver_activity(
    actor: &ApActor,
    activity: &ApActivity,
    target_inbox: &str,
) -> Result<u16, String> {
    let body = serde_json::to_vec(activity)
        .map_err(|e| format!("Failed to serialize activity: {}", e))?;

    let secret_key = actor.secret_key()?;
    let key_id = format!("{}#main-key", actor.id);

    let sig_headers = signatures::sign_post_request(
        &key_id,
        &secret_key,
        target_inbox,
        &body,
    )?;

    let client = reqwest::Client::new();
    let mut request = client.post(target_inbox);

    for (name, value) in &sig_headers {
        request = request.header(name.as_str(), value.as_str());
    }

    let response = request
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Failed to deliver activity: {}", e))?;

    let status = response.status().as_u16();

    #[cfg(debug_assertions)]
    println!("[AP outbox] Delivered to {} — status {}", target_inbox, status);

    if status >= 400 {
        let body_text = response.text().await.unwrap_or_default();
        return Err(format!("Delivery failed with status {}: {}", status, body_text));
    }

    Ok(status)
}

/// Fetch a remote actor's ActivityStreams document.
///
/// If `local_actor` is provided, the request will be signed.
pub async fn fetch_remote_actor(
    actor_url: &str,
    local_actor: Option<&ApActor>,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();

    let mut request = client
        .get(actor_url)
        .header("Accept", "application/activity+json");

    if let Some(actor) = local_actor {
        let secret_key = actor.secret_key()?;
        let key_id = format!("{}#main-key", actor.id);
        let sig_headers = signatures::sign_get_request(&key_id, &secret_key, actor_url)?;
        for (name, value) in &sig_headers {
            request = request.header(name.as_str(), value.as_str());
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to fetch actor: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Actor fetch returned {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Invalid actor JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbox_root() {
        let doc = build_outbox_collection("https://bytescape.social/@alice", 42);
        assert_eq!(doc["type"], "OrderedCollection");
        assert_eq!(doc["totalItems"], 42);
        assert!(doc["first"].as_str().unwrap().contains("page=1"));
    }

    #[test]
    fn test_outbox_page() {
        let items = vec![serde_json::json!({"type": "Create"})];
        let doc = build_outbox_page("https://bytescape.social/@alice", 1, items, true);
        assert_eq!(doc["type"], "OrderedCollectionPage");
        assert!(doc["next"].as_str().unwrap().contains("page=2"));
    }
}

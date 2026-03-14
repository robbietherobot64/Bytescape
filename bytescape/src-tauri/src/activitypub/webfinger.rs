//! WebFinger — acct: resource resolution for AP actor discovery (RFC 7033).

use serde::{Deserialize, Serialize};
use super::actor::AP_DOMAIN;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFingerResponse {
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    pub links: Vec<WebFingerLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFingerLink {
    pub rel: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
}

/// Generate a WebFinger JRD response for a local actor.
pub fn generate_webfinger(username: &str) -> WebFingerResponse {
    let actor_url = format!("https://{}/@{}", AP_DOMAIN, username);
    WebFingerResponse {
        subject: format!("acct:{}@{}", username, AP_DOMAIN),
        aliases: Some(vec![actor_url.clone()]),
        links: vec![
            WebFingerLink {
                rel: "self".to_string(),
                link_type: Some("application/activity+json".to_string()),
                href: Some(actor_url.clone()),
            },
            WebFingerLink {
                rel: "http://webfinger.net/rel/profile-page".to_string(),
                link_type: Some("text/html".to_string()),
                href: Some(actor_url),
            },
        ],
    }
}

/// Parse an `acct:user@domain` resource string into (username, domain).
pub fn parse_acct_resource(resource: &str) -> Result<(String, String), String> {
    let acct = resource.strip_prefix("acct:")
        .ok_or("Resource must start with 'acct:'")?;
    let parts: Vec<&str> = acct.split('@').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid acct format: {}", acct));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Resolve a remote WebFinger resource.
pub async fn resolve_webfinger(
    http_client: &reqwest::Client,
    resource: &str,
) -> Result<WebFingerResponse, String> {
    let (_, domain) = parse_acct_resource(resource)?;
    let url = format!(
        "https://{}/.well-known/webfinger?resource={}",
        domain,
        urlencoding::encode(resource),
    );

    let response = http_client
        .get(&url)
        .header("Accept", "application/jrd+json")
        .send()
        .await
        .map_err(|e| format!("WebFinger request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("WebFinger returned {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Invalid WebFinger JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_webfinger() {
        let wf = generate_webfinger("alice");
        assert_eq!(wf.subject, "acct:alice@bytescape.social");
        assert_eq!(wf.links.len(), 2);
        assert_eq!(wf.links[0].rel, "self");
        assert!(wf.links[0].href.as_ref().unwrap().contains("@alice"));
    }

    #[test]
    fn test_parse_valid_acct() {
        let (user, domain) = parse_acct_resource("acct:alice@example.com").unwrap();
        assert_eq!(user, "alice");
        assert_eq!(domain, "example.com");
    }

    #[test]
    fn test_parse_invalid_acct() {
        assert!(parse_acct_resource("alice@example.com").is_err());
        assert!(parse_acct_resource("acct:alice").is_err());
    }

    #[test]
    fn test_webfinger_serialization() {
        let wf = generate_webfinger("bob");
        let json = serde_json::to_value(&wf).unwrap();
        assert!(json["subject"].as_str().unwrap().starts_with("acct:bob@"));
        assert!(json["links"].is_array());
    }
}

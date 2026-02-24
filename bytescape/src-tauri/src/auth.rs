use serde::{Deserialize, Serialize};
use reqwest::Client;
const XRPC_URL: &str = "https://bsky.social/xrpc";

#[derive(Debug, Serialize)]
struct LoginRequest {
    identifier: String,
    password: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Session {
    pub did: String,
    pub handle: String,
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
}

#[derive(Clone)]
pub struct AuthManager {
    client: Client,
}

impl AuthManager {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn login(&self, identifier: String, password: String) -> Result<Session, String> {
        let url = format!("{}/com.atproto.server.createSession", XRPC_URL);
        
        let response = self.client.post(&url)
            .json(&LoginRequest { identifier, password })
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Login failed: {}", error_text));
        }

        let session = response.json::<Session>()
            .await
            .map_err(|e| format!("Failed to parse session: {}", e))?;

        Ok(session)
    }

    pub async fn refresh_session(&self, refresh_jwt: String) -> Result<Session, String> {
        let url = format!("{}/com.atproto.server.refreshSession", XRPC_URL);
        
        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", refresh_jwt))
            .send()
            .await
            .map_err(|e| format!("Refresh request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Session refresh failed: {}", error_text));
        }

        let session = response.json::<Session>()
            .await
            .map_err(|e| format!("Failed to parse refreshed session: {}", e))?;

        Ok(session)
    }
}

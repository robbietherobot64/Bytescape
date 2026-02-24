use serde::{Deserialize, Serialize};
use reqwest::Client;
use tokio::sync::Mutex;

/// Fallback XRPC URL for non-proxied calls (handle resolution)
const PUBLIC_XRPC: &str = "https://bsky.social/xrpc";
const CHAT_PROXY: &str = "did:web:api.bsky.chat#bsky_chat";
const PLC_DIRECTORY: &str = "https://plc.directory";

// ── Public types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BskyConvo {
    pub id: String,
    pub members: Vec<BskyMember>,
    #[serde(rename = "lastMessage")]
    pub last_message: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BskyMember {
    pub did: String,
    pub handle: Option<String>,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BskyGraphResponse {
    pub subject: BskyMember,
    pub follows: Option<Vec<BskyMember>>,
    pub followers: Option<Vec<BskyMember>>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BskyMessage {
    pub id: String,
    #[serde(default)]
    pub text: String,
    pub sender: BskyMember,
    #[serde(rename = "sentAt", default)]
    pub sent_at: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
}

// ── Private request/response types ──

#[derive(Debug, Deserialize)]
struct GetConvoForMembersResponse {
    convo: BskyConvo,
}

#[derive(Debug, Deserialize)]
struct ListConvosResponse {
    convos: Vec<BskyConvo>,
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GetMessagesResponse {
    messages: Vec<serde_json::Value>, // Mixed types: messages + delete refs
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileRecord {
    #[serde(rename = "$type")]
    pub record_type: String, // usually app.bsky.actor.profile
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "displayName", default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<serde_json::Value>,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GetRecordResponse {
    value: ProfileRecord,
    cid: String,
    uri: String,
}

#[derive(Debug, Serialize)]
struct PutRecordRequest {
    repo: String,
    collection: String,
    rkey: String,
    record: ProfileRecord,
    #[serde(rename = "swapRecord", default)]
    swap_record: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    #[serde(rename = "convoId")]
    convo_id: String,
    message: MessageInput,
}

#[derive(Debug, Serialize)]
struct MessageInput {
    text: String,
}

#[derive(Debug, Deserialize)]
struct DidDocument {
    service: Vec<DidService>,
}

#[derive(Debug, Deserialize)]
struct DidService {
    #[serde(rename = "type")]
    service_type: String,
    #[serde(rename = "serviceEndpoint")]
    service_endpoint: String,
}

// ── ChatClient ──

pub struct ChatClient {
    client: Client,
    /// Cached PDS URL for the authenticated user
    pds_url: Mutex<Option<String>>,
}

impl ChatClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            pds_url: Mutex::new(None),
        }
    }

    /// Get the user's PDS URL, resolving from their DID document if needed.
    pub(crate) async fn get_pds_url(&self, user_did: &str) -> Result<String, String> {
        // Check cache first
        {
            let cached = self.pds_url.lock().await;
            if let Some(ref url) = *cached {
                return Ok(url.clone());
            }
        }

        // Resolve from PLC directory
        let url = format!("{}/{}", PLC_DIRECTORY, user_did);
        println!("ChatClient: Resolving PDS from {}", url);

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("PLC directory lookup failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("PLC directory returned {}", response.status()));
        }

        let doc: DidDocument = response.json()
            .await
            .map_err(|e| format!("Failed to parse DID document: {}", e))?;

        let pds = doc.service.iter()
            .find(|s| s.service_type == "AtprotoPersonalDataServer")
            .ok_or("No PDS found in DID document")?;

        let pds_url = pds.service_endpoint.clone();
        println!("ChatClient: User PDS is {}", pds_url);

        // Cache it
        {
            let mut cached = self.pds_url.lock().await;
            *cached = Some(pds_url.clone());
        }

        Ok(pds_url)
    }

    /// Resolve a Bluesky handle (e.g. "alice.bsky.social") to a DID.
    /// If the input already looks like a DID, returns it as-is.
    pub async fn resolve_handle(&self, handle_or_did: &str) -> Result<String, String> {
        let clean = handle_or_did.trim().trim_start_matches('@');

        // Already a DID
        if clean.starts_with("did:") {
            return Ok(clean.to_string());
        }

        let url = format!("{}/com.atproto.identity.resolveHandle?handle={}", PUBLIC_XRPC, clean);

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("resolveHandle request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("resolveHandle failed for '{}': {}", clean, error_text));
        }

        #[derive(Deserialize)]
        struct ResolveResponse { did: String }

        let parsed: ResolveResponse = response.json()
            .await
            .map_err(|e| format!("Failed to parse DID: {}", e))?;

        Ok(parsed.did)
    }

    pub async fn get_follows(&self, user_did: &str) -> Result<Vec<BskyMember>, String> {
        let url = format!("{}/app.bsky.graph.getFollows?actor={}", PUBLIC_XRPC, user_did);
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("getFollows failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("getFollows failed with status: {}", response.status()));
        }

        let parsed: BskyGraphResponse = response.json().await.map_err(|e| e.to_string())?;
        Ok(parsed.follows.unwrap_or_default())
    }

    pub async fn get_followers(&self, user_did: &str) -> Result<Vec<BskyMember>, String> {
        let url = format!("{}/app.bsky.graph.getFollowers?actor={}", PUBLIC_XRPC, user_did);
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("getFollowers failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("getFollowers failed with status: {}", response.status()));
        }

        let parsed: BskyGraphResponse = response.json().await.map_err(|e| e.to_string())?;
        Ok(parsed.followers.unwrap_or_default())
    }

    /// Get or create a conversation with another user by their DID.
    pub async fn get_or_create_convo(&self, access_jwt: &str, user_did: &str, member_did: &str) -> Result<BskyConvo, String> {
        let pds = self.get_pds_url(user_did).await?;
        let url = format!("{}/xrpc/chat.bsky.convo.getConvoForMembers?members={}", pds, member_did);

        let response = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .header("atproto-proxy", CHAT_PROXY)
            .send()
            .await
            .map_err(|e| format!("Chat request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("getConvoForMembers failed: {}", error_text));
        }

        let parsed: GetConvoForMembersResponse = response.json()
            .await
            .map_err(|e| format!("Failed to parse convo: {}", e))?;

        Ok(parsed.convo)
    }

    /// Send a message to a Bluesky DM conversation.
    pub async fn send_dm(&self, access_jwt: &str, user_did: &str, convo_id: &str, text: &str) -> Result<BskyMessage, String> {
        let pds = self.get_pds_url(user_did).await?;
        let url = format!("{}/xrpc/chat.bsky.convo.sendMessage", pds);

        let body = SendMessageRequest {
            convo_id: convo_id.to_string(),
            message: MessageInput {
                text: text.to_string(),
            },
        };

        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .header("atproto-proxy", CHAT_PROXY)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("sendMessage request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("sendMessage failed: {}", error_text));
        }

        let body = response.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        println!("ChatClient: sendMessage response: {}", &body[..body.len().min(500)]);
        
        let msg: BskyMessage = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse message: {} — body: {}", e, &body[..body.len().min(300)]))?;

        Ok(msg)
    }

    /// List all conversations for the authenticated user.
    pub async fn list_convos(&self, access_jwt: &str, user_did: &str) -> Result<Vec<BskyConvo>, String> {
        println!("ChatClient: Listing conversations for {}", user_did);
        let pds = self.get_pds_url(user_did).await?;
        let url = format!("{}/xrpc/chat.bsky.convo.listConvos", pds);

        let response = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .header("atproto-proxy", CHAT_PROXY)
            .send()
            .await
            .map_err(|e| format!("listConvos request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("listConvos failed: {}", error_text));
        }

        let parsed: ListConvosResponse = response.json()
            .await
            .map_err(|e| format!("Failed to parse convos: {}", e))?;

        Ok(parsed.convos)
    }

    /// Get messages from a specific conversation.
    pub async fn get_messages(&self, access_jwt: &str, user_did: &str, convo_id: &str, cursor: Option<String>) -> Result<(Vec<BskyMessage>, Option<String>), String> {
        // println!("ChatClient: Getting messages for convo {} (cursor: {:?})", convo_id, cursor);
        let pds = self.get_pds_url(user_did).await?;
        let mut url = format!("{}/xrpc/chat.bsky.convo.getMessages?convoId={}&limit=100", pds, convo_id);
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let response = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .header("atproto-proxy", CHAT_PROXY)
            .send()
            .await
            .map_err(|e| format!("getMessages request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("getMessages failed: {}", error_text));
        }

        let body = response.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        
        // Debug first 100 chars
        // println!("ChatClient: getMessages response: {}", &body[..body.len().min(100)]);

        let parsed: GetMessagesResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse messages: {} — body: {}", e, &body[..body.len().min(500)]))?;

        // Filter to only actual messages (not deletions, etc.)
        let messages: Vec<BskyMessage> = parsed.messages
            .into_iter()
            .filter_map(|v| serde_json::from_value::<BskyMessage>(v).ok())
            .collect();

        Ok((messages, parsed.cursor))
    }

    /// Get the user's profile record (app.bsky.actor.profile)
    pub async fn get_profile_record(&self, access_jwt: &str, user_did: &str) -> Result<(ProfileRecord, String), String> {
        let pds = self.get_pds_url(user_did).await?;
        let url = format!("{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=app.bsky.actor.profile&rkey=self", pds, user_did);
        
        let response = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .send()
            .await
            .map_err(|e| format!("getRecord request failed: {}", e))?;

        if !response.status().is_success() {
             return Err(format!("getRecord failed: {}", response.status()));
        }

        let parsed: GetRecordResponse = response.json().await.map_err(|e| e.to_string())?;
        Ok((parsed.value, parsed.cid))
    }

    /// Update the user's profile record
    pub async fn put_profile_record(&self, access_jwt: &str, user_did: &str, record: ProfileRecord, swap_cid: Option<String>) -> Result<(), String> {
        let pds = self.get_pds_url(user_did).await?;
        let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);

        let body = PutRecordRequest {
            repo: user_did.to_string(),
            collection: "app.bsky.actor.profile".to_string(),
            rkey: "self".to_string(),
            record,
            swap_record: swap_cid,
        };

        let response = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", access_jwt))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("putRecord request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("putRecord failed: {}", error_text));
        }

        Ok(())
    }
}

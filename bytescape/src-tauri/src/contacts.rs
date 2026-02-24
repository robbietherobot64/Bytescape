use std::sync::Arc;
use crate::db::{DbManager, Contact};
use crate::chat::ChatClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct DeviceRecord {
    #[serde(rename = "nodeId")]
    node_id: String,
}

#[derive(Debug, Deserialize)]
struct GetRecordResponse {
    value: DeviceRecord,
}

pub struct ContactManager {
    db: DbManager,
    chat: Arc<ChatClient>,
}

impl ContactManager {
    pub fn new(db: DbManager, chat: Arc<ChatClient>) -> Self {
        Self { db, chat }
    }

    pub async fn sync_contacts(&self, user_did: &str, access_jwt: &str) -> Result<(), String> {
        let follows = self.chat.get_follows(user_did).await?;
        let followers = self.chat.get_followers(user_did).await?;
        
        let mut all_contacts = follows;
        for f in followers {
            if !all_contacts.iter().any(|c| c.did == f.did) {
                all_contacts.push(f);
            }
        }

        for member in all_contacts {
            let existing = self.db.get_contact(&member.did).await?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            
            let mut contact = if let Some(c) = existing {
                c
            } else {
                Contact {
                    did: member.did.clone(),
                    handle: member.handle.clone().unwrap_or_else(|| member.did.clone()),
                    p2p_capable: false,
                    node_id: None,
                    last_polled_at: 0,
                }
            };
            
            if let Some(h) = member.handle {
                contact.handle = h;
            }
            
            // Periodically poll capability (every 5 minutes during dev)
            if now - contact.last_polled_at > 300 {
                if let Ok(Some(node_id)) = self.check_p2p_capability(&member.did, access_jwt).await {
                    contact.p2p_capable = true;
                    contact.node_id = Some(node_id);
                } else {
                    contact.p2p_capable = false;
                    contact.node_id = None;
                }
                contact.last_polled_at = now;
            }
            
            self.db.save_contact(&contact).await?;
        }
        
        Ok(())
    }

    async fn check_p2p_capability(&self, did: &str, access_jwt: &str) -> Result<Option<String>, String> {
        // Look for bytescape-pkarr:<node_id> in the description
        match self.chat.get_profile_record(access_jwt, did).await {
            Ok((record, _cid)) => {
                if let Some(desc) = record.description {
                    println!("ContactManager: Checking bio for {}: {:?}", did, &desc[..desc.len().min(200)]);
                    if let Some(start) = desc.find("bytescape-pkarr:") {
                        let rest = &desc[start + 16..]; // "bytescape-pkarr:" is 16 chars
                        // Extract node ID: take chars until whitespace or any non-base32 character
                        let node_id: String = rest.chars()
                            .take_while(|c| c.is_alphanumeric())
                            .collect();
                        let node_id = node_id.trim().to_string();
                        println!("ContactManager: Found node_id for {}: '{}'", did, node_id);
                        if node_id.len() > 10 { 
                             return Ok(Some(node_id));
                        } else {
                            println!("ContactManager: Node ID too short for {}: '{}'", did, node_id);
                        }
                    } else {
                        println!("ContactManager: No bytescape-pkarr tag found for {}", did);
                    }
                } else {
                    println!("ContactManager: No bio/description for {}", did);
                }
                Ok(None)
            },
            Err(e) => {
                println!("ContactManager: Failed to get profile for {}: {}", did, e);
                Ok(None)
            }
        }
    }
}

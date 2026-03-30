//! # contacts.rs — Contact Sync & P2P Discovery
//!
//! This module synchronizes the user's Bluesky social graph (follows and followers)
//! into the local database, and discovers which contacts have Bytescape installed
//! for peer-to-peer communication.
//!
//! ## How It Works
//! 1. `sync_contacts()` fetches the user's follows and followers from Bluesky
//! 2. Both lists are merged (deduped by DID) into a single contact list
//! 3. Each contact is saved to the local `contacts` table in redb
//! 4. Periodically (every 5 minutes), each contact's Bluesky profile bio is
//!    checked for a `bytescape-pkarr:<node_id>` tag — this is how Bytescape
//!    users advertise their P2P node ID
//! 5. Contacts with a valid node ID are marked as `p2p_capable`, enabling
//!    direct encrypted communication via Iroh
//!
//! ## P2P Discovery Protocol
//! Bytescape users embed their Iroh node ID in their Bluesky bio as:
//! `bytescape-pkarr:z6MkNodeIdHere...`
//! This is a simple, decentralized discovery mechanism — no central server needed.
//! The node ID must be >10 characters (base32-encoded public key).
//!
//! ## Related Files
//! - `db/contacts.rs` — Database operations for the contacts table
//! - `chat.rs` — `ChatClient` which provides `get_follows()`, `get_followers()`,
//!   and `get_profile_record()` via the Bluesky API
//! - `sync.rs` — Uses P2P-capable contacts to establish Iroh connections

use std::sync::Arc;
use crate::db::{DbManager, Contact};
use crate::chat::ChatClient;
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════
// INTERNAL TYPES
// AT Protocol record types for device discovery (currently unused but
// reserved for future "device record" based P2P discovery)
// ═══════════════════════════════════════════════════════════════════

/// AT Protocol record containing a device's Iroh node ID.
/// Part of a future device-record-based discovery mechanism.
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct DeviceRecord {
    #[serde(rename = "nodeId")]
    node_id: String,
}

/// Response wrapper for `com.atproto.repo.getRecord` calls.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GetRecordResponse {
    value: DeviceRecord,
}

// ═══════════════════════════════════════════════════════════════════
// CONTACT MANAGER
// Orchestrates contact sync from Bluesky and P2P capability checks
// ═══════════════════════════════════════════════════════════════════

/// Manages the user's contact list by syncing Bluesky social graph data
/// to the local database and checking which contacts have Bytescape installed.
///
/// Uses `DbManager` for local persistence and `ChatClient` for Bluesky API
/// access (fetching follows, followers, and profile records).
#[allow(dead_code)]
pub struct ContactManager {
    /// Local database for storing/querying contacts.
    db: DbManager,
    /// Bluesky chat/API client for fetching social graph and profile data.
    chat: Arc<ChatClient>,
}

#[allow(dead_code)]
impl ContactManager {
    /// Create a new ContactManager with database and chat client handles.
    pub fn new(db: DbManager, chat: Arc<ChatClient>) -> Self {
        Self { db, chat }
    }

    /// Sync the user's Bluesky follows and followers into the local contacts table.
    ///
    /// Fetches both the "following" and "followers" lists from Bluesky, merges them
    /// (deduplicating by DID), and upserts each into the local database. For each
    /// contact, it also periodically checks whether they have a Bytescape P2P node
    /// ID in their profile bio (every 5 minutes to avoid rate limiting).
    ///
    /// # Arguments
    /// * `user_did` — The logged-in user's DID (for API calls)
    /// * `access_jwt` — Current access token for profile lookups
    ///
    /// # Side Effects
    /// * Makes Bluesky API calls to fetch social graph
    /// * Writes/updates contacts in the local redb database
    /// * Logs P2P capability check results to stdout
    pub async fn sync_contacts(&self, user_did: &str, access_jwt: &str) -> Result<(), String> {
        let follows = self.chat.get_follows(user_did).await?;
        let followers = self.chat.get_followers(user_did).await?;
        
        // Merge follows and followers, deduplicating by DID
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

    /// Check whether a contact has advertised a Bytescape P2P node ID in their bio.
    ///
    /// Looks for the `bytescape-pkarr:<node_id>` tag in the contact's Bluesky profile
    /// description field. If found and the node ID is long enough (>10 chars, indicating
    /// a valid base32-encoded public key), returns the node ID.
    ///
    /// # Arguments
    /// * `did` — The contact's DID to look up
    /// * `access_jwt` — Current access token for the Bluesky API
    ///
    /// # Returns
    /// `Ok(Some(node_id))` if a valid tag was found, `Ok(None)` otherwise.
    /// Errors from the profile lookup are swallowed and logged — a failed check
    /// should not prevent other contacts from syncing.
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

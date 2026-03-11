use super::*;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

/// A known peer on the P2P network.
///
/// Stored locally in the `peers` redb table, keyed by `node_id`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerInfo {
    /// Iroh public key (NodeId) — the unique identifier for this peer.
    pub node_id: String,
    /// User-supplied display name for this peer.
    pub display_name: Option<String>,
    /// Bytescape DID if known (e.g. `did:bytescape:...`).
    pub did: Option<String>,
    /// Unix timestamp when this peer was added.
    pub added_at: u64,
    /// Unix timestamp of last successful connectivity check.
    pub last_seen: Option<u64>,
    /// Whether we are actively following this peer.
    pub is_following: bool,
}

impl DbManager {
    /// Save or update a peer record, keyed by `node_id`.
    pub async fn save_peer(&self, peer: &PeerInfo) -> Result<(), String> {
        let serialized = serde_json::to_string(peer).map_err(|e| e.to_string())?;
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            table
                .insert(peer.node_id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Look up a peer by node_id.
    pub async fn get_peer(&self, node_id: &str) -> Result<Option<PeerInfo>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
        match table.get(node_id).map_err(|e| e.to_string())? {
            Some(v) => {
                let peer: PeerInfo =
                    serde_json::from_str(v.value()).map_err(|e| e.to_string())?;
                Ok(Some(peer))
            }
            None => Ok(None),
        }
    }

    /// List all known peers, sorted by most-recently-added first.
    pub async fn list_peers(&self) -> Result<Vec<PeerInfo>, String> {
        let db = self.db.read().await;
        let read_txn = db.begin_read().map_err(|e| e.to_string())?;
        let table = read_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
        let mut peers = Vec::new();
        for item in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = item.map_err(|e| e.to_string())?;
            let peer: PeerInfo =
                serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
            peers.push(peer);
        }
        peers.sort_by(|a, b| b.added_at.cmp(&a.added_at));
        Ok(peers)
    }

    /// Remove a peer by node_id.
    pub async fn remove_peer(&self, node_id: &str) -> Result<(), String> {
        let db = self.db.write().await;
        let write_txn = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = write_txn.open_table(PEERS_TABLE).map_err(|e| e.to_string())?;
            table.remove(node_id).map_err(|e| e.to_string())?;
        }
        write_txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}

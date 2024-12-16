use serde::{Deserialize, Serialize};

/// Topic name for peer discovery messages
pub const GOSSIP_TOPIC: &str = "peer-discovery-v1.0.0";
/// Interval in seconds between gossip broadcasts
pub const GOSSIP_INTERVAL: u64 = 30;

/// Message format for peer discovery gossip
#[derive(Debug, Serialize, Deserialize)]
pub struct GossipMessage {
    /// ID of the sending peer
    pub sender: String,
    /// List of (PeerId, Multiaddr) pairs as strings
    pub known_peers: Vec<(String, String)>,
} 
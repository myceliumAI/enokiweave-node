use serde::{Deserialize, Serialize};

/// Topic for peer discovery gossip messages
pub const GOSSIP_TOPIC: &str = "peer-discovery-v1.0.0";
/// Interval for gossip messages in seconds
pub const GOSSIP_INTERVAL: u64 = 30;

/// Message format for peer gossip
#[derive(Debug, Serialize, Deserialize)]
pub struct GossipMessage {
    /// The sender's peer ID
    pub sender: String,
    /// List of known peers and their addresses
    pub known_peers: Vec<(String, String)>,
} 
use libp2p::Multiaddr;

/// Configuration for a P2P network node
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Network address to listen on
    pub address: Multiaddr,
    /// Interval in seconds between peer health checks
    pub health_check_interval: u64,
    /// List of bootstrap peers to connect to on startup
    pub bootstrap_peers: Vec<Multiaddr>,
}

impl NodeConfig {
    /// Creates a new configuration with bootstrap peers
    pub fn new(
        address: Multiaddr,
        health_check_interval: u64,
        bootstrap_peers: Vec<Multiaddr>,
    ) -> Self {
        Self {
            address,
            health_check_interval,
            bootstrap_peers,
        }
    }

    /// Creates a configuration for a standalone node (no bootstrap peers)
    pub fn standalone(address: Multiaddr, health_check_interval: u64) -> Self {
        Self {
            address,
            health_check_interval,
            bootstrap_peers: Vec::new(),
        }
    }
} 
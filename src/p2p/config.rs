use libp2p::Multiaddr;

/// Default bootstrap nodes for the network
pub const DEFAULT_BOOTSTRAP_NODES: &[&str] = &[
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",  // Example node
    "/ip4/104.131.131.83/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuK",  // Example node
];

/// Configuration for the P2P node
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// The node's network address
    pub address: Multiaddr,
    /// Health check interval in seconds
    pub health_check_interval: u64,
    /// List of bootstrap peers to connect to
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Whether to use the default bootstrap nodes
    pub use_default_bootstrap: bool,
}

impl NodeConfig {
    /// Creates a new NodeConfig
    pub fn new(
        address: Multiaddr,
        health_check_interval: u64,
        bootstrap_peers: Vec<Multiaddr>,
        use_default_bootstrap: bool,
    ) -> Self {
        Self {
            address,
            health_check_interval,
            bootstrap_peers,
            use_default_bootstrap,
        }
    }

    /// Creates a new configuration for a bootstrap node (no bootstrap peers)
    pub fn bootstrap_node(address: Multiaddr, health_check_interval: u64) -> Self {
        Self {
            address,
            health_check_interval,
            bootstrap_peers: Vec::new(),
            use_default_bootstrap: false,
        }
    }
} 
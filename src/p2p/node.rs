use anyhow::{Result, anyhow};
use libp2p::{
    futures::StreamExt,
    gossipsub::{
        self, Behaviour as GossipsubBehaviour, MessageAuthenticity,
        ValidationMode, IdentTopic, PublishError,
    },
    identity::Keypair,
    noise, ping,
    swarm::{Swarm, SwarmEvent},
    tcp, yamux,
    PeerId, Multiaddr, SwarmBuilder,
};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc;
use tracing::{info, debug, error};


use super::{
    behaviour::NodeBehaviour,
    config::NodeConfig,
    message::{GossipMessage, GOSSIP_TOPIC, GOSSIP_INTERVAL},
    NodeEvent,
};

/// A P2P network node that handles peer discovery and communication
pub struct Node {
    pub config: NodeConfig,
    pub peer_id: PeerId,
    known_peers: HashMap<PeerId, Multiaddr>,
    swarm: Swarm<NodeBehaviour>,
    node_id: String, // Short node ID for logging
}

impl Node {
    /// Helper method to get a short node ID for logging
    fn short_id(&self) -> &str {
        &self.node_id
    }

    /// Helper method to format log messages with node ID prefix
    fn log(&self, message: String) -> String {
        format!("[Node-{}] {}", self.short_id(), message)
    }

    /// Creates a new node with the given configuration
    pub async fn new(config: NodeConfig) -> Result<Self> {
        let id_keys = Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        let peer_id_str = peer_id.to_string();
        let node_id = peer_id_str[peer_id_str.len()-6..].to_string();
        
        info!("{}", Self::log_static(&node_id, format!("💡 Created node with PeerId: {} ({})", node_id, peer_id)));
        
        let gossipsub = Self::create_gossipsub_behaviour(&id_keys)?;
        let mut behaviour = NodeBehaviour {
            ping: ping::Behaviour::new(ping::Config::new()
                .with_interval(Duration::from_secs(config.health_check_interval))),
            gossipsub,
        };

        Self::subscribe_to_topic(&mut behaviour, GOSSIP_TOPIC)?;

        let swarm = SwarmBuilder::with_existing_identity(id_keys)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|_| Ok(behaviour))?
            .build();

        let mut node = Self {
            config: config.clone(),
            peer_id,
            known_peers: HashMap::new(),
            swarm,
            node_id,
        };

        // Connect to bootstrap peers if provided
        if !config.bootstrap_peers.is_empty() {
            info!("{}", node.log(format!("💡 Connecting to {} bootstrap peers", config.bootstrap_peers.len())));
            node.connect_to_peers(&config.bootstrap_peers).await;
        } else {
            info!("{}", node.log("💡 Starting as standalone node".to_string()));
        }

        Ok(node)
    }

    /// Helper method for static contexts where self is not available
    fn log_static(node_id: &str, message: String) -> String {
        format!("[Node-{}] {}", node_id, message)
    }

    /// Creates a gossipsub behavior with optimized settings for our use case
    fn create_gossipsub_behaviour(id_keys: &Keypair) -> Result<GossipsubBehaviour> {
        let config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(GOSSIP_INTERVAL))
            .validation_mode(ValidationMode::Permissive)
            // Use more reasonable mesh sizes for better connectivity
            .mesh_n_low(2)     // Allow down to 2 peers minimum
            .mesh_n(4)         // Target 4 peers
            .mesh_n_high(8)    // Allow up to 8 peers
            .mesh_outbound_min(2)
            .history_length(10)
            .history_gossip(3)
            .flood_publish(true)
            .build()
            .map_err(|e| anyhow!("Failed to build gossipsub config: {}", e))?;

        GossipsubBehaviour::new(
            MessageAuthenticity::Signed(id_keys.clone()),
            config,
        ).map_err(|e| anyhow!("Failed to create gossipsub behavior: {}", e))
    }

    /// Subscribes to a gossipsub topic
    fn subscribe_to_topic(behaviour: &mut NodeBehaviour, topic: &str) -> Result<()> {
        let topic = IdentTopic::new(topic);
        behaviour.gossipsub.subscribe(&topic)
            .map(|_| ())
            .map_err(|e| anyhow!("Failed to subscribe to topic: {}", e))
    }

    /// Attempts to discover and connect to a new peer
    async fn discover_peer(&mut self, peer_id: PeerId, addr: Multiaddr) -> Result<bool> {
        if peer_id == self.peer_id {
            return Ok(false);
        }

        if let Some(known_addr) = self.known_peers.get(&peer_id) {
            if known_addr == &addr {
                return Ok(false);
            }
            debug!("{}", self.log(format!("📝 Updating address for peer {}", peer_id)));
        }

        self.known_peers.insert(peer_id, addr.clone());
        
        if !self.swarm.is_connected(&peer_id) {
            if let Err(e) = self.connect_to_peer(addr).await {
                error!("{}", self.log(format!("⚠️ Failed to connect to discovered peer: {}", e)));
            }
        }

        Ok(true)
    }

    /// Broadcasts our known peers to the network for peer discovery
    async fn broadcast_known_peers(&mut self) -> Result<()> {
        if self.known_peers.is_empty() {
            debug!("{}", self.log("📢 No peers to broadcast (standalone mode)".to_string()));
            return Ok(());
        }

        let message = GossipMessage {
            sender: self.peer_id.to_string(),
            known_peers: self.known_peers.iter()
                .map(|(peer_id, addr)| (peer_id.to_string(), addr.to_string()))
                .collect(),
        };

        let encoded = serde_json::to_string(&message)?;
        let topic = IdentTopic::new(GOSSIP_TOPIC);
        
        match self.swarm.behaviour_mut().gossipsub.publish(topic, encoded.as_bytes()) {
            Ok(_) => {
                debug!("{}", self.log(format!("📢 Broadcasting {} known peers", self.known_peers.len())));
                Ok(())
            },
            Err(PublishError::InsufficientPeers) => {
                debug!("{}", self.log("📝 Skipping broadcast: no peers available yet".to_string()));
                Ok(())
            },
            Err(e) => Err(anyhow!("Failed to publish gossip message: {}", e))
        }
    }

    /// Starts the node, listening for connections and handling events
    pub async fn start(&mut self) -> Result<()> {
        self.swarm.listen_on(self.config.address.clone())?;

        // Start health check loop with peer info
        let peer_id = self.peer_id;
        let (health_tx, mut health_rx) = mpsc::channel::<HashMap<PeerId, Multiaddr>>(32);
        let health_tx_clone = health_tx.clone();
        let health_interval = self.config.health_check_interval;
        let node_id = self.node_id.clone();

        // Start health check loop
        let health_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(health_interval));
            let mut last_log = std::time::Instant::now();
            loop {
                interval.tick().await;
                // Only log health check status every 5 minutes unless there's an error
                let should_log = last_log.elapsed() >= Duration::from_secs(300);
                if should_log {
                    debug!("[Node-{}] 💡 Health check for node: {}", node_id, peer_id);
                    last_log = std::time::Instant::now();
                }
                if let Err(e) = health_tx_clone.send(HashMap::new()).await {
                    error!("[Node-{}] ⚠️ Failed to trigger health check: {}", node_id, e);
                    break;
                }
            }
        });

        // Start gossip loop for peer discovery
        let (tx, mut rx) = mpsc::channel(32);
        let tx_clone = tx.clone();
        let node_id = self.node_id.clone();

        let gossip_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GOSSIP_INTERVAL));
            loop {
                interval.tick().await;
                if tx_clone.send(()).await.is_err() {
                    error!("[Node-{}] ⚠️ Gossip loop terminated", node_id);
                    break;
                }
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                Some(_) = rx.recv() => {
                    if let Err(e) = self.broadcast_known_peers().await {
                        error!("{}", self.log(format!("⚠️ Failed to broadcast peers: {}", e)));
                    }
                }
                Some(_) = health_rx.recv() => {
                    // Log network status during health check only if there are peers
                    if !self.known_peers.is_empty() {
                        info!("{}", self.log(format!("📊 Node has {} connections:", self.known_peers.len())));
                        for (peer_id, addr) in self.known_peers.iter() {
                            info!("{}   ├─ {} at {}", self.log("".to_string()), peer_id.to_string().split_at(6).0, addr);
                        }
                    }
                }
                event = self.swarm.next() => {
                    match event {
                        Some(event) => self.handle_swarm_event(event).await?,
                        None => break,
                    }
                }
                else => {
                    debug!("{}", self.log("💡 Main loop shutting down".to_string()));
                    break;
                }
            }
        }

        // Clean up background tasks
        health_handle.abort();
        gossip_handle.abort();

        Ok(())
    }

    /// Attempts to connect to a peer at the given address
    pub async fn connect_to_peer(&mut self, addr: Multiaddr) -> Result<()> {
        info!("{}", self.log(format!("🔌 Attempting to connect to peer at {}", addr)));
        self.swarm.dial(addr)?;
        Ok(())
    }

    /// Attempts to connect to multiple peers
    pub async fn connect_to_peers(&mut self, addrs: &[Multiaddr]) {
        info!("{}", self.log(format!("🔌 Attempting to connect to {} peers", addrs.len())));
        for addr in addrs {
            match self.connect_to_peer(addr.clone()).await {
                Ok(_) => {
                    info!("{}", self.log(format!("✅ Successfully initiated connection to {}", addr)));
                }
                Err(e) => {
                    error!("{}", self.log(format!("❌ Failed to connect to {}: {}", addr, e)));
                }
            }
        }
    }

    /// Removes a peer from our known peers list
    fn remove_peer(&mut self, peer_id: &PeerId) {
        if self.known_peers.remove(peer_id).is_some() {
            info!("{}", self.log(format!("❎ Removed peer: {}", peer_id)));
        }
    }

    /// Handles network events from the swarm
    async fn handle_swarm_event(&mut self, event: SwarmEvent<NodeEvent>) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(NodeEvent::Ping(ping::Event { peer, result, .. })) => {
                match result {
                    Ok(duration) => {
                        debug!("{}", self.log(format!("✅ Ping success: {} responded in {:?}", peer, duration)));
                    }
                    Err(error) => {
                        error!("{}", self.log(format!("⚠️ Ping failure: {} error: {}", peer, error)));
                        if !self.swarm.is_connected(&peer) {
                            self.remove_peer(&peer);
                        }
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("{}", self.log(format!("✅ Listening on: {}", address)));
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                info!("{}", self.log(format!("✅ Connected to: {}", peer_id)));
                let addr = endpoint.get_remote_address();
                if self.discover_peer(peer_id, addr.clone()).await? {
                    self.broadcast_known_peers().await?;
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                if !self.swarm.is_connected(&peer_id) {
                    error!("{}", self.log(format!("⚠️ All connections closed to: {}", peer_id)));
                    self.remove_peer(&peer_id);
                }
            }
            SwarmEvent::Behaviour(NodeEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                let gossip: GossipMessage = serde_json::from_slice(&message.data)?;
                debug!("{}", self.log(format!("📨 Received peer list from {}", gossip.sender)));

                let mut new_peers = false;
                for (peer_id_str, addr_str) in gossip.known_peers {
                    if let (Ok(peer_id), Ok(addr)) = (
                        peer_id_str.parse::<PeerId>(),
                        addr_str.parse::<Multiaddr>(),
                    ) {
                        if self.discover_peer(peer_id, addr).await? {
                            new_peers = true;
                        }
                    }
                }

                if new_peers {
                    self.broadcast_known_peers().await?;
                }
            }
            _ => {
                debug!("{}", self.log(format!("📝 Unhandled event: {:?}", event)));
            }
        }
        Ok(())
    }

    /// Returns true if the node is connected to the given peer
    pub fn is_connected_to(&self, peer_id: &PeerId) -> bool {
        self.swarm.is_connected(peer_id)
    }

    /// Returns true if the node knows about the given peer
    pub fn knows_peer(&self, peer_id: &PeerId) -> bool {
        self.known_peers.contains_key(peer_id)
    }

    /// Returns a list of all known peer IDs
    pub fn known_peer_ids(&self) -> Vec<PeerId> {
        self.known_peers.keys().cloned().collect()
    }

    /// Returns the number of known peers
    pub fn peer_count(&self) -> usize {
        self.known_peers.len()
    }

    /// Start listening on the given address
    pub fn start_listening(&mut self, addr: Multiaddr) -> Result<()> {
        self.swarm.listen_on(addr)?;
        Ok(())
    }

    /// Get the first listen address
    pub fn get_listen_address(&self) -> Option<Multiaddr> {
        self.swarm.listeners().next().cloned()
    }
} 
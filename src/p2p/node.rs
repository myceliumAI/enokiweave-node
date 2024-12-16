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
use tracing::{info, debug, Span};

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
    _span: Span, // Keep the span alive for the lifetime of the node
}

impl Node {
    /// Helper method to get a short node ID for logging
    fn short_id(&self) -> String {
        self.peer_id.to_string().split_at(6).0.to_string()
    }

    /// Creates a new node with the given configuration
    pub async fn new(config: NodeConfig) -> Result<Self> {
        let id_keys = Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        let node_id = peer_id.to_string().split_at(6).0.to_string();
        let _logger = crate::get_logger!("node");
        
        info!("[Node-{}] 💡 Created node with PeerId: {} ({})", node_id, node_id, peer_id);
        
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
            _span: tracing::info_span!("node", id = %node_id),
        };

        // Connect to bootstrap peers if provided
        if !config.bootstrap_peers.is_empty() {
            info!("[Node-{}] 💡 Connecting to {} bootstrap peers", node_id, config.bootstrap_peers.len());
            node.connect_to_peers(&config.bootstrap_peers).await;
        } else {
            info!("[Node-{}] 💡 Starting as standalone node", node_id);
        }

        Ok(node)
    }

    /// Creates a gossipsub behavior with optimized settings for our use case
    fn create_gossipsub_behaviour(id_keys: &Keypair) -> Result<GossipsubBehaviour> {
        let config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(GOSSIP_INTERVAL))
            .validation_mode(ValidationMode::Permissive)
            // Allow operating with minimal peers for better bootstrap
            .mesh_n_low(0)
            .mesh_n(1)
            .mesh_n_high(2)
            .mesh_outbound_min(0)
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
        let node_id = self.short_id();
        if peer_id == self.peer_id {
            return Ok(false);
        }

        if let Some(known_addr) = self.known_peers.get(&peer_id) {
            if known_addr == &addr {
                return Ok(false);
            }
            debug!("[Node-{}] 📝 Updating address for peer {}", node_id, peer_id);
        }

        self.known_peers.insert(peer_id, addr.clone());
        
        if !self.swarm.is_connected(&peer_id) {
            if let Err(e) = self.connect_to_peer(addr).await {
                debug!("[Node-{}] ⚠️ Failed to connect to discovered peer: {}", node_id, e);
            }
        }

        Ok(true)
    }

    /// Broadcasts our known peers to the network for peer discovery
    async fn broadcast_known_peers(&mut self) -> Result<()> {
        let node_id = self.short_id();
        if self.known_peers.is_empty() {
            debug!("[Node-{}] 📢 No peers to broadcast (standalone mode)", node_id);
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
                debug!("[Node-{}] 📢 Broadcasting {} known peers", node_id, self.known_peers.len());
                Ok(())
            },
            Err(PublishError::InsufficientPeers) => {
                debug!("[Node-{}] 📢 Skipping broadcast: no peers available yet", node_id);
                Ok(())
            },
            Err(e) => Err(anyhow!("Failed to publish gossip message: {}", e))
        }
    }

    /// Starts the node, listening for connections and handling events
    pub async fn start(&mut self) -> Result<()> {
        let node_id = self.short_id();
        self.swarm.listen_on(self.config.address.clone())?;

        // Start health check loop with peer info
        let peer_id = self.peer_id;
        let (health_tx, mut health_rx) = mpsc::channel::<HashMap<PeerId, Multiaddr>>(32);
        let health_tx_clone = health_tx.clone();
        let health_interval = self.config.health_check_interval;

        // Start health check loop
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(health_interval));
            let node_id = peer_id.to_string().split_at(6).0.to_string();
            loop {
                interval.tick().await;
                debug!("[Node-{}] 💡 Health check for node: {}", node_id, peer_id);
                if let Err(e) = health_tx_clone.send(HashMap::new()).await {
                    debug!("[Node-{}] ⚠️ Failed to trigger health check: {}", node_id, e);
                    break;
                }
            }
        });

        // Start gossip loop for peer discovery
        let (tx, mut rx) = mpsc::channel(32);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GOSSIP_INTERVAL));
            loop {
                interval.tick().await;
                if tx_clone.send(()).await.is_err() {
                    break;
                }
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                Some(_) = rx.recv() => {
                    if let Err(e) = self.broadcast_known_peers().await {
                        debug!("[Node-{}] ⚠️ Failed to broadcast peers: {}", node_id, e);
                    }
                }
                Some(_) = health_rx.recv() => {
                    // Log network status during health check
                    if !self.known_peers.is_empty() {
                        info!("[Node-{}] 📊 Node has {} connections:", node_id, self.known_peers.len());
                        for (peer_id, addr) in self.known_peers.iter() {
                            info!("[Node-{}]   ├─ {} at {}", node_id, peer_id.to_string().split_at(6).0, addr);
                        }
                    }
                }
                event = self.swarm.next() => {
                    match event {
                        Some(event) => self.handle_swarm_event(event).await?,
                        None => break,
                    }
                }
            }
        }

        Ok(())
    }

    /// Attempts to connect to a peer at the given address
    pub async fn connect_to_peer(&mut self, addr: Multiaddr) -> Result<()> {
        let node_id = self.short_id();
        info!("[Node-{}] 🔌 Attempting to connect to peer at {}", node_id, addr);
        self.swarm.dial(addr)?;
        Ok(())
    }

    /// Attempts to connect to multiple peers
    pub async fn connect_to_peers(&mut self, addrs: &[Multiaddr]) {
        let node_id = self.short_id();
        for addr in addrs {
            if let Err(e) = self.connect_to_peer(addr.clone()).await {
                debug!("[Node-{}] ⚠️ Failed to connect to {}: {}", node_id, addr, e);
            }
        }
    }

    /// Removes a peer from our known peers list
    fn remove_peer(&mut self, peer_id: &PeerId) {
        let node_id = self.short_id();
        if self.known_peers.remove(peer_id).is_some() {
            info!("[Node-{}] ❎ Removed peer: {}", node_id, peer_id);
        }
    }

    /// Handles network events from the swarm
    async fn handle_swarm_event(&mut self, event: SwarmEvent<NodeEvent>) -> Result<()> {
        let node_id = self.short_id();
        match event {
            SwarmEvent::Behaviour(NodeEvent::Ping(ping::Event { peer, result, .. })) => {
                match result {
                    Ok(duration) => {
                        debug!("[Node-{}]  Ping success: {} responded in {:?}", node_id, peer, duration);
                    }
                    Err(error) => {
                        debug!("[Node-{}] ⚠️ Ping failure: {} error: {}", node_id, peer, error);
                        if !self.swarm.is_connected(&peer) {
                            self.remove_peer(&peer);
                        }
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("[Node-{}] ✅ Listening on: {}", node_id, address);
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                info!("[Node-{}] ✅ Connected to: {}", node_id, peer_id);
                let addr = endpoint.get_remote_address();
                if self.discover_peer(peer_id, addr.clone()).await? {
                    self.broadcast_known_peers().await?;
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                if !self.swarm.is_connected(&peer_id) {
                    debug!("[Node-{}] ⚠️ All connections closed to: {}", node_id, peer_id);
                    self.remove_peer(&peer_id);
                }
            }
            SwarmEvent::Behaviour(NodeEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                let gossip: GossipMessage = serde_json::from_slice(&message.data)?;
                debug!("[Node-{}] 📨 Received peer list from {}", node_id, gossip.sender);

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
                debug!("[Node-{}] 📝 Unhandled event: {:?}", node_id, event);
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
use anyhow::{Result, Context};
use libp2p::{
    futures::StreamExt,
    gossipsub::{self, Behaviour as GossipsubBehaviour, MessageAuthenticity, ValidationMode},
    identity::Keypair,
    noise,
    ping,
    swarm::{Swarm, SwarmEvent},
    tcp, yamux,
    PeerId, Multiaddr,
    SwarmBuilder,
};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc;
use tracing::{info, warn, debug};

use super::{
    behaviour::NodeBehaviour,
    config::{NodeConfig, DEFAULT_BOOTSTRAP_NODES},
    message::{GossipMessage, GOSSIP_TOPIC, GOSSIP_INTERVAL},
    NodeEvent,
};

/// Represents a node in the P2P network
pub struct Node {
    /// The node's configuration
    pub config: NodeConfig,
    /// The node's peer ID (derived from public key)
    pub peer_id: PeerId,
    /// Map of known peers to their addresses
    known_peers: HashMap<PeerId, Multiaddr>,
    /// The libp2p swarm that handles network events
    swarm: Swarm<NodeBehaviour>,
}

impl Node {
    /// Creates a new Node instance
    pub async fn new(config: NodeConfig) -> Result<Self> {
        // Generate random keypair for this node
        let id_keys = Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        
        info!("💡 Created node with PeerId: {}", peer_id);
        debug!("📝 Node keypair generated successfully");
        
        // Create gossipsub configuration
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(GOSSIP_INTERVAL))
            .validation_mode(ValidationMode::Strict)
            .build()
            .expect("Valid gossipsub config");

        // Create gossipsub behavior
        let gossipsub = GossipsubBehaviour::new(
            MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub_config,
        ).expect("Valid gossipsub behavior");

        // Subscribe to the peer discovery topic
        let topic = gossipsub::IdentTopic::new(GOSSIP_TOPIC);
        
        // Create the node behaviors
        let mut behaviour = NodeBehaviour {
            ping: ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(config.health_check_interval))),
            gossipsub,
        };

        // Subscribe to the gossipsub topic
        behaviour.gossipsub.subscribe(&topic).expect("Valid topic subscription");
        
        // Build the swarm
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
        };

        // Connect to bootstrap peers
        if config.use_default_bootstrap {
            for addr_str in DEFAULT_BOOTSTRAP_NODES {
                if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                    info!("💡 Attempting to connect to default bootstrap node: {}", addr);
                    if let Err(e) = node.connect_to_peer(addr).await {
                        warn!("⚠️ Failed to connect to bootstrap node: {}", e);
                    }
                }
            }
        }

        for addr in config.bootstrap_peers {
            info!("💡 Attempting to connect to bootstrap peer: {}", addr);
            if let Err(e) = node.connect_to_peer(addr).await {
                warn!("⚠️ Failed to connect to bootstrap peer: {}", e);
            }
        }

        Ok(node)
    }

    /// Broadcasts known peers to the network
    async fn broadcast_known_peers(&mut self) -> Result<()> {
        if self.known_peers.is_empty() {
            debug!("No peers to broadcast");
            return Ok(());
        }

        let message = GossipMessage {
            sender: self.peer_id.to_string(),
            known_peers: self.known_peers.iter()
                .map(|(peer_id, addr)| (peer_id.to_string(), addr.to_string()))
                .collect(),
        };

        let encoded = serde_json::to_string(&message)?;
        let topic = gossipsub::IdentTopic::new(GOSSIP_TOPIC);
        
        self.swarm.behaviour_mut().gossipsub.publish(topic, encoded.as_bytes())?;
        debug!("📢 Broadcasting {} known peers", self.known_peers.len());
        
        Ok(())
    }

    /// Starts the gossip loop
    async fn start_gossip_loop(&mut self) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(32);
        
        // Create a clone of what we need for the gossip task
        let gossip_tx = tx.clone();
        
        // Spawn interval task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(GOSSIP_INTERVAL));
            loop {
                interval.tick().await;
                if gossip_tx.send(()).await.is_err() {
                    break;
                }
            }
        });

        // Handle gossip events in the current task
        while rx.recv().await.is_some() {
            if let Err(e) = self.broadcast_known_peers().await {
                warn!("⚠️ Failed to broadcast peers: {}", e);
            }
        }

        Ok(())
    }

    /// Starts the node and begins listening for connections
    pub async fn start(&mut self) -> Result<()> {
        // Start listening on the specified address
        self.swarm.listen_on(self.config.address.clone())
            .context("Failed to start listening")?;
        info!("✅ Node listening on: {}", self.config.address);

        // Start the health check loop
        self.start_health_check_loop().await
            .context("Failed to start health check loop")?;

        // Start the gossip loop
        self.start_gossip_loop().await
            .context("Failed to start gossip loop")?;

        // Handle swarm events
        self.handle_events().await
            .context("Failed to handle events")?;

        Ok(())
    }

    /// Connects to a peer at the given address
    pub async fn connect_to_peer(&mut self, addr: Multiaddr) -> Result<()> {
        info!("🔌 Attempting to connect to peer at {}", addr);
        self.swarm.dial(addr.clone())
            .context("Failed to dial peer")?;
        Ok(())
    }

    /// Removes a peer from the known peers list
    fn remove_peer(&mut self, peer_id: &PeerId) {
        if self.known_peers.remove(peer_id).is_some() {
            info!("❎ Removed peer: {}", peer_id);
        }
    }

    /// Starts the health check loop
    async fn start_health_check_loop(&mut self) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(32);
        let peer_id = self.peer_id;
        let interval = self.config.health_check_interval;

        // Spawn health check task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval));
            loop {
                interval.tick().await;
                if tx.send(()).await.is_err() {
                    break;
                }
            }
        });

        // Handle health check events
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                debug!("💡 Performing health check for node: {}", peer_id);
            }
        });

        Ok(())
    }

    /// Main event handling loop
    async fn handle_events(&mut self) -> Result<()> {
        loop {
            match self.swarm.next().await {
                Some(event) => self.handle_swarm_event(event).await
                    .context("Failed to handle swarm event")?,
                None => break,
            }
        }
        Ok(())
    }

    /// Handles individual swarm events
    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<NodeEvent>,
    ) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(NodeEvent::Ping(ping::Event { peer, result, connection: _ })) => {
                match result {
                    Ok(duration) => {
                        info!("✅ Ping success: {} responded in {:?}", peer, duration);
                    }
                    Err(error) => {
                        warn!("⚠️ Ping failure: {} error: {}", peer, error);
                        self.remove_peer(&peer);
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("✅ Listening on: {}", address);
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                info!("✅ Connected to: {}", peer_id);
                let addr = endpoint.get_remote_address();
                self.known_peers.insert(peer_id, addr.clone());
                // Broadcast our updated peer list when we get a new connection
                if let Err(e) = self.broadcast_known_peers().await {
                    warn!("⚠️ Failed to broadcast peers after new connection: {}", e);
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                warn!("⚠️ Connection closed to: {}", peer_id);
                self.remove_peer(&peer_id);
            }
            SwarmEvent::Behaviour(NodeEvent::Gossipsub(gossipsub::Event::Message { 
                message,
                ..
            })) => {
                let gossip: GossipMessage = serde_json::from_slice(&message.data)?;
                debug!("📨 Received peer list from {}", gossip.sender);

                let mut new_peers = false;
                for (peer_id_str, addr_str) in gossip.known_peers {
                    if let (Ok(peer_id), Ok(addr)) = (
                        peer_id_str.parse::<PeerId>(),
                        addr_str.parse::<Multiaddr>(),
                    ) {
                        if peer_id != self.peer_id && !self.known_peers.contains_key(&peer_id) {
                            new_peers = true;
                            self.known_peers.insert(peer_id, addr.clone());
                            // Try to connect to the new peer
                            let _ = self.connect_to_peer(addr).await;
                        }
                    }
                }

                // If we discovered new peers, broadcast our updated peer list
                if new_peers {
                    if let Err(e) = self.broadcast_known_peers().await {
                        warn!("⚠️ Failed to broadcast peers after discovery: {}", e);
                    }
                }
            }
            _ => {
                debug!("📝 Unhandled event: {:?}", event);
            }
        }
        Ok(())
    }
} 
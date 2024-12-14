use anyhow::{Result, Context};
use libp2p::{
    core::{self, transport::Transport},
    futures::StreamExt,
    identity::{Keypair, PublicKey},
    noise,
    ping::{Behaviour as PingBehaviour, Config as PingConfig, Event as PingEvent, Success, Failure},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent, SwarmBuilder},
    tcp,
    yamux,
    PeerId, Multiaddr,
};
use std::{collections::HashSet, time::Duration, io};
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

/// Configuration for the P2P node
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// The node's network address
    pub address: Multiaddr,
    /// Health check interval in seconds
    pub health_check_interval: u64,
}

/// Represents the behavior of our P2P node
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NodeEvent")]
struct NodeBehaviour {
    ping: PingBehaviour,
}

/// Events that can be emitted by our node
#[derive(Debug)]
enum NodeEvent {
    Ping(PingEvent),
}

impl From<PingEvent> for NodeEvent {
    fn from(event: PingEvent) -> Self {
        NodeEvent::Ping(event)
    }
}

/// Represents a node in the P2P network
pub struct Node {
    /// The node's configuration
    pub config: NodeConfig,
    /// The node's peer ID (derived from public key)
    pub peer_id: PeerId,
    /// The node's public key
    pub public_key: PublicKey,
    /// Set of known peers in the network
    pub known_peers: HashSet<PeerId>,
    /// The libp2p swarm that handles network events
    swarm: Swarm<NodeBehaviour>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("config", &self.config)
            .field("peer_id", &self.peer_id)
            .field("public_key", &self.public_key)
            .field("known_peers", &self.known_peers)
            .finish_non_exhaustive()
    }
}

impl Node {
    /// Creates a new Node instance
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the node
    ///
    /// # Returns
    ///
    /// Returns a Result containing the new Node instance or an error
    pub async fn new(config: NodeConfig) -> Result<Self> {
        // Generate random keypair for this node
        let id_keys = Keypair::generate_ed25519();
        let public_key = id_keys.public();
        let peer_id = PeerId::from(&public_key);
        
        info!("💡 Created node with PeerId: {}", peer_id);
        debug!("📝 Node public key: {:?}", public_key);
        
        // Create a transport with noise for authentication and encryption
        let transport = Self::create_transport(&id_keys)
            .context("Failed to create transport")?;
        
        // Create the node behavior (ping for health checks)
        let behaviour = NodeBehaviour {
            ping: PingBehaviour::new(PingConfig::new().with_interval(Duration::from_secs(config.health_check_interval))),
        };
        
        // Build the swarm
        let swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id)
            .build();
        
        Ok(Self {
            config,
            peer_id,
            public_key,
            known_peers: HashSet::new(),
            swarm,
        })
    }

    /// Creates a libp2p transport with noise encryption and yamux multiplexing
    fn create_transport(id_keys: &Keypair) -> Result<core::transport::Boxed<(PeerId, core::muxing::StreamMuxerBox)>> {
        Ok(tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(core::upgrade::Version::V1)
            .authenticate(noise::Config::new(id_keys).context("Failed to create noise config")?)
            .multiplex(yamux::Config::default())
            .boxed())
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

        // Handle swarm events
        self.handle_events().await
            .context("Failed to handle events")?;

        Ok(())
    }

    /// Adds a new peer to the known peers list
    pub fn add_peer(&mut self, peer_id: PeerId, addr: Multiaddr) {
        if self.known_peers.insert(peer_id) {
            info!("✅ Added new peer: {}", peer_id);
            // Dial the new peer
            if let Err(e) = self.swarm.dial(addr.clone()) {
                error!("❌ Failed to dial peer {} at {}: {}", peer_id, addr, e);
                self.known_peers.remove(&peer_id);
            }
        }
    }

    /// Removes a peer from the known peers list
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        if self.known_peers.remove(peer_id) {
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
        event: SwarmEvent<NodeEvent, io::Error>,
    ) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(NodeEvent::Ping(ping_event)) => {
                match ping_event {
                    PingEvent::Success(Success { peer, rtt }) => {
                        info!("✅ Ping success: {} responded in {:?}", peer, rtt);
                    }
                    PingEvent::Failure(Failure { peer, error }) => {
                        warn!("⚠️ Ping failure: {} error: {}", peer, error);
                        self.remove_peer(&peer);
                    }
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("✅ Listening on: {}", address);
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("✅ Connected to: {}", peer_id);
                self.known_peers.insert(peer_id);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                warn!("⚠️ Connection closed to: {}", peer_id);
                self.remove_peer(&peer_id);
            }
            _ => {
                debug!("📝 Unhandled event: {:?}", event);
            }
        }
        Ok(())
    }
}

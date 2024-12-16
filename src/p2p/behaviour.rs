use libp2p::{
    gossipsub::{self, Behaviour as GossipsubBehaviour},
    ping,
    swarm::NetworkBehaviour,
};

/// Combined network behavior for our P2P node
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NodeEvent")]
pub struct NodeBehaviour {
    /// Ping protocol for peer health checks
    pub ping: ping::Behaviour,
    /// Gossipsub protocol for peer discovery
    pub gossipsub: GossipsubBehaviour,
}

/// Events that can be emitted by our network behavior
#[derive(Debug)]
pub enum NodeEvent {
    /// Ping protocol events
    Ping(ping::Event),
    /// Gossipsub protocol events
    Gossipsub(gossipsub::Event),
}

impl From<ping::Event> for NodeEvent {
    fn from(event: ping::Event) -> Self {
        NodeEvent::Ping(event)
    }
}

impl From<gossipsub::Event> for NodeEvent {
    fn from(event: gossipsub::Event) -> Self {
        NodeEvent::Gossipsub(event)
    }
} 
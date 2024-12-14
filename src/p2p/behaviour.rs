use libp2p::{
    gossipsub::{self, Behaviour as GossipsubBehaviour},
    ping,
    swarm::NetworkBehaviour,
};

/// Represents the behavior of our P2P node
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NodeEvent")]
pub struct NodeBehaviour {
    pub ping: ping::Behaviour,
    pub gossipsub: GossipsubBehaviour,
}

/// Events that can be emitted by our node
#[derive(Debug)]
pub enum NodeEvent {
    Ping(ping::Event),
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
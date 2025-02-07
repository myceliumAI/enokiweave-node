use clap::Parser;
use libp2p::futures::StreamExt;
use libp2p::mdns::tokio::Tokio;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{
    core::upgrade::Version, identity, noise, tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    mdns::{Behaviour as Mdns, Event as MdnsEvent},
    swarm::{SwarmBuilder, SwarmEvent},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tcp::tokio::Transport as TokioTransport;
use tokio::sync::Mutex;
use tracing::{info, trace};
use transaction_manager::TransactionManager;

use crate::rpc::run_http_rpc_server;

mod address;
mod rpc;
mod transaction;
mod transaction_manager;

const DB_NAME: &str = "./local_db/transaction_db";

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent")]
struct P2PBlockchainBehaviour {
    floodsub: Floodsub,
    mdns: Mdns<Tokio>,
}

impl From<FloodsubEvent> for OutEvent {
    fn from(value: FloodsubEvent) -> Self {
        OutEvent::Floodsub(value)
    }
}
impl From<MdnsEvent> for OutEvent {
    fn from(value: MdnsEvent) -> Self {
        OutEvent::Mdns(value)
    }
}

enum OutEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
}

#[derive(Deserialize)]
pub struct GenesisArgs {
    balances: HashMap<String, u64>,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    genesis_file_path: String,
    #[arg(long)]
    initial_peers_file_path: Option<String>,
    #[arg(long)]
    initial_peers: Option<Vec<String>>,
    #[arg(long, default_value = "3001")]
    rpc_port: u16,
}

async fn handle_swarm_events(mut swarm: Swarm<P2PBlockchainBehaviour>) {
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {:?}", address);
            }
            SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Message(_))) => {}
            SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
                for (peer_id, _multiaddr) in list {
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .add_node_to_partial_view(peer_id);
                }
            }
            SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
                for (peer_id, _multiaddr) in list {
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .remove_node_from_partial_view(&peer_id);
                }
            }
            _ => {}
        }
    }
}

fn are_all_peers_dead(peers: Vec<Multiaddr>, swarm: &mut Swarm<P2PBlockchainBehaviour>) -> bool {
    let mut any_peers_alive = false;
    for peer in peers {
        match Swarm::dial(swarm, peer) {
            Ok(_) => {
                any_peers_alive = true;
            }
            Err(e) => {
                trace!("Failed to dial peer, error: {}", e);
            }
        }
        if !any_peers_alive {
            warn!("No peers are alive and reachable");
        }
    }
    return !any_peers_alive;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();

    // TODO: Create local_peer_id from the node's private key
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    trace!("Local peer id: {:?}", local_peer_id);

    std::fs::create_dir_all("./local_db/transaction_db")
        .expect("Failed to create transaction_db directory");
    let transaction_manager: Arc<Mutex<TransactionManager>> =
        Arc::new(Mutex::new(TransactionManager::new()?));

    {
        let genesis_content =
            std::fs::read_to_string(&args.genesis_file_path).expect("Failed to read genesis file");
        let genesis_args: GenesisArgs =
            serde_json::from_str(&genesis_content).expect("Failed to parse genesis file");

        let transaction_manager = transaction_manager.lock().await;

        transaction_manager.load_genesis_transactions(genesis_args)?;
    }

    // Create a transport
    let transport = {
        let keypair = identity::Keypair::generate_ed25519();
        let noise_config =
            noise::Config::new(&keypair).expect("failed to construct the noise config");

        TokioTransport::new(tcp::Config::default().nodelay(true))
            .upgrade(Version::V1Lazy)
            .authenticate(noise_config)
            .multiplex(yamux::Config::default())
            .boxed()
    };
    // Create a Floodsub topic
    let floodsub_topic = Topic::new("blocks");

    // Create a Swarm to manage peers and events
    let mut swarm = {
        let mdns = Mdns::new(Default::default(), local_peer_id)?;
        let mut behaviour = P2PBlockchainBehaviour {
            floodsub: Floodsub::new(local_peer_id),
            mdns,
        };

        behaviour.floodsub.subscribe(floodsub_topic.clone());
        SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build()
    };

    let mut initial_peers = Vec::new();

    if let Some(file_path) = &args.initial_peers_file_path {
        initial_peers.extend(
            std::fs::read_to_string(file_path)?
                .lines()
                .map(|s| s.parse::<Multiaddr>())
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    if let Some(peers) = args.initial_peers {
        initial_peers.extend(
            peers
                .iter()
                .map(|s| s.parse::<Multiaddr>())
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    are_all_peers_dead(initial_peers, &mut swarm);

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Start handling incoming messages
    tokio::spawn(handle_swarm_events(swarm));

    run_http_rpc_server(transaction_manager, args.rpc_port).await?;

    Ok(())
}

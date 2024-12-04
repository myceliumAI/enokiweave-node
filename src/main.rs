use blocklattice::{BlockLattice, Transaction, TransactionRequest};
use libp2p::futures::StreamExt;
use libp2p::mdns::tokio::Tokio;
use libp2p::swarm::{behaviour, NetworkBehaviour};
use libp2p::{core::upgrade::Version, identity, noise, tcp, yamux, PeerId, Transport};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    mdns::{Behaviour as Mdns, Event as MdnsEvent},
    swarm::{SwarmBuilder, SwarmEvent},
};
use serde_json::Value as JsonValue;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tcp::tokio::Transport as TokioTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

mod blocklattice;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent")]
struct BlockchainBehaviour {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

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
        let mut behaviour = BlockchainBehaviour {
            floodsub: Floodsub::new(local_peer_id),
            mdns,
        };

        behaviour.floodsub.subscribe(floodsub_topic.clone());
        SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build()
    };

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Create a channel for sending messages
    let (tx, _) = mpsc::channel(64);

    let blocklattice: Arc<Mutex<BlockLattice>> = Arc::new(Mutex::new(BlockLattice::new()));
    let blocklattice_clone = Arc::clone(&blocklattice);

    // Handle incoming messages
    tokio::spawn(async move {
        loop {
            match swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                }
                SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Message(message))) => {
                    if let Ok(rpc_request) = serde_json::from_str::<serde_json::Value>(
                        &String::from_utf8_lossy(&message.data),
                    ) {
                        match handle_request(&rpc_request, Arc::clone(&blocklattice_clone)).await {
                            Ok(result) => {
                                let response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": result,
                                    "id": rpc_request["id"]
                                });
                                // Broadcast the response
                                if let Ok(response_str) = serde_json::to_string(&response) {
                                    swarm
                                        .behaviour_mut()
                                        .floodsub
                                        .publish(floodsub_topic.clone(), response_str.as_bytes());
                                }
                            }
                            Err(e) => {
                                println!("Error handling request: {:?}", e);
                            }
                        }
                    }
                }
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
    });

    // Start HTTP RPC server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr).await?;
    println!("RPC server listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;
        let tx = tx.clone();
        let blocklattice = Arc::clone(&blocklattice);

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            match socket.read(&mut buf).await {
                Ok(n) if n == 0 => return,
                Ok(n) => {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if let Ok(rpc_request) = serde_json::from_str::<serde_json::Value>(&req) {
                        // Send the request to the P2P network
                        if let Ok(req_str) = serde_json::to_string(&rpc_request) {
                            let _ = tx.send(req_str.into_bytes()).await;
                        }

                        // Also handle it locally
                        let response = match handle_request(&rpc_request, blocklattice).await {
                            Ok(result) => serde_json::json!({
                                "jsonrpc": "2.0",
                                "result": result,
                                "id": rpc_request["id"]
                            }),
                            Err(e) => serde_json::json!({
                                "jsonrpc": "2.0",
                                "error": {
                                    "code": -32603,
                                    "message": "Internal error",
                                    "data": e.to_string()
                                },
                                "id": rpc_request["id"]
                            }),
                        };

                        let _ = socket
                            .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                            .await;
                    }
                }
                Err(e) => eprintln!("Failed to read from socket: {:?}", e),
            }
        });
    }
}

async fn handle_request(
    req: &JsonValue,
    blocklattice: Arc<Mutex<BlockLattice>>,
) -> Result<String, Box<dyn Error>> {
    match req["method"].as_str() {
        Some("getBlocks") => {
            let blocklattice = blocklattice.lock().await;
            Ok(serde_json::to_string(
                &blocklattice.get_all_transaction_ids(),
            )?)
        }
        Some("getBlockById") => {
            let params = req["params"]
                .as_array()
                .ok_or("Invalid params - expected array")?;
            let id = serde_json::from_value(params[0].clone())?;
            let blocklattice = blocklattice.lock().await;
            Ok(serde_json::to_string(&blocklattice.get_transaction(id))?)
        }
        Some("submitTransaction") => {
            let params = req["params"]
                .as_array()
                .ok_or("Invalid params - expected array")?;

            let tx_request: TransactionRequest = serde_json::from_value(params[0].clone())?;

            // Create transaction from request
            let tx = Transaction::from_request(tx_request);

            let mut blocklattice = blocklattice.lock().await;

            let tx_id = blocklattice.add_transaction(tx.from, tx.to, tx.amount)?;

            Ok(tx_id)
        }
        Some(method) => Err(format!("Unknown method: {}", method).into()),
        None => Err("Missing method".into()),
    }
}

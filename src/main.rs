use anyhow::{anyhow, Result};
use clap::Parser;
use ed25519_dalek::VerifyingKey;
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
use lmdb::Transaction as LmdbTransaction;
use serde_json::Value as JsonValue;
use sha2::Digest;
use sha2::Sha256;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use tcp::tokio::Transport as TokioTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tracing::{error, info, trace, warn};
use transaction_manager::TransactionManager;

use crate::transaction::TransactionRequest;

mod address;
mod transaction;
mod transaction_manager;

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

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    genesis_file_path: String,
    #[arg(long)]
    initial_peers_file_path: String,
}

async fn handle_swarm_events(
    mut swarm: Swarm<P2PBlockchainBehaviour>,
    floodsub_topic: Topic,
    transaction_manager: Arc<Mutex<TransactionManager>>,
) {
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {:?}", address);
            }
            SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Message(message))) => {
                if let Ok(rpc_request) = serde_json::from_str::<serde_json::Value>(
                    &String::from_utf8_lossy(&message.data),
                ) {
                    match handle_request(&rpc_request, Arc::clone(&transaction_manager)).await {
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
                            error!("Error handling request: {:?}", e);
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

    let initial_peers = std::fs::read_to_string(&args.initial_peers_file_path)?
        .lines()
        .map(|s| s.parse::<Multiaddr>())
        .collect::<Result<Vec<_>, _>>()?;

    if are_all_peers_dead(initial_peers, &mut swarm) {
        error!("No initial peers are alive and reachable");
        return Err("No initial peers are alive and reachable".into());
    }

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let transaction_manger: Arc<Mutex<TransactionManager>> =
        Arc::new(Mutex::new(TransactionManager::new()?));
    let transaction_manger_clone = Arc::clone(&transaction_manger);

    // Start handling incoming messages
    tokio::spawn(handle_swarm_events(
        swarm,
        floodsub_topic.clone(),
        transaction_manger_clone,
    ));

    // Start HTTP RPC server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    let listener = TcpListener::bind(addr).await?;
    info!("RPC server listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;
        let transaction_manger = Arc::clone(&transaction_manger);

        tokio::spawn(async move {
            let mut buf = [0; 8192]; // Increased buffer size
            match socket.read(&mut buf).await {
                Ok(n) if n == 0 => {
                    trace!("Connection closed by client");
                    return;
                }
                Ok(n) => {
                    let request = String::from_utf8_lossy(&buf[..n]);

                    // Parse HTTP request to get the body
                    if let Some(body_start) = request.find("\r\n\r\n") {
                        let body = &request[body_start + 4..];
                        trace!("Request body: {}", body);

                        match serde_json::from_str::<serde_json::Value>(body) {
                            Ok(rpc_request) => {
                                match handle_request(&rpc_request, transaction_manger).await {
                                    Ok(result) => {
                                        let response = serde_json::json!({
                                            "jsonrpc": "2.0",
                                            "result": result,
                                            "id": rpc_request["id"]
                                        });

                                        let response_body =
                                            serde_json::to_string(&response).unwrap();
                                        let http_response = format!(
                                            "HTTP/1.1 200 OK\r\n\
                                             Content-Type: application/json\r\n\
                                             Content-Length: {}\r\n\
                                             \r\n\
                                             {}",
                                            response_body.len(),
                                            response_body
                                        );

                                        if let Err(e) =
                                            socket.write_all(http_response.as_bytes()).await
                                        {
                                            error!("Failed to write response: {:?}", e);
                                        }
                                    }
                                    Err(e) => {
                                        let error_response = serde_json::json!({
                                            "jsonrpc": "2.0",
                                            "error": {
                                                "code": -32603,
                                                "message": format!("Internal error: {}", e)
                                            },
                                            "id": rpc_request["id"]
                                        });

                                        let response_body =
                                            serde_json::to_string(&error_response).unwrap();
                                        let http_response = format!(
                                            "HTTP/1.1 500 Internal Server Error\r\n\
                                             Content-Type: application/json\r\n\
                                             Content-Length: {}\r\n\
                                             \r\n\
                                             {}",
                                            response_body.len(),
                                            response_body
                                        );

                                        if let Err(e) =
                                            socket.write_all(http_response.as_bytes()).await
                                        {
                                            error!("Failed to write error response: {:?}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let error_response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": -32700,
                                        "message": format!("Parse error: {}", e)
                                    },
                                    "id": null
                                });

                                let response_body = serde_json::to_string(&error_response).unwrap();
                                let http_response = format!(
                                    "HTTP/1.1 400 Bad Request\r\n\
                                     Content-Type: application/json\r\n\
                                     Content-Length: {}\r\n\
                                     \r\n\
                                     {}",
                                    response_body.len(),
                                    response_body
                                );

                                if let Err(e) = socket.write_all(http_response.as_bytes()).await {
                                    error!("Failed to write parse error response: {:?}", e);
                                }
                            }
                        }
                    } else {
                        error!("Invalid HTTP request format");
                        // Send 400 Bad Request response
                        let error_response = "HTTP/1.1 400 Bad Request\r\n\r\n";
                        if let Err(e) = socket.write_all(error_response.as_bytes()).await {
                            error!("Failed to write error response: {:?}", e);
                        }
                    }
                }
                Err(e) => error!("Failed to read from socket: {:?}", e),
            }
        });
    }
}

async fn handle_request(
    req: &JsonValue,
    transaction_manager: Arc<Mutex<TransactionManager>>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    info!("Handling request method: {:?}", req["method"]);

    match req["method"].as_str() {
        Some("submitTransaction") => {
            let params = req["params"]
                .as_array()
                .ok_or_else(|| "Invalid params - expected array")?;

            if params.is_empty() {
                return Err("Empty params array".into());
            }

            info!("Transaction params: {:?}", params[0]);

            let (tx, rx) = oneshot::channel();
            let params_clone = params[0].clone();
            let transaction_manager_clone = Arc::clone(&transaction_manager);

            thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<()> = rt.block_on(async {
                    trace!("Starting LMDB operation");

                    let transaction_manager = transaction_manager_clone.lock().await;

                    let mut txn =
                        transaction_manager
                            .transaction_env
                            .begin_rw_txn()
                            .map_err(|e| {
                                error!("Failed to begin transaction: {}", e);
                                anyhow!("Failed to begin transaction: {}", e)
                            })?;

                    // Generate a fixed-size key (e.g., using a hash of the transaction)
                    let key = {
                        let mut hasher = Sha256::new();
                        hasher.update(params_clone.to_string().as_bytes());
                        hasher.finalize().to_vec()
                    };

                    // Serialize the transaction data
                    let serialized_tx = bincode::serialize(&params_clone).map_err(|e| {
                        error!("Failed to serialize transaction: {}", e);
                        anyhow!("Failed to serialize transaction: {}", e)
                    })?;

                    let db = transaction_manager.db;
                    txn.put(db, &key, &serialized_tx, lmdb::WriteFlags::empty())
                        .map_err(|e| {
                            error!("Failed to put data: {}", e);
                            anyhow!("Failed to put data: {}", e)
                        })?;

                    txn.commit().map_err(|e| {
                        error!("Failed to commit transaction: {}", e);
                        anyhow!("Failed to commit transaction: {}", e)
                    })?;

                    trace!("LMDB operation completed successfully");
                    Ok(())
                });

                if let Err(ref e) = result {
                    error!("LMDB operation failed: {}", e);
                }
                let _ = tx.send(result);
            });

            // Wait for LMDB operation
            rx.await.map_err(|e| format!("Channel error: {}", e))??;

            let tx_request: TransactionRequest = serde_json::from_value(params[0].clone())
                .map_err(|e| format!("Failed to parse transaction request: {}", e))?;

            let mut transaction_manager = transaction_manager.lock().await;

            let tx_id = transaction_manager.add_transaction(
                tx_request.from,
                tx_request.to,
                tx_request.amount,
                VerifyingKey::from_bytes(&tx_request.public_key)
                    .map_err(|e| format!("Invalid public key: {}", e))?,
                tx_request.timestamp,
                tx_request.id,
                tx_request.signature,
            )?;

            trace!("Transaction added successfully with ID: {}", tx_id);
            Ok(tx_id)
        }
        Some(method) => {
            error!("Unknown method called: {}", method);
            Err(format!("Unknown method: {}", method).into())
        }
        None => {
            error!("Missing method in request");
            Err("Missing method".into())
        }
    }
}

use anyhow::{anyhow, Result};
use blocklattice::BlockLattice;
use ed25519_dalek::VerifyingKey;
use libp2p::futures::StreamExt;
use libp2p::mdns::tokio::Tokio;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{core::upgrade::Version, identity, noise, tcp, yamux, PeerId, Transport};
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
use std::path::Path;
use std::sync::Arc;
use std::thread;
use tcp::tokio::Transport as TokioTransport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::transaction::TransactionRequest;

mod address;
mod blocklattice;
mod transaction;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);
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

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Create a channel for sending messages
    let (tx, _) = mpsc::channel(64);

    let blocklattice: Arc<Mutex<BlockLattice>> = Arc::new(Mutex::new(BlockLattice::new(
        "confirmed_transactions".into(),
        "pending_transactions".into(),
    )));
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
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    let listener = TcpListener::bind(addr).await?;
    println!("RPC server listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;
        let tx = tx.clone();
        let blocklattice = Arc::clone(&blocklattice);

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            match socket.read(&mut buf).await {
                Ok(n) if n == 0 => {
                    println!("Connection closed by client");
                    return;
                }
                Ok(n) => {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    println!("Received request: {}", req); // Log the received request

                    if let Ok(rpc_request) = serde_json::from_str::<serde_json::Value>(&req) {
                        println!("Parsed JSON-RPC request: {:?}", rpc_request);

                        // Send the request to the P2P network
                        if let Ok(req_str) = serde_json::to_string(&rpc_request) {
                            if let Err(e) = tx.send(req_str.into_bytes()).await {
                                eprintln!("Failed to send to P2P network: {:?}", e);
                            }
                        }

                        // Handle it locally
                        match handle_request(&rpc_request, blocklattice).await {
                            Ok(result) => {
                                let response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "result": result,
                                    "id": rpc_request["id"]
                                });
                                println!("Sending response: {:?}", response);

                                match socket
                                    .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                                    .await
                                {
                                    Ok(_) => {
                                        if let Err(e) = socket.flush().await {
                                            eprintln!("Failed to flush socket: {:?}", e);
                                        }
                                    }
                                    Err(e) => eprintln!("Failed to write response: {:?}", e),
                                }
                            }
                            Err(e) => {
                                eprintln!("Error handling request: {:?}", e);
                                let error_response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "error": {
                                        "code": -32603,
                                        "message": "Internal error",
                                        "data": e.to_string()
                                    },
                                    "id": rpc_request["id"]
                                });
                                if let Err(e) = socket
                                    .write_all(
                                        serde_json::to_string(&error_response).unwrap().as_bytes(),
                                    )
                                    .await
                                {
                                    eprintln!("Failed to write error response: {:?}", e);
                                }
                            }
                        }
                    } else {
                        eprintln!("Failed to parse JSON-RPC request");
                        let error_response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": {
                                "code": -32700,
                                "message": "Parse error"
                            },
                            "id": null
                        });
                        if let Err(e) = socket
                            .write_all(serde_json::to_string(&error_response).unwrap().as_bytes())
                            .await
                        {
                            eprintln!("Failed to write parse error response: {:?}", e);
                        }
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
) -> Result<String, Box<dyn Error + Send + Sync>> {
    println!("Handling request method: {:?}", req["method"]);

    match req["method"].as_str() {
        Some("submitTransaction") => {
            let params = req["params"]
                .as_array()
                .ok_or_else(|| "Invalid params - expected array")?;

            if params.is_empty() {
                return Err("Empty params array".into());
            }

            println!("Transaction params: {:?}", params[0]);

            let (tx, rx) = oneshot::channel();
            let params_clone = params[0].clone();

            thread::spawn(move || {
                let result: Result<()> = (|| {
                    println!("Starting LMDB operation");
                    let env = lmdb::Environment::new()
                        .set_max_dbs(1)
                        // Add these configuration options
                        .set_map_size(10 * 1024 * 1024) // 10MB map size
                        .set_max_readers(126)
                        .open(&Path::new("./local_db/transaction_db"))
                        .map_err(|e| anyhow!("Failed to open LMDB environment: {}", e))?;

                    let db = env
                        .create_db(None, lmdb::DatabaseFlags::empty())
                        .map_err(|e| anyhow!("Failed to create DB: {}", e))?;

                    let mut txn = env
                        .begin_rw_txn()
                        .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

                    // Generate a fixed-size key (e.g., using a hash of the transaction)
                    let key = {
                        let mut hasher = Sha256::new();
                        hasher.update(params_clone.to_string().as_bytes());
                        hasher.finalize().to_vec()
                    };

                    // Serialize the transaction data
                    let serialized_tx = bincode::serialize(&params_clone)
                        .map_err(|e| anyhow!("Failed to serialize transaction: {}", e))?;

                    txn.put(db, &key, &serialized_tx, lmdb::WriteFlags::empty())
                        .map_err(|e| anyhow!("Failed to put data: {}", e))?;

                    txn.commit()
                        .map_err(|e| anyhow!("Failed to commit transaction: {}", e))?;

                    println!("LMDB operation completed successfully");
                    Ok(())
                })();

                if let Err(ref e) = result {
                    eprintln!("LMDB operation failed: {}", e);
                }
                let _ = tx.send(result);
            });

            // Wait for LMDB operation
            rx.await.map_err(|e| format!("Channel error: {}", e))??;

            let tx_request: TransactionRequest = serde_json::from_value(params[0].clone())
                .map_err(|e| format!("Failed to parse transaction request: {}", e))?;

            let mut blocklattice = blocklattice.lock().await;

            let tx_id = blocklattice.add_transaction(
                tx_request.from,
                tx_request.to,
                tx_request.amount,
                VerifyingKey::from_bytes(&tx_request.public_key)
                    .map_err(|e| format!("Invalid public key: {}", e))?,
                tx_request.timestamp,
                tx_request.id,
                tx_request.signature,
            )?;

            println!("Transaction added successfully with ID: {}", tx_id);
            Ok(tx_id)
        }
        Some(method) => {
            eprintln!("Unknown method called: {}", method);
            Err(format!("Unknown method: {}", method).into())
        }
        None => {
            eprintln!("Missing method in request");
            Err("Missing method".into())
        }
    }
}

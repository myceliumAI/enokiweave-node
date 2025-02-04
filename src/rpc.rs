use anyhow::{anyhow, Result};
use ed25519_dalek::VerifyingKey;
use serde_json::Value as JsonValue;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{error, info, trace, warn};

use crate::address::Address;
use crate::transaction::TransactionRequest;
use crate::transaction_manager::TransactionManager;

enum RPCRequest {
    Transfer(TransactionRequest),
    GetBalance(Address),
}

struct QueuedTransaction {
    request: RPCRequest,
    response_sender: oneshot::Sender<Result<String, String>>,
}

pub async fn run_http_rpc_server(
    transaction_manager: Arc<Mutex<TransactionManager>>,
    rpc_port: u16,
) -> Result<(), Box<dyn Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], rpc_port));
    let listener = TcpListener::bind(addr).await?;
    info!("RPC server listening on {}", addr);

    // Create channel for transaction queue
    let (tx_queue_sender, mut tx_queue_receiver) = mpsc::channel::<QueuedTransaction>(1000);

    // Spawn transaction processor task
    let transaction_manager_clone = Arc::clone(&transaction_manager);
    tokio::spawn(async move {
        process_transaction_queue(transaction_manager_clone, &mut tx_queue_receiver).await;
    });

    loop {
        let (mut socket, _) = listener.accept().await?;
        let tx_queue_sender = tx_queue_sender.clone();

        tokio::spawn(async move {
            let mut buf = [0; 8192];
            match socket.read(&mut buf).await {
                Ok(n) if n == 0 => {
                    trace!("Connection closed by client");
                    return;
                }
                Ok(n) => {
                    let request = String::from_utf8_lossy(&buf[..n]);

                    if let Some(body_start) = request.find("\r\n\r\n") {
                        let body = &request[body_start + 4..];
                        trace!("Request body: {}", body);

                        match serde_json::from_str::<serde_json::Value>(body) {
                            Ok(rpc_request) => {
                                match handle_rpc_request(&rpc_request, tx_queue_sender).await {
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

async fn process_transaction_queue(
    transaction_manager: Arc<Mutex<TransactionManager>>,
    queue_receiver: &mut mpsc::Receiver<QueuedTransaction>,
) {
    while let Some(queued_tx) = queue_receiver.recv().await {
        let result = process_single_transaction(&transaction_manager, queued_tx.request).await;

        // Convert anyhow::Error to String for response sender
        let result = result.map_err(|e| e.to_string());

        if let Err(e) = queued_tx.response_sender.send(result) {
            error!("Failed to send transaction result: {:?}", e);
        }
    }
}

async fn process_single_transaction(
    transaction_manager: &Arc<Mutex<TransactionManager>>,
    request: RPCRequest,
) -> Result<String> {
    let mut manager = transaction_manager.lock().await;

    match request {
        RPCRequest::Transfer(transaction) => {
            match manager.add_transaction(
                transaction.from,
                transaction.to,
                transaction.amount,
                VerifyingKey::from_bytes(&transaction.public_key)
                    .map_err(|e| anyhow!("Invalid public key: {}", e))?,
                transaction.timestamp,
                transaction.signature,
            ) {
                Ok(transaction_id) => {
                    trace!("Transaction added successfully with ID: {}", transaction_id);
                    Ok(transaction_id.to_string())
                }
                Err(e) => Err(anyhow!("Error processing transaction: {}", e)),
            }
        }
        RPCRequest::GetBalance(address) => {
            match manager.get_address_balance_and_selfchain_height(address) {
                Ok((res, _)) => Ok(res.to_string()),
                Err(e) => Err(anyhow!("Error getting balance: {}", e)),
            }
        }
    }
}

async fn handle_rpc_request(
    req: &JsonValue,
    tx_queue_sender: mpsc::Sender<QueuedTransaction>,
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

            let transaction_request: TransactionRequest =
                serde_json::from_value(params[0].clone())?;

            // Create response channel
            let (response_sender, response_receiver) = oneshot::channel();

            // Queue the transaction
            let queued_tx = QueuedTransaction {
                request: RPCRequest::Transfer(transaction_request),
                response_sender,
            };

            tx_queue_sender
                .send(queued_tx)
                .await
                .map_err(|e| anyhow!("Failed to queue transaction: {}", e))?;

            // Wait for processing result
            match response_receiver.await {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(e)) => Err(anyhow!(e).into()),
                Err(e) => Err(anyhow!("Failed to receive transaction result: {}", e).into()),
            }
        }
        Some("addressBalance") => {
            let params = req["params"]
                .as_str()
                .ok_or_else(|| "Invalid params - expected str")?;

            let address = Address::from_hex(params)?;
            // Create response channel
            let (response_sender, response_receiver) = oneshot::channel();

            // Create a special transaction request for balance query
            let queued_tx = QueuedTransaction {
                request: RPCRequest::GetBalance(address),
                response_sender,
            };

            tx_queue_sender
                .send(queued_tx)
                .await
                .map_err(|e| anyhow!("Failed to queue balance request: {}", e))?;

            // Wait for processing result
            match response_receiver.await {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(e)) => Err(anyhow!(e).into()),
                Err(e) => Err(anyhow!("Failed to receive balance result: {}", e).into()),
            }
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

use anyhow::Result;
use enokiweave::p2p::{Node, NodeConfig};
use libp2p::Multiaddr;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Helper function to create a test node with a random port
pub async fn create_test_node(bootstrap_peers: Vec<Multiaddr>, node_number: usize) -> Result<(Arc<Mutex<Node>>, JoinHandle<()>)> {
    info!("Creating node{} with {} bootstrap peers", node_number, bootstrap_peers.len());
    if !bootstrap_peers.is_empty() {
        info!("Bootstrap peers for node{}: {:?}", node_number, bootstrap_peers);
    }

    let config = NodeConfig {
        address: "/ip4/127.0.0.1/tcp/0".parse()?,
        health_check_interval: 1,
        bootstrap_peers,
    };

    info!("Initializing node{} with config", node_number);
    let node = Node::new(config).await?;
    info!("Node{} initialized, wrapping in Arc<Mutex>", node_number);
    let node = Arc::new(Mutex::new(node));
    let node_clone = Arc::clone(&node);
    
    // Start the node in a separate task
    info!("Spawning node{} task", node_number);
    let handle = tokio::spawn(async move {
        info!("Node{} task started", node_number);
        match node_clone.lock().await.start().await {
            Ok(_) => info!("Node{} task completed normally", node_number),
            Err(e) => warn!("Node{} task error: {}", node_number, e),
        }
    });

    // Wait for the node to start listening
    info!("Waiting for node{} to start listening", node_number);
    let mut retries = 0;
    let max_retries = 50; // 5 seconds total
    let mut listen_addr = None;

    while retries < max_retries {
        {
            let node_lock = node.lock().await;
            if let Some(addr) = node_lock.get_listen_address() {
                listen_addr = Some(addr.clone());
                info!("Node{} got listen address: {}", node_number, addr);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        retries += 1;
        if retries % 10 == 0 {
            info!("Still waiting for node{} to start listening... (attempt {}/{})", node_number, retries, max_retries);
        }
    }

    // Update the node's address if we got one
    if let Some(addr) = listen_addr {
        info!("Updating node{} config with listen address", node_number);
        {
            let mut node_lock = node.lock().await;
            node_lock.config.address = addr.clone();
            info!("Node{} listening on {}", node_number, addr);
        }

        // Give the node some time to fully initialize
        info!("Waiting for node{} to fully initialize", node_number);
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Verify the node is still running
        if handle.is_finished() {
            let err = handle.await.unwrap_err();
            return Err(anyhow::anyhow!("Node{} task ended prematurely: {:?}", node_number, err));
        }

        info!("Node{} successfully initialized", node_number);
        Ok((node, handle))
    } else {
        handle.abort();
        Err(anyhow::anyhow!("Timeout waiting for node{} to start listening", node_number))
    }
}

/// Helper function to wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(mut condition: F, timeout_secs: u64) -> bool 
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let mut last_log = std::time::Instant::now();

    loop {
        if condition().await {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        // Log progress every second
        if last_log.elapsed() >= Duration::from_secs(1) {
            info!("Still waiting for condition... ({:?} elapsed)", start.elapsed());
            last_log = std::time::Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
} 
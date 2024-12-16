use anyhow::Result;
use enokiweave::p2p::{Node, NodeConfig};
use libp2p::Multiaddr;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, debug};

/// Helper function to create a test node with a random port
pub async fn create_test_node(bootstrap_peers: Vec<Multiaddr>, node_number: usize) -> Result<(Arc<Mutex<Node>>, JoinHandle<()>)> {
    info!("Creating node{} with {} bootstrap peers", node_number, bootstrap_peers.len());
    
    let config = NodeConfig {
        address: "/ip4/127.0.0.1/tcp/0".parse()?,
        health_check_interval: 1,
        bootstrap_peers: vec![], // Start with no bootstrap peers
    };

    let node = Node::new(config).await?;
    let node = Arc::new(Mutex::new(node));
    let node_clone = Arc::clone(&node);

    let handle = tokio::spawn(async move {
        if let Err(e) = node_clone.lock().await.start().await {
            debug!("Node{} error: {}", node_number, e);
        }
    });

    // Give the node some time to initialize
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Connect to bootstrap peers if any
    if !bootstrap_peers.is_empty() {
        node.lock().await.connect_to_peers(&bootstrap_peers).await;
    }

    info!("Node{} initialized", node_number);
    Ok((node, handle))
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
            debug!("Still waiting for condition... ({:?} elapsed)", start.elapsed());
            last_log = std::time::Instant::now();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
} 
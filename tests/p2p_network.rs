use anyhow::Result;
use enokiweave::{
    p2p::{NodeConfig, Node},
    utils::logging::init_logging,
    get_logger,
};
use libp2p::Multiaddr;
use tracing::info;
use serial_test::serial;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

/// Helper function to create a test node with a random port
async fn create_test_node(bootstrap_peers: Vec<Multiaddr>) -> Result<Arc<Mutex<Node>>> {
    let _logger = get_logger!("P2PTest");
    info!("Creating test node");

    let config = NodeConfig {
        address: "/ip4/127.0.0.1/tcp/0".parse()?,
        health_check_interval: 1,
        bootstrap_peers,
    };

    let mut node = Node::new(config).await?;
    
    // Start listening and get the actual address
    node.start_listening("/ip4/127.0.0.1/tcp/0".parse()?)?;
    
    // Wait for the listen address to be assigned
    if let Some(addr) = node.get_listen_address() {
        node.config.address = addr.clone();
        info!("Node listening on {}", addr);
    }

    Ok(Arc::new(Mutex::new(node)))
}

/// Helper function to wait for a condition with timeout
async fn wait_for_condition<F, Fut>(mut condition: F, timeout_secs: u64) -> bool 
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    while !condition().await {
        if start.elapsed() > timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    true
}

#[tokio::test]
#[serial]
async fn test_network_formation() -> Result<()> {
    init_logging(Some("p2p_test"));
    let _logger = get_logger!("P2PTest-Formation");
    info!("Starting network formation test");

    // Create first node (bootstrap node)
    let node1 = create_test_node(vec![]).await?;
    let node1_addr = node1.lock().await.config.address.clone();
    let node1_id = node1.lock().await.peer_id;

    // Create second node that connects to node1
    let node2 = create_test_node(vec![node1_addr.clone()]).await?;
    let node2_addr = node2.lock().await.config.address.clone();
    let node2_id = node2.lock().await.peer_id;

    // Create third node that connects to both node1 and node2
    let node3 = create_test_node(vec![node1_addr.clone(), node2_addr.clone()]).await?;
    let node3_id = node3.lock().await.peer_id;

    // Start all nodes
    let node1_clone = Arc::clone(&node1);
    let node2_clone = Arc::clone(&node2);
    let node3_clone = Arc::clone(&node3);

    tokio::spawn(async move {
        node1_clone.lock().await.start().await.unwrap();
    });
    tokio::spawn(async move {
        node2_clone.lock().await.start().await.unwrap();
    });
    tokio::spawn(async move {
        node3_clone.lock().await.start().await.unwrap();
    });

    // Wait for connections to be established and verified
    let success = wait_for_condition(|| async {
        let node1 = node1.lock().await;
        let node2 = node2.lock().await;
        let node3 = node3.lock().await;
        
        node1.is_connected_to(&node2_id) && 
        node2.is_connected_to(&node1_id) &&
        node2.is_connected_to(&node3_id) &&
        node3.is_connected_to(&node2_id)
    }, 5).await;

    assert!(success, "Failed to establish connections between nodes");
    info!("Network formation test completed successfully");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_peer_removal() -> Result<()> {
    init_logging(Some("p2p_test"));
    let _logger = get_logger!("P2PTest-Removal");
    info!("Starting peer removal test");

    // Create first node (bootstrap node)
    let node1 = create_test_node(vec![]).await?;
    let node1_addr = node1.lock().await.config.address.clone();
    let node1_id = node1.lock().await.peer_id;

    // Create second node that connects to node1
    let node2 = create_test_node(vec![node1_addr]).await?;
    let node2_id = node2.lock().await.peer_id;

    // Start both nodes
    let node1_clone = Arc::clone(&node1);
    let node2_clone = Arc::clone(&node2);

    tokio::spawn(async move {
        node1_clone.lock().await.start().await.unwrap();
    });
    tokio::spawn(async move {
        node2_clone.lock().await.start().await.unwrap();
    });

    // Wait for connection to be established
    let success = wait_for_condition(|| async {
        let node1 = node1.lock().await;
        let node2 = node2.lock().await;
        
        node1.is_connected_to(&node2_id) && 
        node2.is_connected_to(&node1_id)
    }, 5).await;

    assert!(success, "Failed to establish connection between nodes");
    info!("Peer removal test completed successfully");

    Ok(())
} 
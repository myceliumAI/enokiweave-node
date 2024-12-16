use anyhow::Result;
use tracing::{info, warn};
use serial_test::serial;
use std::time::Duration;

mod common;
use common::{create_test_node, wait_for_condition};

#[tokio::test]
#[serial]
async fn test_network_formation() -> Result<()> {
    // Initialize logging once at the start
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
    info!("Starting network formation test");

    // Create first node (bootstrap node)
    info!("Creating bootstrap node (node1)");
    let (node1, handle1) = create_test_node(vec![], 1).await?;
    let node1_addr = node1.lock().await.config.address.clone();
    let node1_id = node1.lock().await.peer_id;
    info!("Created node1 (bootstrap) with id {} at {}", node1_id, node1_addr);

    // Verify node1 is running
    if handle1.is_finished() {
        let err = handle1.await.unwrap_err();
        return Err(anyhow::anyhow!("Node1 task ended prematurely: {:?}", err));
    }

    // Wait a bit for the first node to stabilize
    info!("Waiting for node1 to stabilize...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create second node that connects to node1
    info!("Creating node2 connecting to bootstrap node at {}", node1_addr);
    let (node2, handle2) = create_test_node(vec![node1_addr.clone()], 2).await?;
    
    // Verify node2 is running
    if handle2.is_finished() {
        handle1.abort();
        let err = handle2.await.unwrap_err();
        return Err(anyhow::anyhow!("Node2 task ended prematurely: {:?}", err));
    }

    let node2_addr = node2.lock().await.config.address.clone();
    let node2_id = node2.lock().await.peer_id;
    info!("Created node2 with id {} at {}", node2_id, node2_addr);

    // Wait for node2 to establish connection with node1
    info!("Waiting for node2 to connect to node1...");
    let n1_n2_connected = wait_for_condition(|| async {
        let node1 = node1.lock().await;
        let node2 = node2.lock().await;
        let connected = node1.is_connected_to(&node2_id) && node2.is_connected_to(&node1_id);
        info!("Connection check - Node1 -> Node2: {}, Node2 -> Node1: {}", 
            node1.is_connected_to(&node2_id),
            node2.is_connected_to(&node1_id)
        );
        connected
    }, 10).await;

    if !n1_n2_connected {
        info!("Final connection state before error:");
        {
            let node1 = node1.lock().await;
            let node2 = node2.lock().await;
            info!("Node1 peers: {:?}", node1.known_peer_ids());
            info!("Node2 peers: {:?}", node2.known_peer_ids());
        }
        handle1.abort();
        handle2.abort();
        return Err(anyhow::anyhow!("Failed to establish initial connection between node1 and node2"));
    }

    // Create third node that connects to both node1 and node2
    info!("Creating node3 connecting to both previous nodes");
    let (node3, handle3) = create_test_node(vec![node1_addr.clone(), node2_addr.clone()], 3).await?;
    let node3_id = node3.lock().await.peer_id;
    info!("Created node3 with id {}", node3_id);

    // Wait for all connections to be established and verified
    info!("Waiting for all connections to be established...");
    let success = wait_for_condition(|| async {
        let node1 = node1.lock().await;
        let node2 = node2.lock().await;
        let node3 = node3.lock().await;
        
        let n1_to_n2 = node1.is_connected_to(&node2_id);
        let n2_to_n1 = node2.is_connected_to(&node1_id);
        let n2_to_n3 = node2.is_connected_to(&node3_id);
        let n3_to_n2 = node3.is_connected_to(&node2_id);

        info!(
            "Connection status: n1->n2: {}, n2->n1: {}, n2->n3: {}, n3->n2: {}", 
            n1_to_n2, n2_to_n1, n2_to_n3, n3_to_n2
        );

        n1_to_n2 && n2_to_n1 && n2_to_n3 && n3_to_n2
    }, 30).await;

    // Cleanup
    info!("Test complete, cleaning up nodes...");
    handle1.abort();
    handle2.abort();
    handle3.abort();

    assert!(success, "Failed to establish connections between nodes");
    info!("Network formation test completed successfully");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_peer_removal() -> Result<()> {
    // Initialize logging once at the start
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    info!("Starting peer removal test");

    // Create first node (bootstrap node)
    info!("Creating bootstrap node (node1)");
    let (node1, handle1) = create_test_node(vec![], 1).await?;
    let node1_addr = node1.lock().await.config.address.clone();
    let node1_id = node1.lock().await.peer_id;
    info!("Created node1 (bootstrap) with id {} at {}", node1_id, node1_addr);

    // Wait a bit for the first node to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create second node that connects to node1
    info!("Creating node2 connecting to bootstrap node");
    let (node2, handle2) = create_test_node(vec![node1_addr.clone()], 2).await?;
    let node2_id = node2.lock().await.peer_id;
    info!("Created node2 with id {}", node2_id);

    // Wait for connection to be established
    info!("Waiting for connection to be established...");
    let success = wait_for_condition(|| async {
        let node1 = node1.lock().await;
        let node2 = node2.lock().await;
        
        let n1_to_n2 = node1.is_connected_to(&node2_id);
        let n2_to_n1 = node2.is_connected_to(&node1_id);

        info!(
            "Connection status: n1->n2: {}, n2->n1: {}", 
            n1_to_n2, n2_to_n1
        );

        n1_to_n2 && n2_to_n1
    }, 30).await;

    if !success {
        warn!("Failed to establish connection between nodes");
        return Err(anyhow::anyhow!("Failed to establish connection between nodes"));
    }

    // Verify that nodes know about each other
    {
        let node1 = node1.lock().await;
        let node2 = node2.lock().await;
        assert!(node1.knows_peer(&node2_id), "Node1 should know about Node2");
        assert!(node2.knows_peer(&node1_id), "Node2 should know about Node1");
    }

    // Simulate node2 disconnection by aborting its task
    info!("Simulating node2 disconnection...");
    handle2.abort();

    // Wait for node1 to detect node2's removal
    info!("Waiting for node1 to detect node2's removal...");
    let removal_detected = wait_for_condition(|| async {
        let node1 = node1.lock().await;
        let removed = !node1.knows_peer(&node2_id);
        info!("Node1 knows node2: {}", !removed);
        removed
    }, 30).await;

    // Cleanup
    info!("Test complete, cleaning up nodes...");
    handle1.abort();

    assert!(removal_detected, "Node1 should have removed Node2 after disconnection");
    info!("Peer removal test completed successfully");

    Ok(())
} 
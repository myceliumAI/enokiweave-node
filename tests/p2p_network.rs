use anyhow::Result;
use tracing::info;
use serial_test::serial;
use std::time::Duration;

mod common;
use common::utils::{create_test_node, wait_for_condition};

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

    // Wait for the node to get a listen address
    let mut retries = 0;
    let max_retries = 30;
    let (node1_addr, node1_id) = loop {
        if let Some(addr) = node1.lock().await.get_listen_address() {
            let id = node1.lock().await.peer_id;
            break (addr, id);
        }
        if retries >= max_retries {
            handle1.abort();
            return Err(anyhow::anyhow!("Timeout waiting for node1 to get listen address"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        retries += 1;
    };

    info!("Created node1 (bootstrap) with id {} at {}", node1_id, node1_addr);

    // Create second node that connects to node1
    info!("Creating node2 connecting to bootstrap node at {}", node1_addr);
    let (node2, handle2) = create_test_node(vec![node1_addr.clone()], 2).await?;
    let node2_id = node2.lock().await.peer_id;
    let node2_addr = node2.lock().await.get_listen_address()
        .ok_or_else(|| anyhow::anyhow!("Failed to get node2 listen address"))?;
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

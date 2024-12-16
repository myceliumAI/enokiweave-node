use anyhow::Result;
use enokiweave::{
    p2p::{Node, NodeConfig},
    utils::logging,
};
use libp2p::Multiaddr;
use std::time::Duration;
use tokio::{time::sleep, sync::mpsc};

/// Helper function to create a node with a specific port
pub async fn create_test_node(port: u16, bootstrap_addr: Option<Multiaddr>) -> Result<(Node, mpsc::Sender<()>)> {
    let addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse()?;
    let config = match bootstrap_addr {
        Some(bootstrap) => NodeConfig::new(addr, 5, vec![bootstrap]), // Use shorter intervals for testing
        None => NodeConfig::standalone(addr, 5),
    };
    let node = Node::new(config).await?;
    let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);
    Ok((node, shutdown_tx))
}

/// Helper function to wait for an async condition with timeout
pub async fn wait_for_condition<F, Fut>(condition: F, timeout: Duration) -> Result<bool>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while !condition().await {
        if start.elapsed() > timeout {
            return Ok(false);
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(true)
}

/// Initialize test logging with a unique prefix
pub fn init_test_logging(test_name: &str) {
    logging::init_logging(Some(test_name));
} 
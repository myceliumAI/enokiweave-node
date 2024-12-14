mod node;

use libp2p::Multiaddr;
use std::error::Error;
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    // Create a node listening on localhost:8000
    let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8000".parse()?;
    let health_check_interval = 30; // seconds
    
    let mut node = node::Node::new(addr, health_check_interval).await?;
    
    // Start the node
    node.start().await?;

    Ok(())
}

use enokiweave::p2p::{Node, NodeConfig};
use libp2p::Multiaddr;
use std::error::Error;
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    // Get command line arguments
    let args: Vec<String> = std::env::args().collect();
    let node_type = args.get(1).map(|s| s.as_str()).unwrap_or("bootstrap");
    let port = args.get(2).map(|s| s.parse::<u16>().unwrap_or(8000)).unwrap_or(8000);

    // Create node configuration based on node type
    let addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse()?;
    
    let config = match node_type {
        "bootstrap" => {
            println!("Starting bootstrap node on port {}", port);
            NodeConfig::bootstrap_node(addr, 30)
        }
        "regular" => {
            println!("Starting regular node on port {} with bootstrap connection", port);
            // Connect to the bootstrap node (assuming it's running on port 8000)
            let bootstrap_addr: Multiaddr = "/ip4/127.0.0.1/tcp/8000".parse()?;
            NodeConfig::new(
                addr,
                30,  // health check interval in seconds
                vec![bootstrap_addr], // connect to bootstrap node
                false,  // don't use default bootstrap nodes
            )
        }
        _ => {
            println!("Unknown node type '{}', defaulting to bootstrap node", node_type);
            NodeConfig::bootstrap_node(addr, 30)
        }
    };
    
    // Create and start the node
    let mut node = Node::new(config).await?;
    node.start().await?;

    Ok(())
}

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
    let node_type = args.get(1).map(|s| s.as_str()).unwrap_or("standalone");
    let port = args.get(2).map(|s| s.parse::<u16>().unwrap_or(8000)).unwrap_or(8000);
    let bootstrap_addr = args.get(3);

    // Create node configuration based on node type
    let addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse()?;
    
    let config = match (node_type, bootstrap_addr) {
        ("standalone", _) => {
            println!("Starting standalone node on port {}", port);
            NodeConfig::standalone(addr, 30)
        }
        (_, Some(bootstrap)) => {
            println!("Starting node on port {} with bootstrap connection to {}", port, bootstrap);
            let bootstrap_addr: Multiaddr = bootstrap.parse()?;
            NodeConfig::new(
                addr,
                30,
                vec![bootstrap_addr],
            )
        }
        _ => {
            println!("Starting standalone node on port {}", port);
            NodeConfig::standalone(addr, 30)
        }
    };
    
    // Create and start the node
    let mut node = Node::new(config).await?;
    node.start().await?;

    Ok(())
}

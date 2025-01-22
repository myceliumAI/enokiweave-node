use address::Address;
use anyhow::Result;
use clap::Parser;
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use serde_json::json;

mod address;
mod transaction;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    private_key: String,

    #[arg(long)]
    amount: u64,

    #[arg(long)]
    sender: String,

    #[arg(long)]
    recipient: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Convert hex private key to bytes
    let private_key_bytes = hex::decode(args.private_key).expect("Invalid private key hex");
    let private_key_array: [u8; 32] = private_key_bytes
        .try_into()
        .expect("Private key must be 32 bytes");

    let signing_key = SigningKey::from_bytes(&private_key_array);

    // Convert hex addresses to bytes
    let sender_bytes = hex::decode(args.sender).expect("Invalid sender address hex");
    let sender_array: [u8; 32] = sender_bytes
        .try_into()
        .expect("Sender address must be 32 bytes");

    let recipient_bytes = hex::decode(args.recipient).expect("Invalid recipient address hex");
    let recipient_array: [u8; 32] = recipient_bytes
        .try_into()
        .expect("Recipient address must be 32 bytes");

    let tx = transaction::RawTransaction::new(
        Address::from(sender_array),
        Address::from(recipient_array),
        args.amount,
    )?;

    let signature = signing_key.sign(&tx.id.0);

    let json_output = json!({
        "jsonrpc": "2.0",
        "method": "submitTransaction",
        "params": [{
            "from": hex::encode(tx.from),
            "to": hex::encode(tx.to),
            "amount": tx.amount,
            "public_key": hex::encode(signing_key.verifying_key().as_bytes()),
            "signature": {
                "R": hex::encode(signature.r_bytes()),
                "s": hex::encode(signature.s_bytes())
            },
            "timestamp": tx.timestamp,
            "id": hex::encode(tx.id.0)
        }]
    });

    println!("{}", serde_json::to_string_pretty(&json_output)?);

    Ok(())
}

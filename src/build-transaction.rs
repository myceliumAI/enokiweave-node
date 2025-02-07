use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::Parser;
use k256::ecdsa::signature::DigestSigner;
use k256::ecdsa::signature::Signer;
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::{Signature, SigningKey};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::SecretKey;
use serde_json::json;

use enokiweave::address::Address;
use enokiweave::confidential::EncryptedExactAmount;
use enokiweave::transaction::Amount;
use enokiweave::transaction::Transaction;
use enokiweave::transaction::TransactionHash;
use sha2::Digest;
use sha2::Sha256;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    private_key: String,

    #[arg(long)]
    sender: String,

    #[arg(long)]
    amount: u64,

    #[arg(long)]
    recipient: String,

    #[arg(long)]
    previous_transaction_id: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Convert hex private key to bytes
    let private_key_bytes = hex::decode(args.private_key).expect("Invalid private key hex");
    let private_key_array: [u8; 32] = private_key_bytes
        .try_into()
        .expect("Private key must be 32 bytes");

    let secret_key = SecretKey::from_bytes(&private_key_array.into())?;
    let public_key = secret_key.public_key();

    // Convert hex addresses to bytes
    let sender_bytes = hex::decode(args.sender).expect("Invalid sender address hex");
    let sender_array: [u8; 32] = sender_bytes
        .try_into()
        .expect("Sender address must be 32 bytes");

    let recipient_bytes = hex::decode(args.recipient).expect("Invalid recipient address hex");
    let recipient_array: [u8; 32] = recipient_bytes
        .try_into()
        .expect("Recipient address must be 32 bytes");

    let previous_transaction_id_bytes =
        hex::decode(args.previous_transaction_id).expect("Invalid previous_transaction_id hex");
    let previous_transaction_id_array: [u8; 32] = previous_transaction_id_bytes
        .try_into()
        .expect("Previous transaction ID must be 32 bytes");

    let encrypted = EncryptedExactAmount::encrypt(args.amount, &public_key)?;

    let tx = Transaction::new(
        Address::from(sender_array),
        Address::from(recipient_array),
        Amount::Confidential(encrypted.clone()),
        TransactionHash(previous_transaction_id_array),
    )?;

    // Calculate the message hash
    let message = tx.calculate_id()?;

    // Create signing key and sign
    let signing_key = SigningKey::from_bytes(&private_key_array.into())?;
    let verifying_key = signing_key.verifying_key();

    // Sign using the finalized message hash
    let signature: Signature = signing_key.sign(&message);
    let signature_bytes = signature.to_bytes();

    // Verify the signature
    verifying_key
        .verify(&message, &signature)
        .map_err(|e| anyhow!("Invalid signature: {}", e))?;

    let json_output = json!({
        "jsonrpc": "2.0",
        "method": "submitTransaction",
        "params": [{
            "from": hex::encode(tx.from),
            "to": hex::encode(tx.to),
            "amount": {
                "Confidential": {
                    "range_proof": BASE64.encode(encrypted.range_proof.to_bytes()),
                    "c1": BASE64.encode(encrypted.c1.to_affine().to_encoded_point(true).as_bytes()),
                    "c2": BASE64.encode(encrypted.c2.to_affine().to_encoded_point(true).as_bytes())
                }
            },
            "public_key": hex::encode(verifying_key.to_encoded_point(false).as_bytes()),
            "signature": {
                "R": hex::encode(&signature_bytes[..32]),
                "s": hex::encode(&signature_bytes[32..])
            },
            "previous_transaction_id": hex::encode(previous_transaction_id_array),
            "timestamp": tx.timestamp,
        }],
        "id": 1
    });

    println!("{}", serde_json::to_string_pretty(&json_output)?);

    Ok(())
}

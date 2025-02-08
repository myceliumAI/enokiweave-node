use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::Parser;
use enokiweave::transaction::EncryptedAmountProofs;
use k256::ecdsa::signature::Signer;
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::{Signature, SigningKey};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;
use k256::SecretKey;
use serde_json::json;

use enokiweave::address::Address;
use enokiweave::confidential::EncryptedExactAmount;
use enokiweave::transaction::Amount;
use enokiweave::transaction::Transaction;
use enokiweave::transaction::TransactionHash;

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
    let private_key_bytes = hex::decode(&args.private_key)
        .with_context(|| format!("Failed to decode private key hex: {}", args.private_key))?;
    let private_key_array: [u8; 32] = private_key_bytes
        .try_into()
        .map_err(|_| anyhow!("Private key must be exactly 32 bytes"))?;

    let secret_key = SecretKey::from_bytes(&private_key_array.into())
        .context("Failed to create secret key from bytes")?;
    let public_key = secret_key.public_key();

    // Convert hex addresses to bytes
    let sender_bytes = hex::decode(&args.sender)
        .with_context(|| format!("Failed to decode sender address hex: {}", args.sender))?;
    let sender_array: [u8; 32] = sender_bytes
        .try_into()
        .map_err(|_| anyhow!("Sender address must be exactly 32 bytes"))?;

    let recipient_bytes = hex::decode(&args.recipient)
        .with_context(|| format!("Failed to decode recipient address hex: {}", args.recipient))?;
    let recipient_array: [u8; 32] = recipient_bytes
        .try_into()
        .map_err(|_| anyhow!("Recipient address must be exactly 32 bytes"))?;

    let previous_transaction_id_bytes =
        hex::decode(&args.previous_transaction_id).with_context(|| {
            format!(
                "Failed to decode previous transaction ID hex: {}",
                args.previous_transaction_id
            )
        })?;
    let previous_transaction_id_array: [u8; 32] = previous_transaction_id_bytes
        .try_into()
        .map_err(|_| anyhow!("Previous transaction ID must be exactly 32 bytes"))?;

    let sender_encrypted = EncryptedExactAmount::encrypt(args.amount, &public_key)
        .context("Failed to encrypt amount for sender")?;
    let recipient_encrypted = EncryptedExactAmount::encrypt(
        args.amount,
        &PublicKey::from_sec1_bytes(
            &[0x02]
                .iter()
                .chain(recipient_array.iter())
                .copied()
                .collect::<Vec<u8>>(),
        )
        .context("Failed to create recipient public key")?,
    )
    .context("Failed to encrypt amount for recipient")?;

    // Quorum encryption
    let quorum_public_key = PublicKey::from_sec1_bytes(&[
        0x02, 0x79, 0xBE, 0x66, 0x7E, 0xF9, 0xDC, 0xBB, 0xAC, 0x55, 0xA0, 0x62, 0x95, 0xCE, 0x87,
        0x0B, 0x07, 0x02, 0x9B, 0xFC, 0xDB, 0x2D, 0xCE, 0x28, 0xD9, 0x59, 0xF2, 0x81, 0x5B, 0x16,
        0xF8, 0x17, 0x98,
    ])
    .context("Failed to create quorum public key")?;

    let quorum_encrypted = EncryptedExactAmount::encrypt(args.amount, &quorum_public_key)
        .context("Failed to encrypt amount for quorum")?;

    let tx = Transaction::new(
        Address::from(sender_array),
        Address::from(recipient_array),
        Amount::Confidential(EncryptedAmountProofs {
            sender: sender_encrypted.clone(),
            recipient: recipient_encrypted.clone(),
            quorum: quorum_encrypted.clone(),
        }),
        TransactionHash(previous_transaction_id_array),
    )
    .context("Failed to create transaction")?;

    let message = tx
        .calculate_id()
        .context("Failed to calculate transaction ID")?;

    let signing_key = SigningKey::from_bytes(&private_key_array.into())
        .context("Failed to create signing key")?;
    let verifying_key = signing_key.verifying_key();

    let signature: Signature = signing_key.sign(&message);
    let signature_bytes = signature.to_bytes();

    verifying_key
        .verify(&message, &signature)
        .context("Signature verification failed")?;

    let json_output = json!({
        "jsonrpc": "2.0",
        "method": "submitTransaction",
        "params": [{
            "from": hex::encode(tx.from),
            "to": hex::encode(tx.to),
            "amount": {
                "Confidential": {
                    "sender": {
                        "range_proof": BASE64.encode(sender_encrypted.range_proof.to_bytes()),
                        "c1": BASE64.encode(sender_encrypted.c1.to_affine().to_encoded_point(true).as_bytes()),
                        "c2": BASE64.encode(sender_encrypted.c2.to_affine().to_encoded_point(true).as_bytes())
                    },
                    "recipient": {
                        "range_proof": BASE64.encode(recipient_encrypted.range_proof.to_bytes()),
                        "c1": BASE64.encode(recipient_encrypted.c1.to_affine().to_encoded_point(true).as_bytes()),
                        "c2": BASE64.encode(recipient_encrypted.c2.to_affine().to_encoded_point(true).as_bytes())
                    },
                    "quorum": {
                        "range_proof": BASE64.encode(quorum_encrypted.range_proof.to_bytes()),
                        "c1": BASE64.encode(quorum_encrypted.c1.to_affine().to_encoded_point(true).as_bytes()),
                        "c2": BASE64.encode(quorum_encrypted.c2.to_affine().to_encoded_point(true).as_bytes())
                    }
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

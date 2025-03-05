use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use k256::ecdsa::{Signature, SigningKey, signature::Signer};
use k256::elliptic_curve::group::GroupEncoding;
use k256::elliptic_curve::rand_core::OsRng;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{ProjectivePoint, SecretKey as K256SecretKey, PublicKey, Scalar, NonZeroScalar};
use serde_json::json;
use sha2::digest::generic_array::GenericArray;
use sha2::digest::typenum;
use tokio::runtime::Runtime;

use crate::address::Address;
use crate::confidential::{EncryptedExactAmount, ShieldedAmount};
use crate::transaction::{Input, Output, PublicInput, PublicOutput, ShieldedInput, ShieldedOutput, Transaction};
use crate::transaction_hash::TransactionHash;

#[derive(Debug)]
pub struct Utxo {
    pub amount: Option<u64>, // None for confidential amounts
    pub owner: Option<Address>,
    pub value_commitment: Option<ProjectivePoint>,
    pub spending_key_commitment: Option<ProjectivePoint>,
    pub nullifier: Option<ProjectivePoint>,
    pub encrypted_amount: Option<EncryptedExactAmount>,
}

pub async fn fetch_utxo(_txid: &str, _output_index: usize) -> Result<Utxo> {
    // TODO: Implement actual fetching from chain
    // For now return dummy data for testing
    Ok(Utxo {
        amount: Some(100), // Fixed test amount
        owner: None,
        value_commitment: None,
        spending_key_commitment: None,
        nullifier: None,
        encrypted_amount: None,
    })
}

pub fn decode_recipient_pubkey(recipient_pubkey: &str) -> Result<PublicKey> {
    let recipient_pubkey_bytes = hex::decode(recipient_pubkey)
        .context("Failed to decode recipient public key hex")?;
    PublicKey::from_sec1_bytes(&recipient_pubkey_bytes)
        .map_err(|e| anyhow!("Invalid recipient public key: {}", e))
}

pub fn process_confidential_input(
    input_keys_by_id: &str,
    recipient_pubkey: &PublicKey,
    _k256_blinding: Scalar,
) -> Result<(Vec<Input>, Option<(K256SecretKey, PublicKey)>)> {
    let rt = Runtime::new()?;
    let mut first_spending_key = None;
    let tx_inputs = input_keys_by_id
        .split(';')
        .map(|pair| {
            let mut split = pair.split(':');
            let spend_key = split
                .next()
                .ok_or_else(|| anyhow!("Missing spend key in input pair"))?
                .to_string();
            let txid = split
                .next()
                .ok_or_else(|| anyhow!("Missing txid in input pair"))?
                .to_string();
            let output_index = split
                .next()
                .ok_or_else(|| anyhow!("Missing output index in input pair"))?
                .parse::<usize>()?;

            // Fetch UTXO data
            let utxo = rt.block_on(fetch_utxo(&txid, output_index))?;

            // Verify the UTXO exists and is confidential
            if utxo.value_commitment.is_none() {
                return Err(anyhow!("UTXO is not confidential"));
            }

            let spend_key_bytes: [u8; 32] = hex::decode(spend_key.clone())
                .with_context(|| format!("Failed to decode spend key hex: {}", spend_key))?
                .try_into()
                .map_err(|_| anyhow!("Spend key must be exactly 32 bytes"))?;

            let mut spending_key_array = GenericArray::<u8, typenum::U32>::default();
            spending_key_array.copy_from_slice(&spend_key_bytes);
            let spending_key = K256SecretKey::from_bytes(&spending_key_array)?;
            
            // Store first spending key for output creation
            if first_spending_key.is_none() {
                first_spending_key = Some((spending_key.clone(), recipient_pubkey.clone()));
            }

            // Create shielded input using the UTXO data
            let encrypted_amount = utxo.encrypted_amount.unwrap();
            let shielded_amount = ShieldedAmount {
                value_commitment: utxo.value_commitment.unwrap(),
                nullifier: utxo.nullifier.unwrap(),
                range_proof: encrypted_amount.range_proof.clone(),
                spending_key_commitment: utxo.spending_key_commitment.unwrap(),
                encrypted_amount: encrypted_amount,
            };

            let shielded_input = ShieldedInput::new(shielded_amount, &spending_key)?;
            
            Ok(Input::Confidential(shielded_input))
        })
        .collect::<Result<Vec<Input>>>()?;

    Ok((tx_inputs, first_spending_key))
}

pub fn process_public_input(
    input_address: &str,
    amount: u64,
    _txid: &str,
    _output_index: usize,
) -> Result<Input> {
    let address = Address::from_hex(input_address)?;
    Ok(Input::Public(PublicInput {
        amount,
        owner: address,
    }))
}

pub fn create_confidential_output(
    amount: u64,
    spending_key: &K256SecretKey,
    recipient_pubkey: &PublicKey,
    k256_blinding: Scalar,
) -> Result<Output> {
    let output = ShieldedOutput::new(
        amount,
        spending_key,
        recipient_pubkey,
        k256_blinding,
    )?;
    Ok(Output::Confidential(output))
}

pub fn create_public_output(
    amount: u64,
    recipient_address: &str,
) -> Result<Output> {
    let recipient = Address::from_hex(recipient_address)?;
    Ok(Output::Public(PublicOutput {
        amount,
        recipient,
    }))
}

pub fn generate_blinding_factor() -> Result<Scalar> {
    let scalar = NonZeroScalar::random(&mut OsRng);
    Ok(*scalar.as_ref())
}

pub fn create_and_sign_transaction(
    inputs: Vec<Input>,
    outputs: Vec<Output>,
) -> Result<(Transaction, Signature)> {
    let tx = Transaction::new(inputs, outputs, TransactionHash([0; 32]))?;
    tx.verify_amounts_consistency()?;

    let signing_key = SigningKey::random(&mut OsRng);
    let signature = signing_key.sign(&tx.calculate_id()?);

    Ok((tx, signature))
}

pub fn create_transaction_json(tx: &Transaction, signature_bytes: &[u8]) -> Result<serde_json::Value> {
    let json_inputs: Vec<_> = tx.inputs.iter().map(|input| {
        match input {
            Input::Confidential(shielded_input) => {
                json!({
                    "type": "confidential",
                    "merkle_proof": {
                        "path": shielded_input.merkle_proof.path.iter().map(|(point, is_right)| {
                            json!({
                                "point": hex::encode(point.to_bytes()),
                                "is_right": is_right
                            })
                        }).collect::<Vec<_>>(),
                        "root": hex::encode(shielded_input.merkle_proof.root.to_bytes()),
                        "leaf_index": shielded_input.merkle_proof.leaf_index
                    },
                    "spending_proof": {
                        "nullifier_commitment": hex::encode(shielded_input.spending_proof.nullifier_commitment.to_bytes()),
                        "value_commitment": hex::encode(shielded_input.spending_proof.value_commitment.to_bytes()),
                        "proof": BASE64.encode(shielded_input.spending_proof.proof.to_bytes())
                    }
                })
            },
            Input::Public(public_input) => {
                json!({
                    "type": "public",
                    "amount": public_input.amount,
                    "owner": public_input.owner.as_hex()
                })
            }
        }
    }).collect();

    let json_outputs: Vec<_> = tx.outputs.iter().map(|output| {
        match output {
            Output::Confidential(shielded_output) => {
                json!({
                    "type": "confidential",
                    "value_commitment": hex::encode(shielded_output.value_commitment.to_bytes()),
                    "range_proof": BASE64.encode(shielded_output.range_proof.to_bytes()),
                    "spending_key_commitment": hex::encode(shielded_output.spending_key_commitment.to_bytes()),
                    "nullifier": hex::encode(shielded_output.nullifier.to_bytes()),
                    "recipient_public_key": hex::encode(shielded_output.recipient_public_key.to_encoded_point(false).as_bytes()),
                    "encrypted_amount": {
                        "c1": BASE64.encode(shielded_output.encrypted_amount.c1.to_affine().to_encoded_point(false).as_bytes()),
                        "c2": BASE64.encode(shielded_output.encrypted_amount.c2.to_affine().to_encoded_point(false).as_bytes()),
                        "range_proof": BASE64.encode(shielded_output.encrypted_amount.range_proof.to_bytes())
                    }
                })
            },
            Output::Public(public_output) => {
                json!({
                    "type": "public",
                    "amount": public_output.amount,
                    "recipient": public_output.recipient.as_hex()
                })
            }
        }
    }).collect();

    Ok(json!({
        "jsonrpc": "2.0",
        "method": "submitTransaction",
        "params": [{
            "inputs": json_inputs,
            "outputs": json_outputs,
            "signature": {
                "R": hex::encode(&signature_bytes[..32]),
                "s": hex::encode(&signature_bytes[32..])
            },
            "previous_transaction_id": hex::encode(tx.previous_transaction_id.0),
            "timestamp": tx.timestamp,
        }],
        "id": 1
    }))
}

pub fn build_transaction(
    amount: u64,
    input_type: &str,
    output_type: &str,
    input_keys_by_id: Option<&str>,
    recipient_pubkey: Option<&str>,
    input_address: Option<&str>,
    output_address: Option<&str>,
) -> Result<serde_json::Value> {
    let mut outputs = Vec::new();
    let mut first_spending_key = None;

    // For public-to-confidential transactions:
    // 1. Public input uses Scalar::ZERO as blinding factor
    // 2. Confidential output uses -Scalar::ZERO as blinding factor to ensure they sum to zero
    let k256_blinding = if input_type == "public" && output_type == "confidential" {
        -Scalar::ZERO // Negative of zero is still zero, but explicitly showing the logic
    } else {
        generate_blinding_factor()?
    };

    let inputs = match input_type {
        "confidential" => {
            let input_keys_by_id = input_keys_by_id
                .ok_or_else(|| anyhow!("input_keys_by_id required for confidential inputs"))?;
            
            let recipient_pubkey = recipient_pubkey
                .ok_or_else(|| anyhow!("recipient_pubkey required for confidential outputs"))?;
            let recipient_pubkey = decode_recipient_pubkey(recipient_pubkey)?;

            // Parse input keys and create inputs
            let (tx_inputs, first_spending_key_opt) = process_confidential_input(
                input_keys_by_id,
                &recipient_pubkey,
                k256_blinding,
            )?;

            first_spending_key = first_spending_key_opt;

            Ok(tx_inputs)
        }
        "public" => {
            let input_address = input_address
                .ok_or_else(|| anyhow!("input_address required for public inputs"))?;
            Ok(vec![process_public_input(input_address, amount, "", 0)?])
        }
        _ => Err(anyhow!("Invalid input type")),
    }?;

    // Create output based on input type
    match output_type {
        "confidential" => {
            let (spending_key, recipient_pubkey) = if let Some(key_pair) = first_spending_key {
                // Use existing key pair from confidential input
                key_pair
            } else {
                // Generate new spending key for public-to-confidential transactions
                let recipient_pubkey = recipient_pubkey
                    .ok_or_else(|| anyhow!("recipient_pubkey required for confidential outputs"))?;
                let recipient_pubkey = decode_recipient_pubkey(recipient_pubkey)?;
                
                // Generate a new random spending key
                let spending_key = K256SecretKey::random(&mut OsRng);
                
                (spending_key, recipient_pubkey)
            };

            let output = create_confidential_output(
                amount,
                &spending_key,
                &recipient_pubkey,
                k256_blinding,
            )?;
            outputs.push(output);
        }
        "public" => {
            let recipient_address = output_address
                .ok_or_else(|| anyhow!("output_address required for public outputs"))?;
            let output = create_public_output(amount, recipient_address)?;
            outputs.push(output);
        }
        _ => return Err(anyhow!("Invalid output type")),
    }

    // Create and sign transaction
    let (tx, signature) = create_and_sign_transaction(inputs, outputs)?;
    let signature_bytes = signature.to_bytes();

    // Create JSON output
    create_transaction_json(&tx, &signature_bytes)
}

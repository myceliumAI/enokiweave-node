use crate::address::Address;
use anyhow::Result;
use chrono::Utc;
use ed25519_dalek::Signature;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub [u8; 32]);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionRequest {
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub public_key: [u8; 32],
    #[serde(deserialize_with = "deserialize_signature")]
    pub signature: Signature,
}

fn deserialize_signature<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_sig = RawSignature::deserialize(deserializer)?;

    // Combine R and s into a single 64-byte array
    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(&raw_sig.R);
    combined.extend_from_slice(&raw_sig.s);

    // Convert to array and create Signature
    let sig_bytes: [u8; 64] = combined.try_into().map_err(serde::de::Error::custom)?;

    Signature::from_bytes(&sig_bytes).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub timestamp: i64,
    pub signature: Option<Signature>,
}

impl Transaction {
    pub fn new(from: Address, to: Address, amount: u64) -> Result<Self> {
        let timestamp = Utc::now().timestamp_millis();

        let id = Self::calculate_id(from, to, amount, timestamp)?;

        Ok(Self {
            id: TransactionId(id),
            from,
            to,
            amount,
            timestamp: Utc::now().timestamp_millis(),
            signature: None,
        })
    }

    pub fn calculate_id(
        from: Address,
        to: Address,
        amount: u64,
        timestamp: i64,
    ) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();
        hasher.update(amount.to_be_bytes());
        hasher.update(&from);
        hasher.update(&to);
        hasher.update(timestamp.to_be_bytes());

        let hash = &hasher.finalize()[..];

        let id: [u8; 32] = hash.try_into().expect("Wrong length");

        Ok(id)
    }
}

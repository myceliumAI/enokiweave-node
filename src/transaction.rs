use crate::address::Address;
use anyhow::Result;
use chrono::Utc;
use ed25519_dalek::Signature;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionHash(pub [u8; 32]);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionRequest {
    #[serde(deserialize_with = "deserialize_hex_to_address")]
    pub from: Address,
    #[serde(deserialize_with = "deserialize_hex_to_address")]
    pub to: Address,
    pub amount: u64,
    #[serde(deserialize_with = "deserialize_hex_to_bytes")]
    pub public_key: [u8; 32],
    #[serde(deserialize_with = "deserialize_signature")]
    pub signature: Signature,
    pub timestamp: i64,
    #[serde(deserialize_with = "deserialize_hex_to_tx_id")]
    pub id: TransactionHash,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct SignatureComponents {
    R: String,
    s: String,
}

fn deserialize_signature<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
    D: Deserializer<'de>,
{
    let components = SignatureComponents::deserialize(deserializer)?;

    #[allow(non_snake_case)]
    let R_bytes = hex::decode(components.R).map_err(de::Error::custom)?;
    #[allow(non_snake_case)]
    let mut R_array = [0u8; 32];
    if R_array.len() != 32 {
        return Err(de::Error::custom("Invalid length for R"));
    }
    R_array.copy_from_slice(&R_bytes);

    let s_bytes = hex::decode(components.s).map_err(de::Error::custom)?;
    let mut s_array = [0u8; 32];
    if s_array.len() != 32 {
        return Err(de::Error::custom("Invalid length for s"));
    }
    s_array.copy_from_slice(&s_bytes);

    // Combine R and s into a single 64-byte array
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&R_array);
    sig_bytes[32..].copy_from_slice(&s_array);

    Ok(Signature::from_bytes(&sig_bytes))
}

fn deserialize_hex_to_bytes<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).map_err(de::Error::custom)?;
    let mut array = [0u8; 32];
    if bytes.len() != 32 {
        return Err(de::Error::custom("Invalid length for byte array"));
    }
    array.copy_from_slice(&bytes);
    Ok(array)
}

fn deserialize_hex_to_address<'de, D>(deserializer: D) -> Result<Address, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).map_err(de::Error::custom)?;
    let mut array = [0u8; 32];
    if bytes.len() != 32 {
        return Err(de::Error::custom("Invalid length for byte array"));
    }
    array.copy_from_slice(&bytes);
    Ok(Address::from(array))
}
fn deserialize_hex_to_tx_id<'de, D>(deserializer: D) -> Result<TransactionHash, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).map_err(de::Error::custom)?;
    let mut array = [0u8; 32];
    if bytes.len() != 32 {
        return Err(de::Error::custom("Invalid length for byte array"));
    }
    array.copy_from_slice(&bytes);
    Ok(TransactionHash(array))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transaction {
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub timestamp: i64,
}

impl Transaction {
    pub fn new(from: Address, to: Address, amount: u64) -> Result<Self> {
        Ok(Self {
            from,
            to,
            amount,
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    pub fn calculate_id(&self) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();
        hasher.update(self.amount.to_be_bytes());
        hasher.update(self.from);
        hasher.update(self.to);
        hasher.update(self.timestamp.to_be_bytes());

        let hash = &hasher.finalize()[..];
        let id: [u8; 32] = hash.try_into().expect("Wrong length");
        Ok(id)
    }
}

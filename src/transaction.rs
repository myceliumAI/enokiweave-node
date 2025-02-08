use anyhow::Result;
use chrono::Utc;
use k256::ecdsa::Signature;
use k256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::address::Address;
use crate::confidential::EncryptedExactAmount;
use crate::serialization::signature::{deserialize_signature, serialize_signature};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionHash(pub [u8; 32]);

impl From<[u8; 32]> for TransactionHash {
    fn from(tx_id: [u8; 32]) -> TransactionHash {
        TransactionHash(tx_id)
    }
}

impl AsRef<[u8; 32]> for TransactionHash {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionRequest {
    #[serde(deserialize_with = "deserialize_hex_to_address")]
    pub from: Address,
    #[serde(deserialize_with = "deserialize_hex_to_address")]
    pub to: Address,
    pub amount: Amount,
    #[serde(
        deserialize_with = "deserialize_hex_to_public_key",
        serialize_with = "serialize_public_key"
    )]
    pub public_key: PublicKey,
    #[serde(
        deserialize_with = "deserialize_signature",
        serialize_with = "serialize_signature"
    )]
    pub signature: Signature,
    pub timestamp: i64,
    #[serde(deserialize_with = "deserialize_hex_to_tx_id")]
    pub previous_transaction_id: TransactionHash,
}

fn deserialize_hex_to_public_key<'de, D>(deserializer: D) -> Result<PublicKey, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).map_err(de::Error::custom)?;

    // For ECDSA, the public key is 65 bytes (uncompressed) or 33 bytes (compressed)
    if bytes.len() == 65 && bytes[0] == 0x04 {
        // This is an uncompressed public key
        // let key_bytes = &bytes[1..]; // Remove the 0x04 prefix
        // println!("{:?}", key_bytes);
        PublicKey::from_sec1_bytes(&bytes)
            .map_err(|e| de::Error::custom(format!("Invalid public key: {}", e)))
    } else if bytes.len() == 33 && (bytes[0] == 0x02 || bytes[0] == 0x03) {
        // This is a compressed public key
        PublicKey::from_sec1_bytes(&bytes)
            .map_err(|e| de::Error::custom(format!("Invalid public key: {}", e)))
    } else {
        Err(de::Error::custom(format!(
            "Invalid public key length: {}",
            bytes.len()
        )))
    }
}

fn serialize_public_key<S>(key: &PublicKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bytes = key.to_encoded_point(false);
    let hex_string = hex::encode(bytes.as_bytes());
    serializer.serialize_str(&hex_string)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedAmountProofs {
    pub sender: EncryptedExactAmount,
    pub recipient: EncryptedExactAmount,
    pub quorum: EncryptedExactAmount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Amount {
    Confidential(EncryptedAmountProofs),
    Public(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub from: Address,
    pub to: Address,
    pub amount: Amount,
    pub timestamp: i64,
    pub previous_transaction_id: TransactionHash,
}

impl Transaction {
    pub fn new(
        from: Address,
        to: Address,
        amount: Amount,
        previous_transaction_id: TransactionHash,
    ) -> Result<Self> {
        Ok(Self {
            from,
            to,
            amount,
            timestamp: Utc::now().timestamp_millis(),
            previous_transaction_id,
        })
    }

    pub fn new_confidential(
        from: Address,
        to: Address,
        sender: EncryptedExactAmount,
        recipient: EncryptedExactAmount,
        quorum: EncryptedExactAmount,
        previous_transaction_id: TransactionHash,
    ) -> Result<Self> {
        Ok(Self {
            from,
            to,
            amount: Amount::Confidential(EncryptedAmountProofs {
                sender,
                recipient,
                quorum,
            }),
            timestamp: Utc::now().timestamp_millis(),
            previous_transaction_id,
        })
    }

    pub fn calculate_id(&self) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();
        hasher.update(&self.from);
        hasher.update(&self.to);
        match &self.amount {
            Amount::Confidential(amount) => {
                hasher.update(
                    amount
                        .sender
                        .c1
                        .to_affine()
                        .to_encoded_point(true)
                        .as_bytes(),
                );
                hasher.update(
                    amount
                        .sender
                        .c2
                        .to_affine()
                        .to_encoded_point(true)
                        .as_bytes(),
                );
                hasher.update(amount.sender.range_proof.to_bytes());
                hasher.update(
                    amount
                        .recipient
                        .c1
                        .to_affine()
                        .to_encoded_point(true)
                        .as_bytes(),
                );
                hasher.update(
                    amount
                        .recipient
                        .c2
                        .to_affine()
                        .to_encoded_point(true)
                        .as_bytes(),
                );
                hasher.update(amount.recipient.range_proof.to_bytes());
                hasher.update(
                    amount
                        .quorum
                        .c1
                        .to_affine()
                        .to_encoded_point(true)
                        .as_bytes(),
                );
                hasher.update(
                    amount
                        .quorum
                        .c2
                        .to_affine()
                        .to_encoded_point(true)
                        .as_bytes(),
                );
                hasher.update(amount.quorum.range_proof.to_bytes());
            }
            Amount::Public(amount) => {
                hasher.update(amount.to_be_bytes());
            }
        }
        hasher.update(self.timestamp.to_be_bytes());
        hasher.update(&self.previous_transaction_id.0);

        let mut res = [0u8; 32];
        res.copy_from_slice(&hasher.finalize());

        Ok(res)
    }
}

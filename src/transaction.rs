use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use bulletproofs::RangeProof;
use chrono::Utc;
use k256::ecdh::diffie_hellman;
use k256::ecdsa::{Signature, VerifyingKey};
use k256::{elliptic_curve::sec1::ToEncodedPoint, ProjectivePoint, PublicKey, SecretKey};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::address::Address;
use crate::confidential::EncryptedExactAmount;
use crate::serialization::signature::{deserialize_signature, serialize_signature};

#[derive(Debug, Clone)]
pub struct StealthMetadata {
    pub ephemeral_public_key: PublicKey,
    pub view_tag: u8,
}

impl Serialize for StealthMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("StealthMetadata", 2)?;

        // Convert PublicKey to bytes and then to base64
        let key_bytes = self.ephemeral_public_key.to_sec1_bytes();
        let key_base64 = BASE64.encode(key_bytes);

        state.serialize_field("ephemeral_public_key", &key_base64)?;
        state.serialize_field("view_tag", &self.view_tag)?;
        state.end()
    }
}

// Implement custom deserialization
impl<'de> Deserialize<'de> for StealthMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            ephemeral_public_key: String,
            view_tag: u8,
        }

        let helper = Helper::deserialize(deserializer)?;

        // Convert base64 back to PublicKey
        let key_bytes = BASE64
            .decode(helper.ephemeral_public_key)
            .map_err(serde::de::Error::custom)?;

        let public_key =
            PublicKey::from_sec1_bytes(&key_bytes).map_err(serde::de::Error::custom)?;

        Ok(StealthMetadata {
            ephemeral_public_key: public_key,
            view_tag: helper.view_tag,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StealthTransaction {
    pub transaction: Transaction,
    pub stealth_metadata: Option<StealthMetadata>,
}

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
    pub stealth_metadata: Option<StealthMetadata>,
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
pub enum Amount {
    Confidential(EncryptedExactAmount),
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
        c1: ProjectivePoint,
        c2: ProjectivePoint,
        range_proof: RangeProof,
        previous_transaction_id: TransactionHash,
    ) -> Result<Self> {
        Ok(Self {
            from,
            to,
            amount: Amount::Confidential(EncryptedExactAmount {
                c1,
                c2,
                range_proof,
            }),
            timestamp: Utc::now().timestamp_millis(),
            previous_transaction_id,
        })
    }

    pub fn create_stealth(
        from: Address,
        receiver_pub: &PublicKey,
        amount: Amount,
        ephemeral_secret: &SecretKey,
        previous_transaction_id: TransactionHash,
    ) -> Result<(Self, StealthMetadata)> {
        // Generate stealth address
        let (stealth_address, _) = Address::generate_stealth(receiver_pub, ephemeral_secret)?;

        // Create view tag (first byte of shared secret)
        let shared_secret = diffie_hellman(
            ephemeral_secret.to_nonzero_scalar(),
            receiver_pub.as_affine(),
        );
        let view_tag = shared_secret.raw_secret_bytes()[0];

        // Create the transaction
        let transaction = Self {
            from,
            to: stealth_address,
            amount,
            timestamp: Utc::now().timestamp_millis(),
            previous_transaction_id,
        };

        // Create stealth metadata
        let stealth_metadata = StealthMetadata {
            ephemeral_public_key: ephemeral_secret.public_key(),
            view_tag,
        };

        Ok((transaction, stealth_metadata))
    }

    pub fn scan_stealth(
        &self,
        metadata: &StealthMetadata,
        view_private_key: &SecretKey,
        spend_public_key: &PublicKey,
    ) -> Result<bool> {
        // Compute shared secret
        let shared_secret = diffie_hellman(
            view_private_key.to_nonzero_scalar(),
            metadata.ephemeral_public_key.as_affine(),
        );

        // Check view tag
        if shared_secret.raw_secret_bytes()[0] != metadata.view_tag {
            return Ok(false);
        }

        // Compute expected stealth address
        let (expected_stealth_address, _) = Address::generate_stealth(
            spend_public_key,
            &SecretKey::from_bytes(shared_secret.raw_secret_bytes())?,
        )?;

        // Check if the transaction's destination matches the computed stealth address
        Ok(self.to == expected_stealth_address)
    }

    pub fn calculate_id(&self) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();
        hasher.update(&self.from);
        hasher.update(&self.to);
        match &self.amount {
            Amount::Confidential(amount) => {
                hasher.update(amount.c1.to_affine().to_encoded_point(true).as_bytes());
                hasher.update(amount.c2.to_affine().to_encoded_point(true).as_bytes());
                hasher.update(amount.range_proof.to_bytes());
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

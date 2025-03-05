use anyhow::anyhow;
use anyhow::Result;
use chrono::Utc;
use curve25519_dalek::ristretto::CompressedRistretto;
use curve25519_dalek::scalar::Scalar as DalekScalar;
use k256::ecdsa::Signature;
use k256::elliptic_curve::group::GroupEncoding;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::elliptic_curve::PrimeField;
use k256::PublicKey;
use k256::{ProjectivePoint, Scalar, SecretKey};
use lazy_static::lazy_static;
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use sha2::digest::generic_array::GenericArray;
use sha2::{Digest, Sha256};
use std::fmt;

use crate::address::Address;
use crate::confidential::EncryptedExactAmount;
use crate::confidential::ShieldedAmount;
use crate::confidential::commit_amount;
use crate::transaction_hash::TransactionHash;
use crate::utils::hash_to_curve;
use bulletproofs::RangeProof as BulletproofRangeProof;

lazy_static! {
    static ref QUORUM_KEY: PublicKey = PublicKey::from_sec1_bytes(&[0; 32]).expect("Invalid key");
}

#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub path: Vec<(ProjectivePoint, bool)>,
    pub root: ProjectivePoint,
    pub leaf_index: u64,
}

impl Serialize for MerkleProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("MerkleProof", 3)?;
        let path: Vec<(String, bool)> = self
            .path
            .iter()
            .map(|(point, is_right)| (hex::encode(point.to_bytes()), *is_right))
            .collect();
        state.serialize_field("path", &path)?;
        state.serialize_field("root", &hex::encode(self.root.to_bytes()))?;
        state.serialize_field("leaf_index", &self.leaf_index)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MerkleProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MerkleProofVisitor;

        impl<'de> Visitor<'de> for MerkleProofVisitor {
            type Value = MerkleProof;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct MerkleProof")
            }

            fn visit_map<V>(self, mut map: V) -> Result<MerkleProof, V::Error>
            where
                V: MapAccess<'de>,
            {
                let path_hex: Vec<(String, bool)> = map
                    .next_entry::<&str, Vec<(String, bool)>>()
                    .and_then(|opt| opt.ok_or_else(|| de::Error::invalid_length(0, &self)))?
                    .1;
                let path = path_hex
                    .into_iter()
                    .map(|(point_hex, is_right)| {
                        let point_bytes = hex::decode(&point_hex)
                            .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                        let point = Option::from(ProjectivePoint::from_bytes(
                            GenericArray::from_slice(&point_bytes),
                        ))
                        .ok_or_else(|| de::Error::custom("Invalid point in path"))?;
                        Ok((point, is_right))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let root_hex = map
                    .next_entry::<&str, String>()
                    .and_then(|opt| opt.ok_or_else(|| de::Error::invalid_length(1, &self)))?
                    .1;
                let root_bytes = hex::decode(&root_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let root = Option::from(ProjectivePoint::from_bytes(GenericArray::from_slice(
                    &root_bytes,
                )))
                .ok_or_else(|| de::Error::custom("Invalid root point"))?;

                let leaf_index = map
                    .next_entry::<&str, u64>()
                    .and_then(|opt| opt.ok_or_else(|| de::Error::invalid_length(2, &self)))?
                    .1;

                Ok(MerkleProof {
                    path,
                    root,
                    leaf_index,
                })
            }
        }

        const FIELDS: &[&str] = &["path", "root", "leaf_index"];
        deserializer.deserialize_struct("MerkleProof", FIELDS, MerkleProofVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct SpendingProof {
    pub nullifier_commitment: ProjectivePoint,
    pub value_commitment: ProjectivePoint,
    pub proof: BulletproofRangeProof,
}

impl Serialize for SpendingProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("SpendingProof", 3)?;

        state.serialize_field(
            "nullifier_commitment",
            &hex::encode(self.nullifier_commitment.to_bytes()),
        )?;
        state.serialize_field(
            "value_commitment",
            &hex::encode(self.value_commitment.to_bytes()),
        )?;
        state.serialize_field("proof", &self.proof)?;

        state.end()
    }
}

impl<'de> Deserialize<'de> for SpendingProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SpendingProofVisitor;

        impl<'de> Visitor<'de> for SpendingProofVisitor {
            type Value = SpendingProof;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct SpendingProof")
            }

            fn visit_map<V>(self, mut map: V) -> Result<SpendingProof, V::Error>
            where
                V: MapAccess<'de>,
            {
                let nullifier_commitment_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?
                    .1;
                let nullifier_commitment_bytes = hex::decode(&nullifier_commitment_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let nullifier_commitment = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&nullifier_commitment_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid nullifier commitment point"))?;

                let value_commitment_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?
                    .1;
                let value_commitment_bytes = hex::decode(&value_commitment_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let value_commitment = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&value_commitment_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid value commitment point"))?;

                let proof = map
                    .next_entry::<&str, BulletproofRangeProof>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?
                    .1;

                Ok(SpendingProof {
                    nullifier_commitment,
                    value_commitment,
                    proof,
                })
            }
        }

        const FIELDS: &[&str] = &["nullifier_commitment", "value_commitment", "proof"];
        deserializer.deserialize_struct("SpendingProof", FIELDS, SpendingProofVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct BalanceProof {
    pub input_sum_commitment: ProjectivePoint,
    pub output_sum_commitment: ProjectivePoint,
    pub proof: BulletproofRangeProof,
}

impl Serialize for BalanceProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("BalanceProof", 3)?;
        state.serialize_field(
            "input_sum_commitment",
            &hex::encode(self.input_sum_commitment.to_bytes()),
        )?;
        state.serialize_field(
            "output_sum_commitment",
            &hex::encode(self.output_sum_commitment.to_bytes()),
        )?;
        state.serialize_field("proof", &self.proof)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for BalanceProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BalanceProofVisitor;

        impl<'de> Visitor<'de> for BalanceProofVisitor {
            type Value = BalanceProof;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct BalanceProof")
            }

            fn visit_map<V>(self, mut map: V) -> Result<BalanceProof, V::Error>
            where
                V: MapAccess<'de>,
            {
                let input_sum_commitment_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?
                    .1;
                let input_sum_commitment_bytes = hex::decode(&input_sum_commitment_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let input_sum_commitment = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&input_sum_commitment_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid input sum commitment point"))?;

                let output_sum_commitment_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?
                    .1;
                let output_sum_commitment_bytes = hex::decode(&output_sum_commitment_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let output_sum_commitment = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&output_sum_commitment_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid output sum commitment point"))?;

                let proof = map
                    .next_entry::<&str, BulletproofRangeProof>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?
                    .1;

                Ok(BalanceProof {
                    input_sum_commitment,
                    output_sum_commitment,
                    proof,
                })
            }
        }

        const FIELDS: &[&str] = &["input_sum_commitment", "output_sum_commitment", "proof"];
        deserializer.deserialize_struct("BalanceProof", FIELDS, BalanceProofVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct ShieldedInput {
    pub merkle_proof: MerkleProof,
    pub spending_proof: SpendingProof,
}

impl Serialize for ShieldedInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("ShieldedInput", 3)?;
        state.serialize_field("merkle_proof", &self.merkle_proof)?;
        state.serialize_field("spending_proof", &self.spending_proof)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ShieldedInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ShieldedInputVisitor;

        impl<'de> Visitor<'de> for ShieldedInputVisitor {
            type Value = ShieldedInput;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct ShieldedInput")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ShieldedInput, V::Error>
            where
                V: MapAccess<'de>,
            {
                let merkle_proof = map
                    .next_entry::<&str, MerkleProof>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?
                    .1;

                let spending_proof = map
                    .next_entry::<&str, SpendingProof>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?
                    .1;

                Ok(ShieldedInput {
                    merkle_proof,
                    spending_proof,
                })
            }
        }

        const FIELDS: &[&str] = &["nullifier", "merkle_proof", "spending_proof"];
        deserializer.deserialize_struct("ShieldedInput", FIELDS, ShieldedInputVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct ShieldedOutput {
    pub value_commitment: ProjectivePoint,
    pub range_proof: BulletproofRangeProof,
    pub spending_key_commitment: ProjectivePoint,
    pub nullifier: ProjectivePoint,
    pub recipient_public_key: PublicKey,
    pub encrypted_amount: EncryptedExactAmount,
}

impl Serialize for ShieldedOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("ShieldedOutput", 6)?;
        state.serialize_field(
            "value_commitment",
            &hex::encode(self.value_commitment.to_bytes()),
        )?;
        state.serialize_field("range_proof", &self.range_proof)?;
        state.serialize_field(
            "spending_key_commitment", 
            &hex::encode(self.spending_key_commitment.to_bytes()),
        )?;
        state.serialize_field("nullifier", &hex::encode(self.nullifier.to_bytes()))?;
        state.serialize_field("recipient_public_key", &self.recipient_public_key)?;
        state.serialize_field("encrypted_amount", &self.encrypted_amount)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ShieldedOutput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ShieldedOutputVisitor;

        impl<'de> Visitor<'de> for ShieldedOutputVisitor {
            type Value = ShieldedOutput;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct ShieldedOutput")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ShieldedOutput, V::Error>
            where
                V: MapAccess<'de>,
            {
                let value_commitment_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?
                    .1;
                let value_commitment_bytes = hex::decode(&value_commitment_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let value_commitment = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&value_commitment_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid value commitment point"))?;

                let range_proof = map
                    .next_entry::<&str, BulletproofRangeProof>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?
                    .1;

                let spending_key_commitment_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?
                    .1;
                let spending_key_commitment_bytes = hex::decode(&spending_key_commitment_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let spending_key_commitment = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&spending_key_commitment_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid spending key commitment point"))?;

                let nullifier_hex = map
                    .next_entry::<&str, String>()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?
                    .1;
                let nullifier_bytes = hex::decode(&nullifier_hex)
                    .map_err(|e| de::Error::custom(format!("Invalid hex: {}", e)))?;
                let nullifier = Option::from(ProjectivePoint::from_bytes(
                    GenericArray::from_slice(&nullifier_bytes),
                ))
                .ok_or_else(|| de::Error::custom("Invalid nullifier point"))?;

                let recipient_public_key = map
                    .next_entry::<&str, PublicKey>()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?
                    .1;

                let encrypted_amount = map
                    .next_entry::<&str, EncryptedExactAmount>()?
                    .ok_or_else(|| de::Error::invalid_length(5, &self))?
                    .1;

                Ok(ShieldedOutput {
                    value_commitment,
                    range_proof,
                    spending_key_commitment,
                    nullifier,
                    recipient_public_key,
                    encrypted_amount,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "value_commitment",
            "range_proof", 
            "spending_key_commitment",
            "nullifier",
            "recipient_public_key",
            "encrypted_amount"
        ];
        deserializer.deserialize_struct("ShieldedOutput", FIELDS, ShieldedOutputVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicInput {
    pub amount: u64,
    pub owner: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicOutput {
    pub amount: u64,
    pub recipient: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Input {
    Public(PublicInput),
    Confidential(ShieldedInput),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedPublicInput {
    pub input: PublicInput,
    pub signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedConfidentialInput {
    pub input: ShieldedInput,
    pub signature: Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Output {
    Public(PublicOutput),
    Confidential(ShieldedOutput),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub timestamp: i64,
    pub previous_transaction_id: TransactionHash,
}

impl Transaction {
    pub fn new(
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        previous_transaction_id: TransactionHash,
    ) -> Result<Self> {
        Ok(Self {
            inputs,
            outputs,
            timestamp: Utc::now().timestamp_millis(),
            previous_transaction_id,
        })
    }

    pub fn verify_amounts_consistency(&self) -> Result<bool> {
        let mut public_input_amount = 0u64;
        let mut public_output_amount = 0u64;
        let mut confidential_input_amount = ProjectivePoint::IDENTITY;
        let mut confidential_output_amount = ProjectivePoint::IDENTITY;

        // Collect all input amounts
        for input in self.inputs.iter() {
            match input {
                Input::Public(public_input) => {
                    public_input_amount = public_input_amount.checked_add(public_input.amount)
                        .ok_or_else(|| anyhow!("Input amount overflow"))?;
                }
                Input::Confidential(shielded_input) => {
                    confidential_input_amount += shielded_input.spending_proof.value_commitment;
                }
            }
        }
        
        // Collect all output amounts
        for output in self.outputs.iter() {
            match output {
                Output::Public(public_output) => {
                    public_output_amount = public_output_amount.checked_add(public_output.amount)
                        .ok_or_else(|| anyhow!("Output amount overflow"))?;
                }
                Output::Confidential(shielded_output) => {
                    confidential_output_amount += shielded_output.value_commitment;
                }
            }
        }

        // For public amounts, we need to convert them to commitments
        // We use the same blinding factor for both input and output to ensure they match
        let public_input_commitment = commit_amount(public_input_amount, Scalar::ZERO)?;
        let public_output_commitment = commit_amount(public_output_amount, Scalar::ZERO)?;

        // Add public and confidential amounts together and verify they match
        let total_input = confidential_input_amount + public_input_commitment;
        let total_output = confidential_output_amount + public_output_commitment;

        if total_input != total_output {
            return Err(anyhow!("Total input amount does not equal total output amount"));
        }

        Ok(true)
    }

    pub fn calculate_id(&self) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();

        // Hash inputs
        for input in self.inputs.iter() {
            match input {
                Input::Public(public_input) => {
                    hasher.update(public_input.amount.to_be_bytes());
                    hasher.update(&public_input.owner.0);
                }
                Input::Confidential(shielded_input) => {
                    hash_merkle_proof(&mut hasher, &shielded_input.merkle_proof);
                    hash_spending_proof(&mut hasher, &shielded_input.spending_proof);
                }
            }
        }

        // Hash outputs
        for output in self.outputs.iter() {
            match output {
                Output::Public(public_output) => {
                    hasher.update(public_output.amount.to_be_bytes());
                    hasher.update(&public_output.recipient.0);
                }
                Output::Confidential(shielded_output) => {
                    hasher.update(shielded_output.nullifier.to_bytes());
                    hasher.update(shielded_output.range_proof.to_bytes());
                    hasher.update(shielded_output.spending_key_commitment.to_bytes());
                    hasher.update(shielded_output.recipient_public_key.to_sec1_bytes());
                    hasher.update(shielded_output.encrypted_amount.range_proof.to_bytes());
                    hasher.update(shielded_output.encrypted_amount.c1.to_bytes());
                    hasher.update(shielded_output.encrypted_amount.c2.to_bytes());
                }
            }
        }

        hasher.update(self.timestamp.to_be_bytes());
        hasher.update(&self.previous_transaction_id.0);

        let mut res = [0u8; 32];
        res.copy_from_slice(&hasher.finalize());
        Ok(res)
    }
}

// Helper functions for hashing different components
fn hash_merkle_proof(hasher: &mut Sha256, proof: &MerkleProof) -> () {
    for (point, is_right) in &proof.path {
        hasher.update(point.to_bytes());
        hasher.update(&[*is_right as u8]);
    }
    hasher.update(proof.root.to_bytes());
    hasher.update(proof.leaf_index.to_be_bytes());
}

fn hash_spending_proof(hasher: &mut Sha256, proof: &SpendingProof) {
    hasher.update(proof.nullifier_commitment.to_bytes());
    hasher.update(proof.value_commitment.to_bytes());
    hasher.update(proof.proof.to_bytes());
}

// Implementation for components
impl ShieldedInput {
    pub fn new(note: ShieldedAmount, spending_key: &SecretKey) -> Result<Self> {
        let merkle_proof = MerkleProof {
            path: vec![], // TODO: This should be populated with the actual Merkle path
            root: ProjectivePoint::IDENTITY, // TODO: This should be the actual Merkle root
            leaf_index: 0, // TODO: This should be the actual leaf index
        };

        let spending_proof = SpendingProof {
            nullifier_commitment: note.nullifier,
            value_commitment: note.value_commitment,
            proof: note.range_proof,
        };

        Ok(Self {
            merkle_proof,
            spending_proof,
        })
    }
}

impl ShieldedOutput {
    pub fn new(
        amount: u64,
        secret_key: &SecretKey,
        recipient_key: &PublicKey,
        blinding: Scalar,
    ) -> Result<Self> {
        // Create value commitment using the provided blinding factor
        let value_commitment = commit_amount(amount, blinding)?;

        // Create range proof using the same blinding factor
        let mut transcript = merlin::Transcript::new(b"example");
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(blinding.to_repr().as_ref());
        let dalek_blinding = curve25519_dalek::scalar::Scalar::from_bytes_mod_order(bytes);
        let range_proof = BulletproofRangeProof::prove_single(
            &bulletproofs::BulletproofGens::new(64, 1),
            &bulletproofs::PedersenGens::default(),
            &mut transcript,
            amount,
            &dalek_blinding,
            64,
        )?
        .0;

        // Create spending key commitment using the same blinding factor
        let spending_key_commitment = commit_amount(amount, blinding + *secret_key.to_nonzero_scalar())?;
        
        // Create nullifier
        let nullifier = create_nullifier(secret_key, &value_commitment)?;

        // Create encrypted amount for recipient
        let encrypted_amount = EncryptedExactAmount::encrypt(amount, recipient_key)?;

        Ok(Self {
            value_commitment,
            range_proof,
            spending_key_commitment,
            nullifier,
            recipient_public_key: recipient_key.clone(),
            encrypted_amount,
        })
    }

    pub fn verify(&self) -> Result<bool> {
        Ok(true)
    }
}

impl SpendingProof {
    pub fn verify(&self) -> Result<bool> {
        // Verify the nullifier commitment
        let nullifier_commitment_bytes = self.nullifier_commitment.to_affine().to_bytes();
        let nullifier_commitment_scalar: Scalar = Option::from(Scalar::from_repr(
            *GenericArray::from_slice(&nullifier_commitment_bytes),
        ))
        .ok_or_else(|| anyhow!("Invalid nullifier"))?;
        let expected_nullifier_commitment =
            ProjectivePoint::GENERATOR * nullifier_commitment_scalar;
        if self.value_commitment != expected_nullifier_commitment {
            return Err(anyhow!("Invalid nullifier commitment"));
        }

        // Verify the range proof
        let mut transcript = merlin::Transcript::new(b"example");
        let pc_gens = bulletproofs::PedersenGens::default();
        let bp_gens = bulletproofs::BulletproofGens::new(64, 1);
        self.proof.verify_single(
            &bp_gens,
            &pc_gens,
            &mut transcript,
            &CompressedRistretto::from_slice(&self.value_commitment.to_bytes())?,
            64,
        )?;

        Ok(true)
    }
}

impl MerkleProof {
    pub fn verify(&self) -> Result<bool> {
        let computed_root = self
            .leaf_index
            .to_le_bytes()
            .iter()
            .zip(&self.path)
            .try_fold(
                self.path[0].0,
                |acc, (_, (sibling, is_right))| -> Result<ProjectivePoint> {
                    if *is_right {
                        let res: ProjectivePoint =
                            Option::from(ProjectivePoint::from_bytes(&GenericArray::from_slice(
                                &[acc.to_bytes(), sibling.to_bytes()].concat(),
                            )))
                            .ok_or_else(|| anyhow!("Invalid point conversion"))?;
                        Ok(res)
                    } else {
                        let res: ProjectivePoint =
                            Option::from(ProjectivePoint::from_bytes(&GenericArray::from_slice(
                                &[sibling.to_bytes(), acc.to_bytes()].concat(),
                            )))
                            .ok_or_else(|| anyhow!("Invalid point conversion"))?;
                        Ok(res)
                    }
                },
            )?;

        if computed_root == self.root {
            Ok(true)
        } else {
            Err(anyhow!("Merkle proof verification failed"))
        }
    }
}

impl BalanceProof {
    pub fn verify(&self) -> Result<bool> {
        // Verify the range proof
        let mut transcript = merlin::Transcript::new(b"balance_proof");
        let pc_gens = bulletproofs::PedersenGens::default();
        let bp_gens = bulletproofs::BulletproofGens::new(64, 1);
        self.proof.verify_single(
            &bp_gens,
            &pc_gens,
            &mut transcript,
            &CompressedRistretto::from_slice(&self.input_sum_commitment.to_bytes())?,
            32,
        )?;

        // Verify that input sum commitment equals output sum commitment
        if self.input_sum_commitment != self.output_sum_commitment {
            return Err(anyhow!(
                "Input sum commitment does not equal output sum commitment"
            ));
        }

        Ok(true)
    }
}

fn create_nullifier(
    spending_key: &SecretKey,
    commitment: &ProjectivePoint,
) -> Result<ProjectivePoint> {
    let mut hasher = Sha256::new();
    hasher.update(spending_key.to_bytes());
    hasher.update(commitment.to_bytes());
    let hash = hasher.finalize();
    let nullifier = hash_to_curve(GenericArray::clone_from_slice(&hash))?;
    Ok(nullifier)
}

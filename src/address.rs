use anyhow::Result;
use ark_ff::PrimeField;
use k256::elliptic_curve::ecdh::diffie_hellman;
use k256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ZERO_ADDRESS: Address = Address([0; 32]);
const THRESHOLD_FLAG: u8 = 0x80;

pub struct StealthAddress {
    pub ephemeral_public: PublicKey,
    pub stealth_public: PublicKey,
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone, Copy)]
pub struct Address(pub [u8; 32]);

impl Address {
    pub fn generate_stealth(
        receiver_pub: &PublicKey,
        ephemeral_secret_key: &SecretKey,
    ) -> Result<(Self, PublicKey)> {
        let shared_secret = diffie_hellman(
            ephemeral_secret_key.to_nonzero_scalar(),
            receiver_pub.as_affine(),
        );

        // Convert shared secret to new public key point
        let point = PublicKey::from_sec1_bytes(shared_secret.raw_secret_bytes().as_slice())?;
        let stealth_pub = receiver_pub.to_projective() + point.to_projective();
        let stealth_pub = PublicKey::from_affine(stealth_pub.to_affine())?;

        let mut addr = [0u8; 32];
        addr.copy_from_slice(&stealth_pub.to_encoded_point(true).as_bytes()[..32]);
        Ok((Self(addr), stealth_pub))
    }

    pub fn is_threshold_group(&self) -> bool {
        self.0[0] & THRESHOLD_FLAG != 0
    }

    pub fn to_zk_field<F: PrimeField>(&self) -> Result<F> {
        Ok(F::from_be_bytes_mod_order(&self.0))
    }

    pub fn from_commitment(commitment: &[u8; 32]) -> Self {
        Self(*commitment)
    }

    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
    }

    pub fn as_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(hex_address: &str) -> Result<Address> {
        let decoded = hex::decode(hex_address)?;
        let mut address = [0u8; 32];
        address.copy_from_slice(&decoded);
        Ok(Address::new(address))
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for Address {
    fn from(data: [u8; 32]) -> Self {
        Self(data)
    }
}

use anyhow::{anyhow, Result};
use k256::elliptic_curve::PrimeField;
use k256::{ProjectivePoint, Scalar};
use sha2::{Digest, Sha256};
use sha2::digest::generic_array::{GenericArray, typenum::U32};

pub fn hash_to_curve(to_bytes: GenericArray<u8, U32>) -> Result<ProjectivePoint> {
    // Hash input bytes to scalar using SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&to_bytes);
    let hash = hasher.finalize(); // This produces a 32-byte output

    // Convert hash to scalar and multiply generator point
    let scalar: Scalar = Option::from(Scalar::from_repr(*GenericArray::from_slice(&hash)))
        .ok_or_else(|| anyhow!("Invalid scalar conversion"))?;
    
    Ok(ProjectivePoint::GENERATOR * &scalar)
}
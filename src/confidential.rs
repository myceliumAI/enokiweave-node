use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use bulletproofs::{BulletproofGens, PedersenGens, RangeProof};
use k256::elliptic_curve::rand_core::OsRng;
use k256::elliptic_curve::sec1::FromEncodedPoint;
use k256::{
    elliptic_curve::{sec1::ToEncodedPoint, Field},
    ProjectivePoint, PublicKey, SecretKey,
};
use merlin::Transcript;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct EncryptedExactAmount {
    // ElGamal encryption of exact value
    pub c1: ProjectivePoint, // r * G
    pub c2: ProjectivePoint, // m * G + r * pub_key
    // Range proof to prove value is positive
    pub range_proof: RangeProof,
}

impl Serialize for EncryptedExactAmount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("EncryptedExactAmount", 3)?;

        // Convert ProjectivePoints to base64-encoded bytes
        let c1_bytes = self.c1.to_affine().to_encoded_point(false);
        let c2_bytes = self.c2.to_affine().to_encoded_point(false);

        state.serialize_field("c1", &BASE64.encode(c1_bytes))?;
        state.serialize_field("c2", &BASE64.encode(c2_bytes))?;
        state.serialize_field("range_proof", &self.range_proof)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for EncryptedExactAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            c1: String,
            c2: String,
            range_proof: String, // Changed from RangeProof to String
        }

        let helper = Helper::deserialize(deserializer)?;

        // Convert base64 encoded points back to ProjectivePoint
        let c1_bytes = BASE64.decode(helper.c1).map_err(serde::de::Error::custom)?;
        let c2_bytes = BASE64.decode(helper.c2).map_err(serde::de::Error::custom)?;

        let c1_point =
            k256::EncodedPoint::from_bytes(&c1_bytes).map_err(serde::de::Error::custom)?;
        let c2_point =
            k256::EncodedPoint::from_bytes(&c2_bytes).map_err(serde::de::Error::custom)?;

        let c1 = Option::from(ProjectivePoint::from_encoded_point(&c1_point))
            .ok_or_else(|| serde::de::Error::custom("Invalid c1 point"))?;

        let c2 = Option::from(ProjectivePoint::from_encoded_point(&c2_point))
            .ok_or_else(|| serde::de::Error::custom("Invalid c2 point"))?;

        // Decode base64 range proof
        let range_proof_bytes = BASE64
            .decode(helper.range_proof)
            .map_err(serde::de::Error::custom)?;

        // Convert bytes to RangeProof
        let range_proof =
            RangeProof::from_bytes(&range_proof_bytes).map_err(serde::de::Error::custom)?;

        Ok(EncryptedExactAmount {
            c1,
            c2,
            range_proof,
        })
    }
}

impl EncryptedExactAmount {
    pub fn encrypt(amount: u64, public_key: &PublicKey) -> Result<Self> {
        // Generate random scalar for blinding
        let r = k256::Scalar::random(&mut OsRng);

        // Convert amount to scalar
        let m = k256::Scalar::from(amount);

        // Base point G
        let g = ProjectivePoint::GENERATOR;

        // Encrypt: (r*G, m*G + r*P)
        let c1 = g * r;
        let c2 = (g * m) + (public_key.to_projective() * r);

        // Create range proof
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(64, 1);
        let mut prover_transcript = Transcript::new(b"amount_range_proof");

        // Convert k256 scalar to curve25519 scalar for bulletproofs
        let blinding = curve25519_dalek::scalar::Scalar::random(&mut OsRng);
        let (range_proof, _) = RangeProof::prove_single(
            &bp_gens,
            &pc_gens,
            &mut prover_transcript,
            amount,
            &blinding,
            64,
        )?;

        Ok(Self {
            c1,
            c2,
            range_proof,
        })
    }

    pub fn decrypt(&self, private_key: &SecretKey) -> Result<u64> {
        // Convert private key to scalar
        let scalar = *private_key.to_nonzero_scalar();

        // Decrypt: c2 - priv_key * c1 = m*G
        let m_point = self.c2 - (self.c1 * scalar);

        let m = find_exact_discrete_log(m_point)?;
        Ok(m)
    }
    pub fn verify_greater_than_u64(&self, value: u64) -> Result<bool> {
        // Convert u64 to encrypted point using same base point
        let g = ProjectivePoint::GENERATOR;
        let m = k256::Scalar::from(value);
        let value_point = g * m;

        // Subtract from our encrypted value
        let diff_c2 = self.c2 - value_point;

        // Convert k256 ProjectivePoint to bytes for range proof
        let point_bytes = diff_c2.to_affine().to_encoded_point(false);
        let compressed =
            curve25519_dalek::ristretto::CompressedRistretto::from_slice(point_bytes.as_bytes())?;

        // Verify range proof
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(64, 1);

        let mut transcript = Transcript::new(b"amount_range_proof");
        self.range_proof
            .verify_single(&bp_gens, &pc_gens, &mut transcript, &compressed, 64)?;

        // Compare points using their canonical byte representation
        let encoded_diff = diff_c2.to_affine().to_encoded_point(false);
        let encoded_identity = ProjectivePoint::IDENTITY
            .to_affine()
            .to_encoded_point(false);

        let a = encoded_diff.as_bytes();
        let b = encoded_identity.as_bytes();

        Ok(a > b) // Check if difference is positive
    }

    pub fn verify_greater_than(&self, other: &Self) -> Result<bool> {
        // Subtract encrypted points
        let _diff_c1 = self.c1 - other.c1;
        let diff_c2 = self.c2 - other.c2;

        // Convert k256 ProjectivePoint to bytes for range proof
        let point_bytes = diff_c2.to_affine().to_encoded_point(false);
        let compressed =
            curve25519_dalek::ristretto::CompressedRistretto::from_slice(point_bytes.as_bytes())?;

        // Verify range proofs
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(64, 1);

        // Verify both range proofs
        let mut transcript1 = Transcript::new(b"amount_range_proof");
        self.range_proof
            .verify_single(&bp_gens, &pc_gens, &mut transcript1, &compressed, 64)?;

        let mut transcript2 = Transcript::new(b"amount_range_proof");
        other
            .range_proof
            .verify_single(&bp_gens, &pc_gens, &mut transcript2, &compressed, 64)?;

        // Compare points using their canonical byte representation
        let encoded_diff = diff_c2.to_affine().to_encoded_point(false);
        let encoded_identity = ProjectivePoint::IDENTITY
            .to_affine()
            .to_encoded_point(false);

        let a = encoded_diff.as_bytes();
        let b = encoded_identity.as_bytes();

        Ok(a > b) // Check if difference is positive
    }
}

// Helper function to find exact discrete log for small values
fn find_exact_discrete_log(point: ProjectivePoint) -> Result<u64> {
    let g = ProjectivePoint::GENERATOR;

    let mut low = 0u64;
    let mut high = u64::MAX;

    while low <= high {
        let mid = (low + high) / 2;
        let scalar = k256::Scalar::from(mid);
        let test_point = g * scalar;

        // Compare points using their canonical byte representation
        let test_affine = test_point.to_affine().to_encoded_point(false);
        let point_affine = point.to_affine().to_encoded_point(false);

        match test_affine.as_bytes().cmp(point_affine.as_bytes()) {
            std::cmp::Ordering::Equal => return Ok(mid),
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid - 1,
        }
    }

    Err(anyhow!("Could not find exact value"))
}

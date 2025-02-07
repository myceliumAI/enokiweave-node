use anyhow::Result;
use k256::ecdsa::Signature;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct SignatureComponents {
    R: String,
    s: String,
}

pub fn deserialize_signature<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
    D: Deserializer<'de>,
{
    // First try to deserialize as SignatureComponents
    let result = SignatureComponents::deserialize(deserializer);

    match result {
        Ok(components) => {
            // Combine R and s into a single 64-byte array
            let r_bytes = hex::decode(components.R.trim_start_matches("0x"))
                .map_err(|e| de::Error::custom(format!("Invalid R component hex: {}", e)))?;
            let s_bytes = hex::decode(components.s.trim_start_matches("0x"))
                .map_err(|e| de::Error::custom(format!("Invalid s component hex: {}", e)))?;

            let mut signature_bytes = Vec::with_capacity(64);
            signature_bytes.extend_from_slice(&r_bytes);
            signature_bytes.extend_from_slice(&s_bytes);

            Signature::try_from(signature_bytes.as_slice())
                .map_err(|e| de::Error::custom(format!("Invalid signature: {}", e)))
        }
        Err(e) => Err(de::Error::custom(format!(
            "Failed to deserialize signature: {}",
            e
        ))),
    }
}

pub fn serialize_signature<S>(signature: &Signature, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let sig_bytes = signature.to_bytes();
    let components = SignatureComponents {
        R: hex::encode(&sig_bytes[..32]),
        s: hex::encode(&sig_bytes[32..]),
    };
    components.serialize(serializer)
}

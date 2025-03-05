use serde::{Deserialize, Serialize};

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

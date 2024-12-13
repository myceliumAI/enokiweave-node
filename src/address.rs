use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone, Copy)]
pub struct Address(pub [u8; 32]);

impl Address {
    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
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

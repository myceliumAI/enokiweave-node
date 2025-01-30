use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const ZERO_ADDRESS: Address = Address([0; 32]);

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone, Copy)]
pub struct Address(pub [u8; 32]);

impl Address {
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

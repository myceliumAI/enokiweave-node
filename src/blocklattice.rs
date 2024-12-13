use anyhow::{anyhow, Result};
use chrono::Utc;
use ed25519_dalek::Signature;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

const ALICE_ADDRESS: Address = Address([1; 32]);
const BOB_ADDRESS: Address = Address([2; 32]);

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone, Copy)]
pub struct Address(pub [u8; 32]);

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct H256(pub [u8; 32]);

impl std::fmt::Display for H256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub H256);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionRequest {
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub public_key: [u8; 32],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub timestamp: i64,
    pub signature: Option<Signature>,
}

impl Transaction {
    pub fn new(from: Address, to: Address, amount: u64) -> Result<Self> {
        let timestamp = Utc::now().timestamp_millis();

        let id = Self::calculate_id(from, to, amount, timestamp)?;

        Ok(Self {
            id: TransactionId(H256(id)),
            from,
            to,
            amount,
            timestamp: Utc::now().timestamp_millis(),
            signature: None,
        })
    }

    pub fn calculate_id(
        from: Address,
        to: Address,
        amount: u64,
        timestamp: i64,
    ) -> Result<[u8; 32]> {
        let mut hasher = Sha256::new();
        hasher.update(amount.to_be_bytes());
        hasher.update(&from);
        hasher.update(&to);
        hasher.update(timestamp.to_be_bytes());

        let hash = &hasher.finalize()[..];

        let id: [u8; 32] = hash.try_into().expect("Wrong length");

        Ok(id)
    }
}

pub struct BlockLattice {
    transactions: HashMap<TransactionId, Transaction>,
    all_transaction_ids: HashSet<TransactionId>,
    buffer_incoming_transactions: VecDeque<Transaction>,
}

impl BlockLattice {
    pub fn new() -> Self {
        Self {
            transactions: HashMap::new(),
            all_transaction_ids: HashSet::new(),
            buffer_incoming_transactions: VecDeque::new(),
        }
    }

    pub fn add_transaction(
        &mut self,
        from: Address,
        to: Address,
        amount: u64,
        public_key: VerifyingKey,
    ) -> Result<String> {
        let transaction = Transaction::new(from, to, amount)?;

        if !Self::is_transaction_valid(transaction, public_key)? {
            return Err(anyhow!("Transaction is invalid"));
        }

        Ok(transaction.id.clone().0.to_string())
    }

    pub fn is_transaction_valid(
        transaction: Transaction,
        public_key: VerifyingKey,
    ) -> Result<bool> {
        let incoming_tx_id = Transaction::calculate_id(
            transaction.from,
            transaction.to,
            transaction.amount,
            transaction.timestamp,
        )?;
        if TransactionId(H256(incoming_tx_id)) != transaction.id {
            return Err(anyhow!("Transaction ID invalid"));
        }

        let signature = transaction
            .signature
            .ok_or(anyhow!("Signature is missing"))?;

        public_key
            .verify_strict(&transaction.id.0 .0, &signature)
            .map_err(|e| anyhow!("Signature verification failed: {}", e))?;

        Ok(true)
    }

    pub fn get_transaction(&self, id: [u8; 32]) -> Option<&Transaction> {
        self.transactions.get(&TransactionId(H256(id)))
    }

    pub fn get_all_transaction_ids(&self) -> &HashSet<TransactionId> {
        &self.all_transaction_ids
    }
}

fn hex_to_owned_slice(hex_string: &str) -> [u8; 32] {
    let bytes = hex::decode(hex_string).expect("Decoding failed");
    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);

    array
}

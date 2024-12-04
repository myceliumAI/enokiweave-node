use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

const ALICE_ADDRESS: Address = Address([1; 32]);
const BOB_ADDRESS: Address = Address([2; 32]);

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone, Copy)]
pub struct Address(pub [u8; 32]);

impl Address {
    fn null_address() -> Address {
        Address([0; 32])
    }
}

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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub timestamp: i64,
}

impl Transaction {
    fn new(from: Address, to: Address, amount: u64) -> Self {
        let timestamp = Utc::now().timestamp_millis();
        let mut hasher = Sha256::new();
        hasher.update(amount.to_be_bytes());
        hasher.update(&from);
        hasher.update(&to);
        hasher.update(timestamp.to_be_bytes());

        let hash = &hasher.finalize()[..];

        let id: [u8; 32] = hash.try_into().expect("Wrong length");
        Self {
            id: TransactionId(H256(id)),
            from,
            to,
            amount,
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    // Add this constructor
    pub fn from_request(req: TransactionRequest) -> Self {
        Self::new(req.from, req.to, req.amount)
    }
}

pub struct BlockLattice {
    transactions: HashMap<TransactionId, Transaction>,
    all_transaction_ids: HashSet<TransactionId>,

    latest_transaction_ids: VecDeque<TransactionId>,
}

impl BlockLattice {
    pub fn new() -> Self {
        let first_transaction = Transaction::new(Address::null_address(), ALICE_ADDRESS, 0);
        let second_transaction = Transaction::new(Address::null_address(), BOB_ADDRESS, 0);

        Self {
            transactions: HashMap::from([
                (first_transaction.id, first_transaction),
                (second_transaction.id, second_transaction),
            ]),
            all_transaction_ids: HashSet::new(),
            latest_transaction_ids: VecDeque::new(),
        }
    }

    pub fn add_transaction(&mut self, from: Address, to: Address, amount: u64) -> Result<String> {
        let transaction = Transaction::new(from, to, amount);

        // Verify that the transaction is valid

        self.all_transaction_ids.insert(transaction.id.clone());
        self.transactions.insert(transaction.id, transaction);

        Ok(transaction.id.clone().0.to_string())
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

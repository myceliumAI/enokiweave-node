use anyhow::{anyhow, Result};
use ed25519_dalek::Signature;
use ed25519_dalek::VerifyingKey;
use lmdb::Cursor;
use lmdb::Transaction as LmdbTransaction;
use std::path::Path;

use crate::address::Address;
use crate::transaction::RawTransaction;
use crate::transaction::{Transaction, TransactionId};
pub struct BlockLattice {
    confirmed_transactions_uri: String,
    pending_transactions_uri: String,
}

impl BlockLattice {
    pub fn new(confirmed_transactions_uri: String, pending_transactions_uri: String) -> Self {
        Self {
            confirmed_transactions_uri,
            pending_transactions_uri,
        }
    }

    pub fn add_transaction(
        &mut self,
        from: Address,
        to: Address,
        amount: u64,
        public_key: VerifyingKey,
        timestamp: i64,
        id: TransactionId,
        signature: Signature,
    ) -> Result<String> {
        let transaction = Transaction {
            from,
            to,
            amount,
            timestamp,
            id,
            signature,
        };

        if !Self::is_transaction_valid(transaction, public_key)? {
            return Err(anyhow!("Transaction is invalid"));
        }

        Ok(hex::encode(transaction.id.clone().0))
    }

    pub fn is_transaction_valid(
        transaction: Transaction,
        public_key: VerifyingKey,
    ) -> Result<bool> {
        let incoming_tx_id = RawTransaction::calculate_id(
            transaction.from,
            transaction.to,
            transaction.amount,
            transaction.timestamp,
        )?;

        println!("{:?}", incoming_tx_id);

        if TransactionId(incoming_tx_id) != transaction.id {
            return Err(anyhow!("Transaction ID invalid"));
        }

        public_key
            .verify_strict(&transaction.id.0, &transaction.signature)
            .map_err(|e| anyhow!("Signature verification failed: {}", e))?;

        Ok(true)
    }

    pub fn get_transaction(&self, id: [u8; 32]) -> Result<Transaction> {
        let env = lmdb::Environment::new()
            .open(&Path::new(&self.confirmed_transactions_uri))
            .map_err(|e| anyhow!("Failed to open LMDB environment: {}", e))?;

        let db = env
            .open_db(None)
            .map_err(|e| anyhow!("Failed to open database: {}", e))?;

        let reader = env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let transaction_bytes = match reader.get(db, &id) {
            Ok(bytes) => bytes,
            Err(lmdb::Error::NotFound) => return Err(anyhow!("Transaction not found")),
            Err(e) => return Err(anyhow!("Database error: {}", e)),
        };

        let transaction: Transaction = bincode::deserialize(transaction_bytes)
            .map_err(|e| anyhow!("Failed to deserialize transaction: {}", e))?;

        Ok(transaction)
    }

    pub fn get_all_transaction_ids(&self) -> Result<Vec<TransactionId>> {
        let env = lmdb::Environment::new()
            .open(&Path::new(&self.confirmed_transactions_uri))
            .map_err(|e| anyhow!("Failed to open LMDB environment: {}", e))?;

        let db = env
            .open_db(None)
            .map_err(|e| anyhow!("Failed to open database: {}", e))?;

        let reader = env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let mut transaction_ids = Vec::new();

        // Create a cursor to iterate through all entries
        let mut cursor = reader
            .open_ro_cursor(db)
            .map_err(|e| anyhow!("Failed to create cursor: {}", e))?;

        // cursor.iter() returns Result<(&[u8], &[u8])>
        // First &[u8] is the key (transaction ID)
        // Second &[u8] is the value (serialized transaction)
        for (result, _) in cursor.iter() {
            let mut id = [0u8; 32];
            id.copy_from_slice(result);
            transaction_ids.push(TransactionId(id));
        }

        Ok(transaction_ids)
    }
}

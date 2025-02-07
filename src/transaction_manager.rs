use anyhow::{anyhow, Result};
use ed25519_dalek::Signature;
use ed25519_dalek::VerifyingKey;
use lmdb::Cursor;
use lmdb::Database;
use lmdb::Environment;
use lmdb::Transaction as LmdbTransaction;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

use crate::address::{Address, ZERO_ADDRESS};
use crate::transaction::{Transaction, TransactionHash};
use crate::GenesisArgs;
use crate::DB_NAME;

static LMDB_ENV: Lazy<Arc<Environment>> = Lazy::new(|| {
    std::fs::create_dir_all(DB_NAME).expect("Failed to create transaction_db directory");
    Arc::new(
        lmdb::Environment::new()
            .set_max_dbs(1)
            .set_map_size(10 * 1024 * 1024)
            .set_max_readers(126)
            .open(&Path::new(DB_NAME))
            .expect("Failed to create LMDB environment"),
    )
});

#[derive(Debug, Serialize, Deserialize, Clone)]
enum TransactionStatus {
    Pending,
    Confirmed,
    Invalid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TransactionRecord {
    transaction: Transaction,
    previous_transaction_hash: TransactionHash,
    status: TransactionStatus,
    signature: Signature,
}

pub struct TransactionManager {
    pub lmdb_transaction_env: Arc<Environment>,
    pub db: Database,
}

impl TransactionManager {
    pub fn new() -> Result<Self> {
        let env = LMDB_ENV.clone();
        let db = env
            .open(Path::new(DB_NAME))
            .expect("Failed to create LMDB environment");

        Ok(TransactionManager {
            lmdb_transaction_env: env,
            db,
        })
    }

    pub fn load_genesis_transactions(&self, genesis_args: GenesisArgs) -> Result<()> {
        // Begin a write transaction
        let mut txn = self
            .lmdb_transaction_env
            .begin_rw_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        // Insert each genesis transaction into the database
        for (i, (address, amount)) in genesis_args.balances.into_iter().enumerate() {
            let mut transaction_id = [0u8; 32];
            let bytes = i.to_be_bytes();
            transaction_id[24..32].copy_from_slice(&bytes);

            let transaction = Transaction {
                from: ZERO_ADDRESS,
                to: Address::from_hex(&address)?,
                amount,
                timestamp: 0,
            };

            let transaction_record = TransactionRecord {
                transaction,
                previous_transaction_hash: TransactionHash([0u8; 32]),
                signature: Signature::from_bytes(&[0u8; 64]),
                status: TransactionStatus::Confirmed,
            };

            // Serialize the transaction
            let serialized_transaction_record = bincode::serialize(&transaction_record)
                .map_err(|e| anyhow!("Failed to serialize transaction: {}", e))?;

            // Use the transaction ID as the key
            txn.put(
                self.db,
                &format!("{}:0", &address),
                &serialized_transaction_record,
                lmdb::WriteFlags::empty(),
            )
            .map_err(|e| anyhow!("Failed to put transaction in database: {}", e))?;

            info!("Added genesis balance for address: {}", &address);
        }

        // Commit the transaction
        txn.commit()
            .map_err(|e| anyhow!("Failed to commit genesis transactions: {}", e))?;

        Ok(())
    }

    pub fn add_transaction(
        &mut self,
        from: Address,
        to: Address,
        amount: u64,
        public_key: VerifyingKey,
        timestamp: i64,
        signature: Signature,
    ) -> Result<String> {
        let transaction = Transaction {
            from,
            to,
            amount,
            timestamp,
        };

        if !Self::is_transaction_valid(transaction, public_key, signature)? {
            return Err(anyhow!("Transaction is invalid"));
        }
        let (balance, selfchain_height_from) =
            self.get_address_balance_and_selfchain_height(from)?;
        let (_, selfchain_height_to) = self.get_address_balance_and_selfchain_height(to)?;
        if balance < amount {
            return Err(anyhow!("Unsufficient balance"));
        }

        // write in the DB the transaction to both the recipient and the emitter
        let serialized_tx = bincode::serialize(&transaction)
            .map_err(|e| anyhow!("Failed to serialize transaction: {}", e))?;

        let mut txn = self
            .lmdb_transaction_env
            .begin_rw_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        // We add the transaction to the sender personal chain
        txn.put(
            self.db,
            &format!("{}:{}", from.as_hex(), selfchain_height_from),
            &serialized_tx,
            lmdb::WriteFlags::empty(),
        )
        .map_err(|e| anyhow!("Failed to put transaction in database: {}", e))?;

        let transaction_id = format!("{}:{}", to.as_hex(), selfchain_height_to);

        // As well as the receiver personal chain
        txn.put(
            self.db,
            &transaction_id,
            &serialized_tx,
            lmdb::WriteFlags::empty(),
        )
        .map_err(|e| anyhow!("Failed to put transaction in database: {}", e))?;

        txn.commit()?;

        info!("Successfully added new transaction");

        Ok(transaction_id)
    }

    pub fn get_address_balance_and_selfchain_height(
        &mut self,
        address: Address,
    ) -> Result<(u64, u32)> {
        let mut balance: u64 = 0;

        let reader = self
            .lmdb_transaction_env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let mut iterator = 0;

        loop {
            let key = format!("{}:{}", address.as_hex(), iterator);
            let transaction_bytes = match reader.get(self.db, &key) {
                Ok(bytes) => bytes,
                Err(lmdb::Error::NotFound) => break,
                Err(e) => return Err(anyhow!("Database error: {}", e)),
            };

            let transaction: Transaction = bincode::deserialize(transaction_bytes)
                .map_err(|e| anyhow!("Failed to deserialize transaction: {}", e))?;

            if transaction.from == address {
                if balance < transaction.amount {
                    return Err(anyhow!(
                        "Balance underflow detected for address: {}",
                        address.as_hex()
                    ));
                }
                balance -= transaction.amount;
            } else if transaction.to == address {
                balance += transaction.amount;
            } else {
                return Err(anyhow!(
                    "Transaction {} does not have the address being checked as either sender or receiver",
                    key
                ));
            }
            iterator += 1;
        }

        Ok((balance, iterator))
    }

    pub fn is_transaction_valid(
        transaction: Transaction,
        public_key: VerifyingKey,
        signature: Signature,
    ) -> Result<bool> {
        let transaction_id = transaction.calculate_id()?;

        public_key
            .verify_strict(&transaction_id, &signature)
            .map_err(|e| anyhow!("Signature verification failed: {}", e))?;

        Ok(true)
    }

    #[allow(dead_code)]
    pub fn get_transaction(&self, id: String) -> Result<Transaction> {
        let reader = self
            .lmdb_transaction_env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let transaction_bytes = match reader.get(self.db, &id) {
            Ok(bytes) => bytes,
            Err(lmdb::Error::NotFound) => return Err(anyhow!("Transaction not found")),
            Err(e) => return Err(anyhow!("Database error: {}", e)),
        };

        let transaction: Transaction = bincode::deserialize(transaction_bytes)
            .map_err(|e| anyhow!("Failed to deserialize transaction: {}", e))?;

        Ok(transaction)
    }

    #[allow(dead_code)]
    pub fn get_all_transaction_ids(&self) -> Result<Vec<TransactionHash>> {
        let reader = self
            .lmdb_transaction_env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let mut transaction_ids = Vec::new();

        // Create a cursor to iterate through all entries
        let mut cursor = reader
            .open_ro_cursor(self.db)
            .map_err(|e| anyhow!("Failed to create cursor: {}", e))?;

        // cursor.iter() returns Result<(&[u8], &[u8])>
        // First &[u8] is the key (transaction ID)
        // Second &[u8] is the value (serialized transaction)
        for (result, _) in cursor.iter() {
            let mut id = [0u8; 32];
            id.copy_from_slice(result);
            transaction_ids.push(TransactionHash(id));
        }

        Ok(transaction_ids)
    }
}

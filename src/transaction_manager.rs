use anyhow::{anyhow, Result};
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::Signature;
use k256::ecdsa::VerifyingKey;
use k256::PublicKey;
use lmdb::Cursor;
use lmdb::Database;
use lmdb::Environment;
use lmdb::Transaction as LmdbTransaction;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

use crate::address::{Address, ZERO_ADDRESS};
use crate::serialization::signature::{deserialize_signature, serialize_signature};
use crate::transaction::Amount;
use crate::transaction::{Transaction, TransactionHash};

const DB_NAME: &'static str = "./local_db/transaction_db";

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

#[derive(Deserialize)]
pub struct GenesisArgs {
    pub balances: HashMap<String, u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum TransactionStatus {
    Pending,
    Confirmed,
    Invalid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TransactionRecord {
    transaction: Transaction,
    status: TransactionStatus,
    #[serde(
        serialize_with = "serialize_signature",
        deserialize_with = "deserialize_signature"
    )]
    signature: Signature,
}

pub struct TransactionManager {
    pub lmdb_transaction_env: Arc<Environment>,
    pub db: Database,
}

impl TransactionManager {
    pub fn new() -> Result<Self> {
        let env = LMDB_ENV.clone();
        let db = env.create_db(Some(DB_NAME), lmdb::DatabaseFlags::empty())?;

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
        for (address, amount) in genesis_args.balances {
            let transaction = Transaction {
                from: ZERO_ADDRESS,
                to: Address::from_hex(&address)?,
                amount: Amount::Public(amount),
                timestamp: 0,
                previous_transaction_id: TransactionHash([0u8; 32]),
            };

            let genesis_signature = Signature::try_from([1u8; 64].as_ref())
                .map_err(|e| anyhow!("Failed to create genesis signature: {}", e))?;

            let transaction_record = TransactionRecord {
                transaction,
                signature: genesis_signature,
                status: TransactionStatus::Confirmed,
            };

            // Serialize the transaction
            let serialized_transaction_record = bincode::serialize(&transaction_record)
                .map_err(|e| anyhow!("Failed to serialize transaction: {}", e))?;

            // Use the transaction ID as the key
            txn.put(
                self.db,
                &format!("{}", &address),
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
        amount: Amount,
        public_key: PublicKey,
        timestamp: i64,
        signature: Signature,
        previous_transaction_id: TransactionHash,
    ) -> Result<String> {
        let transaction = Transaction {
            from,
            to,
            amount,
            timestamp,
            previous_transaction_id,
        };

        let message = transaction.calculate_id()?;

        let verifying_key = VerifyingKey::from_affine(public_key.as_affine().clone())
            .map_err(|e| anyhow!("Invalid public key: {}", e))?;

        verifying_key
            .verify(&message, &signature)
            .map_err(|e| anyhow!("Invalid signature: {}", e))?;

        if let Err(err) = self.verify_transaction_chain(&transaction) {
            return Err(anyhow!("Insufficient balance: {}", err));
        }

        // write in the DB the transaction to both the recipient and the emitter
        let serialized_tx = bincode::serialize(&transaction)
            .map_err(|e| anyhow!("Failed to serialize transaction: {}", e))?;

        let mut txn = self
            .lmdb_transaction_env
            .begin_rw_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        // We add the transaction to the sender personal chain
        txn.put(self.db, &message, &serialized_tx, lmdb::WriteFlags::empty())
            .map_err(|e| anyhow!("Failed to put transaction in database: {}", e))?;

        txn.commit()?;

        info!("Successfully added new transaction");

        Ok(hex::encode(message))
    }

    pub fn verify_transaction_chain(&self, transaction_to_verify: &Transaction) -> Result<bool> {
        let reader = self
            .lmdb_transaction_env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let mut found_last_public_transaction = false;
        let mut current_transaction_id = transaction_to_verify.calculate_id()?;
        let mut commitments_chain = Vec::<Amount>::new();

        while !found_last_public_transaction {
            let transaction_bytes = match reader.get(self.db, &current_transaction_id) {
                Ok(bytes) => bytes,
                Err(lmdb::Error::NotFound) => {
                    return Err(anyhow!(
                        "Transaction not found: {:?}",
                        current_transaction_id
                    ))
                }
                Err(e) => return Err(anyhow!("Database error: {}", e)),
            };

            let transaction_record: TransactionRecord = bincode::deserialize(transaction_bytes)
                .map_err(|e| anyhow!("Failed to deserialize transaction: {}", e))?;

            match transaction_record.transaction.amount {
                Amount::Public(_amount) => {
                    commitments_chain.push(transaction_record.transaction.amount);
                    found_last_public_transaction = true;
                }
                Amount::Confidential(ref _confidential) => {
                    let tx_record = transaction_to_verify.calculate_id()?;
                    current_transaction_id = tx_record;
                    commitments_chain.push(transaction_record.transaction.amount);
                }
            }
        }

        // Verify balance consistency between consecutive transactions
        for window in commitments_chain.windows(2) {
            match (&window[0], &window[1]) {
                (Amount::Confidential(current), Amount::Confidential(previous)) => {
                    if !&current.verify_greater_than(&previous)? {
                        return Ok(false);
                    }
                }
                (Amount::Confidential(current), Amount::Public(previous)) => {
                    if !current.verify_greater_than_u64(*previous)? {
                        return Ok(false);
                    }
                }
                _ => continue,
            }
        }

        Ok(true)
    }

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

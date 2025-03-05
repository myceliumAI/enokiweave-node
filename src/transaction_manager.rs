use anyhow::{anyhow, Result};
use k256::ecdsa::signature::Verifier;
use k256::ecdsa::Signature;
use k256::ecdsa::VerifyingKey;
use k256::PublicKey;
use lmdb::Cursor;
use lmdb::Database;
use lmdb::Environment;
use lmdb::Transaction as LmdbTransaction;
use lmdb::{DatabaseFlags, WriteFlags};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

use crate::address::{Address, ZERO_ADDRESS};
use crate::serialization::signature::{deserialize_signature, serialize_signature};
use crate::transaction::Input;
use crate::transaction::Output;
use crate::transaction::PublicInput;
use crate::transaction::PublicOutput;
use crate::transaction::Transaction;
use crate::transaction_hash::TransactionHash;

const DB_NAME: &'static str = "./local_db/transaction_db";
const MERKLE_DB_NAME: &'static str = "./local_db/merkle_db";

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

static MERKLE_LMDB_ENV: Lazy<Arc<Environment>> = Lazy::new(|| {
    std::fs::create_dir_all(MERKLE_DB_NAME).expect("Failed to create merkle_db directory");
    Arc::new(
        lmdb::Environment::new()
            .set_max_dbs(1)
            .set_map_size(10 * 1024 * 1024)
            .set_max_readers(126)
            .open(&Path::new(MERKLE_DB_NAME))
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
    pub merkle_db: Database,
}

impl TransactionManager {
    pub fn new() -> Result<Self> {
        let env = LMDB_ENV.clone();
        let db = env.create_db(Some(DB_NAME), DatabaseFlags::empty())?;
        let merkle_env = MERKLE_LMDB_ENV.clone();
        let merkle_db = merkle_env.create_db(Some(MERKLE_DB_NAME), DatabaseFlags::empty())?;

        Ok(TransactionManager {
            lmdb_transaction_env: env,
            db,
            merkle_db,
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
                inputs: vec![Input::Public(PublicInput {
                    amount,
                    owner: ZERO_ADDRESS,
                })],    
                outputs: vec![Output::Public(PublicOutput {
                    amount,
                    recipient: Address::from_hex(&address)?,
                })],
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
                &Address::from_hex(&address)?.0,
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
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        timestamp: i64,
        signature: Signature,
        previous_transaction_id: TransactionHash,
    ) -> Result<String> {
        let transaction = Transaction {
            inputs,
            outputs,
            timestamp,
            previous_transaction_id,
        };

        let id = transaction.calculate_id()?;

        let reader = self
            .lmdb_transaction_env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        match reader.get(self.db, &id) {
            Ok(_) => return Err(anyhow!("Transaction already exists in DB")),
            Err(lmdb::Error::NotFound) => {}
            Err(e) => return Err(anyhow!("Database error: {}", e)),
        };

        reader.abort();

        if let Err(err) = transaction.verify_amounts_consistency() {
            return Err(anyhow!("{}", err));
        }

        // write in the DB the transaction to both the recipient and the emitter
        let serialized_tx = bincode::serialize(&transaction)
            .map_err(|e| anyhow!("Failed to serialize transaction: {}", e))?;

        let mut txn = self
            .lmdb_transaction_env
            .begin_rw_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        txn.put(self.db, &id, &serialized_tx, lmdb::WriteFlags::empty())
            .map_err(|e| anyhow!("Failed to put transaction in database: {}", e))?;

        txn.commit()?;

        info!("Successfully added new transaction");

        Ok(hex::encode(id))
    }

    pub fn add_merkle_root(&self, root: &[u8; 32]) -> Result<()> {
        let mut txn = self
            .lmdb_transaction_env
            .begin_rw_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        txn.put(self.merkle_db, root, &[], WriteFlags::empty())
            .map_err(|e| anyhow!("Failed to put Merkle root in database: {}", e))?;

        txn.commit()
            .map_err(|e| anyhow!("Failed to commit Merkle root: {}", e))?;

        Ok(())
    }

    pub fn get_merkle_roots(&self) -> Result<Vec<[u8; 32]>> {
        let reader = self
            .lmdb_transaction_env
            .begin_ro_txn()
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;

        let mut roots = Vec::new();
        let mut cursor = reader
            .open_ro_cursor(self.merkle_db)
            .map_err(|e| anyhow!("Failed to create cursor: {}", e))?;

        for (key, _) in cursor.iter() {
            let mut root = [0u8; 32];
            root.copy_from_slice(key);
            roots.push(root);
        }

        Ok(roots)
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

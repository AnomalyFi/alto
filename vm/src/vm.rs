use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::vec;

use crate::capture_logs::capture_logs;
use alto_storage::state_db::StateViewDb;
use alto_storage::transactional_db::TransactionalDb;
use alto_storage::{
    database::Database,
    transactional_db::{InMemoryCachingTransactionalDb, Key, Op},
};
use alto_types::null_error::NullError;
use alto_types::state_view::StateView;
use alto_types::{
    signed_tx::{SignedTx, SignedTxChars},
    tx::{Tx, TxMethods, TxResult, UnitContext},
};
use tracing::info;

pub struct VM {
    pub block_number: u64,
    pub timestamp: u64,
    pub chain_id: u64,
    pub state_cache: Arc<Mutex<HashMap<Key, Op>>>,
    pub unfinalized_state: Arc<Mutex<HashMap<u64, HashMap<Key, Op>>>>,
    pub state_db: Arc<Mutex<dyn Database + Send + Sync>>,
}

impl VM {
    pub fn new(
        block_number: u64,
        timestamp: u64,
        chain_id: u64,
        state_cache: Arc<Mutex<HashMap<Key, Op>>>,
        unfinalized_state: Arc<Mutex<HashMap<u64, HashMap<Key, Op>>>>,
        state_db: Arc<Mutex<dyn Database + Send + Sync>>,
    ) -> Self {
        Self {
            block_number,
            timestamp,
            chain_id,
            state_cache,
            unfinalized_state,
            state_db,
        }
    }

    // applies new set of txs on the given state.
    // apply assumes apply is equivalent to executing all the txs in a block.
    // and moves all the touched state by the txs into unfinalized state.
    pub fn apply(&mut self, stxs: Vec<SignedTx>) -> Vec<TxResult> {
        info!("Applying {} txs", stxs.len());
        let unfinalized_state_for_in_mem =
            merge_maps(self.unfinalized_state.lock().unwrap().clone());
        let mut in_mem_db = InMemoryCachingTransactionalDb::new(
            Arc::clone(&self.state_cache),
            Arc::new(Mutex::new(unfinalized_state_for_in_mem)),
            Arc::clone(&self.state_db),
        );
        let mut results = Vec::new();
        for stx in stxs {
            let mut state_view = StateViewDb::new(&mut in_mem_db);
            let mut tx = stx.tx.clone();
            tx.set_actor(stx.address());
            let result = self.apply_tx(tx, &mut state_view);
            if result.status {
                let _ = in_mem_db.commit_last_tx();
            } else {
                let _ = in_mem_db.rollback_last_tx();
            }
            println!("Tx result: {:?}", result);
            results.push(result);
        }
        
        self.unfinalized_state
            .lock()
            .unwrap()
            .insert(self.block_number, in_mem_db.touched);
        // let _ = in_mem_db.commit();
        // @todo we did not call commit, but moved all the touched state into unfinalized state.
        results
    }

    // applies a single tx on the given state.
    fn apply_tx<'a, T: StateView>(
        &mut self,
        tx: Tx,
        state_view: &mut T,
        // exec_logs: &'a mut Vec<String>,
    ) -> TxResult {
        let tx_context = UnitContext {
            timestamp: self.timestamp,
            chain_id: self.chain_id,
            sender: tx.actor(),
        };
        let mut sv_boxed: Box<&mut dyn StateView> = Box::new(state_view);
        let mut outputs: Vec<Vec<u8>> = Vec::new();
        // apply units one by one.
        // stop and revert if any unit fails.
        let (result, log) = capture_logs(|| {
            for unit in tx.units {
                let res = unit.apply(&tx_context, &mut sv_boxed);
                match res {
                    Ok(output) => {
                        if let Some(output) = output {
                            outputs.push(output);
                        } else {
                            // if output is None, unit execution does not return anything.
                            // push empty vec.
                            outputs.push(vec![]);
                        }
                    }
                    Err(e) => {
                        // return the error.
                        return Err(e);
                    }
                }
            }
            Ok(())
        });
        if result.is_err() {
            TxResult {
                status: false,
                error: result.err().unwrap().to_string(),
                exec_logs: log,
                output: outputs,
            }
        } else {
            TxResult {
                status: true,
                error: NullError.to_string(),
                exec_logs: log,
                output: outputs,
            }
        }
    }
}

fn merge_maps<Key: std::cmp::Eq + std::hash::Hash + Clone, Op: Clone>(
    input: HashMap<u64, HashMap<Key, Op>>,
) -> HashMap<Key, Op> {
    let mut merged = HashMap::new();

    // Sort the keys in ascending order so we can override with higher keys last
    let mut keys: Vec<_> = input.keys().cloned().collect();
    keys.sort();

    for k in keys {
        if let Some(inner_map) = input.get(&k) {
            for (inner_key, val) in inner_map {
                // Insert or override
                merged.insert(inner_key.clone(), val.clone());
            }
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use alto_storage::database::Database;
    use alto_storage::hashmap_db::HashmapDatabase;
    use alto_storage::state_db::StateViewDb;
    use alto_storage::transactional_db::{Key, Op};
    use alto_types::account::Account;
    use alto_types::address::Address;
    use alto_types::create_test_keypair;
    use alto_types::curr_timestamp;
    use alto_types::signed_tx::{SignedTx, SignedTxChars};
    use alto_types::tx::{Tx, TxMethods, Unit};
    use alto_types::units::msg::SequencerMsg;
    use alto_types::units::transfer::Transfer;
    use commonware_codec::{Codec, WriteBuffer};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::vec;

    use super::VM;

    const DB_WRITE_BUFFER_CAPACITY: usize = 500;

    #[test]
    fn test_single_tx() {
        let state_db = Arc::new(Mutex::new(HashmapDatabase::new()));
        let cache: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));
        let unfinalized: Arc<Mutex<HashMap<u64, HashMap<Key, Op>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let address = Address::create_random_address();
        let account = Account {
            address: address.clone(),
            balance: 1000,
        };

        let key = StateViewDb::key_accounts(&account.address);
        let mut write_buf = WriteBuffer::new(DB_WRITE_BUFFER_CAPACITY);
        account.write(&mut write_buf);
        state_db
            .lock()
            .unwrap()
            .put(&key, write_buf.as_ref())
            .unwrap();

        let block_number = 10;
        let timestamp = curr_timestamp();
        let chain_id = 1;
        let mut vm = VM::new(
            block_number,
            timestamp,
            chain_id,
            cache,
            unfinalized,
            state_db,
        );
        let tfer_unit = Transfer {
            to_address: Address::create_random_address(),
            value: 100,
            memo: vec![],
        };
        let msg_unit = SequencerMsg {
            chain_id: 10,
            data: vec![0, 0, 0, 0],
            from_address: Address::create_random_address(),
        };
        let units: Vec<Box<dyn Unit>> = vec![Box::new(tfer_unit), Box::new(msg_unit)];
        let tx = <Tx as TxMethods>::from(timestamp, units, 10, 5, 1, address);
        let (pk, _sk) = create_test_keypair();
        let stx = SignedTx::new(tx, pk, vec![]);
        let results = vm.apply(vec![stx]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, true);
        assert_eq!(results[0].output.len(), 2);
        assert_eq!(results[0].output[0].len(), 0);
        assert_eq!(results[0].error.to_string(), "NoError");
        println!("{}", results[0].exec_logs);
    }
}

use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::vec;

use alto_storage::state_db::StateViewDb;
use alto_storage::transactional_db::TransactionalDb;
use alto_storage::{
    database::Database,
    transactional_db::{InMemoryCachingTransactionalDb, Key, Op},
};
use alto_types::state_view::StateView;
use alto_types::tx::{Tx, TxMethods, Unit, UnitContext};
use alto_types::null_error::NullError;

pub struct VM {
    pub block_number: u64,
    pub timestamp: u64,
    pub chain_id: u64,
    pub state_cache: Arc<Mutex<HashMap<Key, Op>>>,
    pub unfinalized_state: Arc<Mutex<HashMap<Key, Op>>>,
    pub state_db: Arc<Mutex<dyn Database + Send + Sync>>,
}

impl VM {
    pub fn new(
        block_number: u64,
        timestamp: u64,
        chain_id: u64,
        state_cache: Arc<Mutex<HashMap<Key, Op>>>,
        unfinalized_state: Arc<Mutex<HashMap<Key, Op>>>,
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
    pub fn apply(&mut self, txs: Vec<Tx>) -> (Vec<Vec<Vec<u8>>>, Vec<Box<dyn Error>>) {
        let mut in_mem_db = InMemoryCachingTransactionalDb::new(
            Arc::clone(&self.state_cache),
            Arc::clone(&self.unfinalized_state),
            Arc::clone(&self.state_db),
        );
        let mut outputs: Vec<Vec<Vec<u8>>> = Vec::new();
        let mut errors: Vec<Box<dyn Error>> = Vec::new();
        for tx in txs {
            let mut state_view = StateViewDb::new(&mut in_mem_db);
            let result = self.apply_tx(tx.clone(), &mut state_view);
            match result {
                Ok(output) => {
                    // tx executed successfully. 
                    // commit the state changes made by the tx.
                    let _ = in_mem_db.commit_last_tx();
                    // push the output of the tx to the outputs.
                    outputs.push(output);
                    // push null error
                    errors.push(Box::new(NullError));
                }
                Err(e) => {
                    // tx execution failed.
                    // rollback the transaction.
                    let _ = in_mem_db.rollback_last_tx();
                    // push empty vec to the outputs.
                    outputs.push(vec![]);
                    // push the error to the errors.
                    errors.push(e);
                }
            }
        }
        (outputs, errors)
    }

    // applies a single tx on the given state.
    fn apply_tx<T: StateView>(&mut self, tx: Tx, state_view: &mut T) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
        let tx_context = UnitContext {
            timestamp: self.timestamp,
            chain_id: self.chain_id,
            sender: tx.actor(),
        };
        let mut sv_boxed: Box<&mut dyn StateView> = Box::new(state_view);
        let mut outputs:Vec<Vec<u8>> = Vec::new();
        // apply units one by one.
        // stop and revert if any unit fails.
        for unit in tx.units {
            let result = unit.apply(&tx_context, &mut sv_boxed);
            match result {
                Ok(output) => {
                    if let Some(output) = output {
                        outputs.push(output);
                    }else{
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
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use alto_storage::database::Database;
    use alto_storage::hashmap_db::HashmapDatabase;
    use alto_storage::state_db::StateViewDb;
    use alto_types::account::Account;
    use alto_types::address::Address;
    use alto_types::curr_timestamp;
    use alto_types::units::msg::SequencerMsg;
    use alto_types::units::transfer::Transfer;
    use std::sync::{Arc,Mutex};
    use std::collections::HashMap;
    use alto_storage::transactional_db::{Key, Op};
    use alto_types::tx::{Tx,TxMethods, Unit};
    use commonware_codec::{WriteBuffer, Codec};

    use super::VM;

    const DB_WRITE_BUFFER_CAPACITY: usize = 500;

    #[test]
    fn test_single_tx(){
        let state_db = Arc::new(Mutex::new(HashmapDatabase::new()));
        let cache: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));
        let unfinalized: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));

        let address = Address::create_random_address();
        let account = Account{
            address: address.clone(),
            balance: 1000,
        };

        let key = StateViewDb::key_accounts(&account.address);
        let mut write_buf = WriteBuffer::new(DB_WRITE_BUFFER_CAPACITY);
        account.write(&mut write_buf);
        state_db.lock().unwrap().put(&key, write_buf.as_ref()).unwrap();

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
        let tfer_unit = Transfer{
            to_address: Address::create_random_address(),
            value: 100,
            memo: vec![],
        };
        let msg_unit = SequencerMsg{
            chain_id: 10,
            data: vec![0,0,0,0],
            from_address: Address::create_random_address(),
        };
        let units: Vec<Box<dyn Unit>> = vec![Box::new(tfer_unit), Box::new(msg_unit)];
        let tx = <Tx as TxMethods>::from(timestamp, units, 10, 5, 1, address);
        let (outputs, errors) = vm.apply(vec![tx]);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].len(), 2);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "NoError");
    }
}

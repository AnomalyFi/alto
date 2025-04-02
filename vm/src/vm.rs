use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use alto_storage::state_db::StateDb;
use alto_storage::{
    database::Database,
    transactional_db::{Key, Op, InMemoryCachingTransactionalDb},
};
use alto_types::tx::{Tx, Unit, UnitContext, TxMethods};
use alto_types::state_view::StateView;

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
    pub fn apply(&mut self, txs: Vec<Tx>) -> Result<(), Box<dyn Error>> {
        let mut in_mem_db = InMemoryCachingTransactionalDb::new(Arc::clone(&self.state_cache), Arc::clone(&self.unfinalized_state), Arc::clone(&self.state_db));
        let mut state_view = StateDb::new(&mut in_mem_db);
        for tx in txs {
            self.apply_tx(tx.clone(), &mut state_view)?;
        }
        Ok(())
    }

    // applies a single tx on the given state.
    fn apply_tx<T: StateView>(&mut self, tx: Tx, state_view: &mut T) -> Result<(), Box<dyn Error>> {
        let tx_context = UnitContext{
            timestamp: self.timestamp,
            chain_id: self.chain_id,
            sender: tx.actor(),
        };
        let mut sv_boxed:Box<&mut dyn StateView> = Box::new(state_view);
        // apply units one by one.
        // stop and revert if any unit fails.
        // rollback the state changes if unit fails.
        for unit in tx.units {
            // success status.
            // revertion handling.
            unit.apply(&tx_context, &mut sv_boxed)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {}

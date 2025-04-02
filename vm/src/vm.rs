use std::error::Error;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use alto_storage::transactional_db::InMemoryCachingTransactionalDb;
use alto_storage::{transactional_db::{TransactionalDb, Key, Op}, database::Database};
use alto_types::tx::Tx;

pub struct VM {
    pub block_number: u64,
    pub state_cache: Arc<Mutex<HashMap<Key, Op>>>,
    pub unfinalized_state: Arc<Mutex<HashMap<Key, Op>>>,
    pub state_db: Arc<Mutex<dyn Database + Send + Sync>>,
}

impl VM {
    pub fn new(
        block_number: u64,
        state_cache: Arc<Mutex<HashMap<Key, Op>>>,
        unfinalized_state: Arc<Mutex<HashMap<Key, Op>>>,
        state_db: Arc<Mutex<dyn Database + Send + Sync>>,
    ) -> Self {
        Self {
            block_number,
            state_cache,
            unfinalized_state,
            state_db,
        }
    }

    // applies new set of txs on the given state.
    pub fn apply(&mut self, txs: Vec<Tx>) -> Result<(), Box<dyn Error>> {
        // let mut in_mem_db = InMemoryCachingTransactionalDb::new(Arc::clone(self.state_cache), Arc::clone(self.unfinalized_state), self.state_db);
        Ok(())
    }

    // applies a single tx on the given state.
    fn apply_tx(&mut self, tx: Vec<Tx>) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

}

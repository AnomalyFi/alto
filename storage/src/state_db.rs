use crate::transactional_db::TransactionalDb;
use alto_types::account::{Account, Balance};
use alto_types::address::Address;
use alto_types::state_view::StateView;
use bytes::Bytes;
use commonware_codec::{Codec, ReadBuffer, WriteBuffer};
use std::error::Error;
use tracing::{info, warn};
const ACCOUNTS_PREFIX: u8 = 0x0;
pub const DB_WRITE_BUFFER_CAPACITY: usize = 500;

/// StateViewDb is a wrapper around TransactionalDb that provides StateViews for block execution.
/// StateViewDb simplifies the interactions with state by providing methods that abstract away the underlying database operations.
/// It allows for easy retrieval and modification of account states, such as balances.
pub struct StateViewDb<'a> {
    db: &'a mut dyn TransactionalDb,
}

impl StateView for StateViewDb<'_> {
    fn get_account(&mut self, address: &Address) -> Result<Option<Account>, Box<dyn Error>> {
        let key = Self::key_accounts(address);
        self.db.get(&key).and_then(|v| {
            if let Some(value) = v {
                let bytes = Bytes::from(value);
                let mut read_buf = ReadBuffer::new(bytes);
                Account::read(&mut read_buf)
                    .map(Some)
                    .map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Account not found".into())
            }
        })
    }

    fn set_account(&mut self, acc: &Account) -> Result<(), Box<dyn Error>> {
        let key = Self::key_accounts(&acc.address);
        let mut write_buf = WriteBuffer::new(DB_WRITE_BUFFER_CAPACITY);
        acc.write(&mut write_buf);
        self.db.insert(&key, write_buf.as_ref().to_vec())
    }

    fn get_balance(&mut self, address: &Address) -> Option<Balance> {
        info!("Getting balance for address: {}", address);
        match self.get_account(address) {
            // return balance if account exists
            Ok(Some(acc)) => Some(acc.balance),
            // return 0 if no account
            Ok(None) => {
                info!("Account not found, returning 0 balance");
                Some(0)
            }
            // return none if an err occurred
            Err(e) => {
                warn!("Error getting account: {}", e);
                None
            }
        }
    }

    fn set_balance(&mut self, address: &Address, amt: Balance) -> bool {
        info!("Setting balance for address: {}", address);
        match self.get_account(address) {
            Ok(Some(mut acc)) => {
                acc.balance = amt;
                self.set_account(&acc).is_ok()
            }
            Err(e) => {
                warn!("Error getting account: {}", e);
                let acc = Account {
                    address: address.clone(),
                    balance: amt,
                };
                self.set_account(&acc).is_ok()
            }
            _ => false,
        }
    }
}

impl<'a> StateViewDb<'a> {
    pub fn new(db: &'a mut dyn TransactionalDb) -> Self {
        StateViewDb { db }
    }

    pub fn key_accounts(addr: &Address) -> [u8; 33] {
        Self::make_multi_key(ACCOUNTS_PREFIX, addr.as_slice())
    }

    fn make_multi_key(prefix: u8, sub_id: &[u8]) -> [u8; 33] {
        assert_eq!(sub_id.len(), 32, "Sub_id must be exactly 32 bytes");

        let mut key = [0u8; 33];
        key[0] = prefix;
        key[1..33].copy_from_slice(sub_id);
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashmap_db::HashmapDatabase;
    use crate::transactional_db::{InMemoryCachingTransactionalDb, Key, Op};
    use alto_types::address::Address;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_it_works() {
        // setup state db
        let cache: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));
        let unfinalized: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));
        let db = Arc::new(Mutex::new(HashmapDatabase::new()));
        let mut in_mem =
            InMemoryCachingTransactionalDb::new(Arc::clone(&cache), Arc::clone(&unfinalized), db);

        let address = Address::create_random_address();
        let account = Account {
            address: address.clone(),
            balance: 1000,
        };
        let mut state_db = StateViewDb::new(&mut in_mem);
        state_db.set_account(&account).unwrap();
        let _ = in_mem.commit_last_tx();
        let _ = in_mem.commit();
        assert_eq!(unfinalized.lock().unwrap().len(), 1);
        let mut state_db2 = StateViewDb::new(&mut in_mem);
        let retrieved = state_db2.get_account(&address).unwrap().unwrap();
        assert_eq!(retrieved, account);
    }

    #[test]
    #[should_panic]
    fn test_no_account_earlier() {
        // setup state db
        let cache: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));
        let unfinalized: Arc<Mutex<HashMap<Key, Op>>> = Arc::new(Mutex::new(HashMap::new()));
        let db = Arc::new(Mutex::new(HashmapDatabase::new()));
        let mut in_mem =
            InMemoryCachingTransactionalDb::new(Arc::clone(&cache), Arc::clone(&unfinalized), db);

        let address = Address::create_random_address();

        let mut state_db = StateViewDb::new(&mut in_mem);

        let _ = state_db.get_account(&address).unwrap();
    }
}

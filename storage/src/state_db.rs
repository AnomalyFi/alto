use crate::transactional_db::{InMemoryCachingTransactionalDb, TransactionalDb};
use alto_types::account::{Account, Balance};
use alto_types::address::Address;
use bytes::Bytes;
use commonware_codec::{Codec, ReadBuffer, WriteBuffer};
use std::error::Error;

const ACCOUNTS_PREFIX: u8 = 0x0;
const DB_WRITE_BUFFER_CAPACITY: usize = 500;

// StateDb is a wrapper around TransactionalDb that provides StateViews for block execution.
// StateDb simplifies the interactions with state by providing methods that abstract away the underlying database operations.
// It allows for easy retrieval and modification of account states, such as balances.
pub struct StateDb<'a> {
    db: &'a mut dyn TransactionalDb<'a>,
}

impl<'a> StateDb<'a> {
    pub fn new(db: &'a mut dyn TransactionalDb<'a>) -> Self {
        StateDb { db }
    }

    pub fn get_account(&mut self, address: &Address) -> Result<Option<Account>, Box<dyn Error>> {
        let key = Self::key_accounts(address);
        self.db.get(&key).and_then(|v| {
            if let Some(value) = v {
                let bytes = Bytes::from(value);
                let mut read_buf = ReadBuffer::new(bytes);
                Account::read(&mut read_buf).map(Some).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Account not found".into())
            }
        })
    }

    pub fn set_account(&mut self, acc: &Account) -> Result<(), Box<dyn Error>> {
        let key = Self::key_accounts(&acc.address);
        let mut write_buf = WriteBuffer::new(DB_WRITE_BUFFER_CAPACITY);
        acc.write(&mut write_buf);
        self.db.insert(&key, write_buf.as_ref().to_vec())
    }

    pub fn get_balance(&mut self, address: &Address) -> Option<Balance> {
        match self.get_account(address) {
            Ok(Some(acc)) => Some(acc.balance),  // return balance if account exists
            Ok(None) => Some(0),  // return 0 if no account
            Err(_) => None,  // return none if an err occurred
        }
    }

    pub fn set_balance(&mut self, address: &Address, amt: Balance) -> bool {
        match self.get_account(address) {
            Ok(Some(mut acc)) => {
                acc.balance = amt;
                self.set_account(&acc).is_ok()
            }
            _ => false,
        }
    }

    fn key_accounts(addr: &Address) -> [u8; 33] {
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
    use alto_types::address::Address;
    use crate::hashmap_db::HashmapDatabase;
    use crate::transactional_db::{InMemoryCachingTransactionalDb, Op, Key};
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_statedb_accounts() {
        // setup state db
        let mut cache: HashMap<Key, Op> = HashMap::new();
        let mut unfinalized: HashMap<Key, Op> = HashMap::new();
        let db = Arc::new(Mutex::new(HashmapDatabase::new()));
    
        let mut in_mem = InMemoryCachingTransactionalDb::new(&mut cache, &mut unfinalized, db);
        let address = Address::create_random_address();
        {
            let mut state_db = StateDb::new(&mut in_mem);
            // use state_db
            let _ = state_db.get_account(&address); // sample call
        } // <- state_db dropped here
    
        // ✅ Continue using `in_mem` freely
        // let _ = in_mem.commit();
    }
}
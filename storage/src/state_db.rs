use crate::database::Database;
use crate::transactional_db::{TransactionalDb,InMemoryCachingTransactionalDb};
use alto_types::account::{Account, Balance};
use alto_types::address::Address;
use bytes::Bytes;
use commonware_codec::{Codec, ReadBuffer, WriteBuffer};
use std::error::Error;
use crate::rocks_db::RocksDbDatabase;

const ACCOUNTS_PREFIX: u8 = 0x0;
const DB_WRITE_BUFFER_CAPACITY: usize = 500;

pub struct StateDb {
    db: Box<dyn TransactionalDb>,
}
// can use like a redis from Arcadia like get and set for diff types?
impl StateDb {
    pub fn new(db: Box<dyn TransactionalDb>) -> StateDb {
        StateDb { db }
    }

    pub fn get_account(&mut self, address: &Address) -> Result<Option<Account>, Box<dyn Error>> {
        let key = Self::key_accounts(address);
        // try cache first
        if let Some(value) = self.db.get_from_cache(&key)?
            .or_else(|| self.db.get(&key).ok().flatten()) // falls to DB if cache miss
        {
            let bytes = Bytes::from(value);
            let mut read_buf = ReadBuffer::new(bytes);
            let acc = Account::read(&mut read_buf)?;
            return Ok(Some(acc));
        }
        Ok(None)
    }

    pub fn set_account(&mut self, acc: &Account) -> Result<(), Box<dyn Error>> {
        let key = Self::key_accounts(&acc.address);
        let mut write_buf = WriteBuffer::new(DB_WRITE_BUFFER_CAPACITY);
        acc.write(&mut write_buf);
        self.db.put(&key, write_buf.as_ref())?;
        Ok(())
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

impl Database for StateDb {
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn Error>> {
       self.db.put(key, value)
    }

    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.db.get(key)
    }

    fn delete(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>> {
        self.db.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use alto_types::address::Address;
    use alto_types::account::Account;
    use super::*;

    fn setup_state_db() -> StateDb {
        let db = InMemoryCachingTransactionalDb::new(Box::new(
            RocksDbDatabase::new_tmp_db().expect("db could not be created"),
        ));
        StateDb::new(Box::new(db))
    }
    #[test]
    fn test_rocks_db_accounts() {
        let mut state_db = setup_state_db();

        let mut account = Account::new();
        let test_address_bytes = [0u8; 32];
        let test_address = Address::new(&test_address_bytes);
        account.address = test_address.clone();
        account.balance = 100;

        // make sure account does not exist (base case)
        assert!(state_db.get_account(&test_address).unwrap().is_none());

        // create account and check retrieval
        state_db.set_account(&account).unwrap();
        let retrieved_account = state_db.get_account(&test_address).unwrap().expect("Account not found");
        assert_eq!(retrieved_account.address, test_address);
        assert_eq!(retrieved_account.balance, 100);

        // update account balance and check retrieval
        assert!(state_db.set_balance(&test_address, 200));
        assert_eq!(state_db.get_balance(&test_address), Some(200));

        // test if updating balance is persistent
        assert!(state_db.set_balance(&test_address, 300));
        let updated_account = state_db.get_account(&test_address).unwrap().expect("Account not found");
        assert_eq!(updated_account.balance, 300);

        // test retrieval of balance directly
        assert_eq!(state_db.get_balance(&test_address), Some(300));

        // check a non-existent account returns None
        let non_existent_address = Address::new(b"0xDEAD");
        assert!(state_db.get_account(&non_existent_address).unwrap().is_none());
    }
}
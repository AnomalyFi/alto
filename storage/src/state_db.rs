/*
use crate::database::Database;
use alto_types::account::{Account, Balance};
use alto_types::address::Address;
use bytes::Bytes;
use commonware_codec::{Codec, ReadBuffer, WriteBuffer};
use std::error::Error;
use crate::rocks_db::RocksDbDatabase;

const ACCOUNTS_PREFIX: &[u8] = b"sal_accounts";
const DB_WRITE_BUFFER_CAPACITY: usize = 500;

pub struct StateDb<'a> {
    db: Box<dyn Database>,
}
// can use like a redis from Arcadia like get and set for diff types?
impl StateDb {
    pub fn new(db: Box<dyn Database>) -> StateDb {
        StateDb { db }
    }

    pub fn get_account(&mut self, address: &Address) -> Result<Option<Account>, Box<dyn Error>> {
        let key = Self::key_accounts(address);

        let result = self.db.get(key.as_slice())?;
        match result {
            None => Ok(None),
            Some(value) => {
                let bytes = Bytes::copy_from_slice(&value);
                let mut read_buf = ReadBuffer::new(bytes);
                let acc = Account::read(&mut read_buf)?;
                Ok(Some(acc))
            }
        }
    }

    pub fn set_account(&mut self, acc: &Account) -> Result<(), Box<dyn Error>> {
        let key = Self::key_accounts(&acc.address);
        let mut write_buf = WriteBuffer::new(DB_WRITE_BUFFER_CAPACITY);
        acc.write(&mut write_buf);
        self.db.put(&key, write_buf.as_ref()).expect("TODO: panic message");
        Ok(())
    }

    pub fn get_balance(&mut self, address: &Address) -> Option<Balance> {
        let result = self.get_account(address).and_then(|acc| match acc {
            Some(acc) => Ok(acc.balance),
            None => Ok(0),
        });
        match result {
            Ok(balance) => Some(balance),
            _ => None,
        }
    }

    pub fn set_balance(&mut self, address: &Address, amt: Balance) -> bool {
        let result = self.get_account(address);
        match result {
            Ok(Some(mut acc)) => {
                acc.balance = amt;
                let result = self.set_account(&acc);
                result.is_ok()
            }
            _ => false,
        }
    }

    fn key_accounts(addr: &Address) -> Vec<u8> {
        Self::make_multi_key(ACCOUNTS_PREFIX, addr.as_slice())
    }
    fn make_multi_key(prefix: &[u8], sub_id: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(prefix.len() + sub_id.len() + 1);
        key.extend_from_slice(prefix);
        key.push(b':');
        key.extend_from_slice(sub_id);
        key
    }
}

impl<'b> Database for StateDb {
    fn put<'a>(&mut self, key: &'a [u8], value: &[u8]) -> Result<(), Box<dyn Error>> {
       self.db.put(key, value)
    }

    fn get<'a>(&mut self, key: &'a [u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.db.get(key)
    }

    fn delete<'a>(&mut self, key: &'a [u8]) -> Result<(), Box<dyn Error>> {
        self.db.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use alto_types::address::Address;
    use alto_types::account::Account;
    use super::*;

    #[test]
    fn test_rocks_db_accounts() {
        let db = RocksDbDatabase::new_tmp_db().expect("db could not be created");
        let mut state_db = StateDb::new(Box::new(db));

        let mut account = Account::new();
        let test_address = Address::new(b"0xBEEF");
        account.address = test_address.clone();
        account.balance = 100;

        // get account for test address is empty
        let empty_result = state_db.get_account(&test_address);
        empty_result.unwrap().is_none();

        // set account
        state_db.set_account(&account).unwrap();

        let acct_result = state_db.get_account(&test_address).unwrap();
        assert!(acct_result.is_some());
        let account = acct_result.unwrap();
        assert_eq!(account.address, test_address);
        assert_eq!(account.balance, 100);
    }
}
 */
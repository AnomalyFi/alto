use crate::database::Database;
use alto_types::account::{Account, Balance};
use alto_types::address::Address;
use bytes::Bytes;
use commonware_codec::{Codec, ReadBuffer, WriteBuffer};
use std::error::Error;

const ACCOUNTS_PREFIX: &[u8] = b"sal_accounts";
const DB_WRITE_BUFFER_CAPACITY: usize = 500;

pub struct StateDb {
    db: Box<dyn Database>,
}
// can use like a redis from Arcadia like get and set for diff types?
impl StateDb {
    pub fn new(db: Box<dyn Database>) -> StateDb {
        StateDb { db }
    }

    pub fn get_account(&self, address: &Address) -> Result<Option<Account>, Box<dyn Error>> {
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

    pub fn get_balance(&self, address: &Address) -> Option<Balance> {
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

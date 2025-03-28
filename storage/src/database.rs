use alto_types::account::{Balance, Account};
use std::error::Error;
use bytes::Bytes;
use commonware_codec::{Codec, ReadBuffer, WriteBuffer};
use alto_types::address::Address;

// Define database interface that will be used for all impls
pub trait Database {
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn Error>>;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>>;

    fn delete(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>>;
}
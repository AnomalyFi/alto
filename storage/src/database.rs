use std::error::Error;

// Define database interface that will be used for all impls
pub trait Database<'a> {
    fn put(&mut self, key: &'a [u8], value: &[u8]) -> Result<(), Box<dyn Error>>;

    fn get(&mut self, key: &'a [u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>>;

    fn delete(&mut self, key: &'a [u8]) -> Result<(), Box<dyn Error>>;
}
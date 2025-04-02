use std::error::Error;

// Define database interface that will be used for all impls
pub trait Database {
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn Error>>;

    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>>;

    fn delete(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>>;
}

use std::error::Error;

// Define database interface that will be used for all impls
pub trait Database {
    fn put<'a: 'b>(&mut self, key: &'a [u8], value: &[u8]) -> Result<(), Box<dyn Error>>;

    fn get<'a: 'b>(&mut self, key: &'a [u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>>;

    fn delete<'a: 'b>(&mut self, key: &'a [u8]) -> Result<(), Box<dyn Error>>;
}
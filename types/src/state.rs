use std::error::Error;

pub type UnitKey<'a> = &'a [u8;33]; // 1st byte denotes the type of key. 0b for account key, 1b for others.
pub trait State {
    fn get(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>;
    fn get_multi_key(&self, key: Vec<UnitKey>) -> Result<Vec<Vec<u8>>, Box<dyn Error>>;
    fn update(&mut self, key: UnitKey, value: Vec<u8>) -> Result<(), Box<dyn Error>>;
    fn delete(&mut self, key: UnitKey) -> Result<(), Box<dyn Error>>;
    fn commit(&mut self) -> Result<(), Box<dyn Error>>;
    fn rollback(&mut self) -> Result<(), Box<dyn Error>>;
}
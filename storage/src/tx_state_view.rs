use std::collections::HashMap;
use crate::database::Database;
use std::error::Error;

type Key<'a> = &'a [u8;33]; // 1st byte denotes the type of key. 0b for account key, 1b for others.

pub enum OpAction {
    Read, // key was read
    Create, // key was created
    Update, // key was updated
    Delete, // key got deleted
}

pub struct Op<'a>{
    pub action: OpAction,
    pub key: Key<'a>,
    pub value: Vec<u8>,
}

pub struct TxStateView<'a, DB: Database> {
    pub cache: HashMap<Key<'a>, Vec<u8>>, // key-value, state view cache before tx execution. This is not an exhaustive list of all state keys read/write during tx. If cache misses occur, the state view will read from the underlying storage.
    pub ops: Vec<Op<'a>>, // list of state ops applied.
    pub touched: HashMap<Key<'a>, Vec<u8>>, // key-value pairs that were changed during tx execution. This is a subset of the cache.
    pub db: DB, // underlying state storage, to use when cache misses occur.
}


pub trait TxStateViewTrait<DB: Database> {
    fn new(db: DB) -> Self; // create a new instance of TxStateView.
    fn init_cache(&mut self, cache: HashMap<Key, Vec<u8>>); // initialize the cache with an already available hashmap of key-value pairs.
    fn get(&mut self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the state view. If the key is not in the touched and cache, it will read from the underlying storage. add the key-value pair to `touched` and append op to `ops`.
    fn get_from_cache(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_state(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the underlying storage. If the key is not in the storage, it will return an error.
    fn update(&mut self, key: Key, value: Vec<u8>) -> Result<(), Box<dyn Error>>; // update a key in the state view. add the key-value pair to `touched` and append op to `ops`.
    fn delete(&mut self, key: Key) -> Result<(), Box<dyn Error>>; // delete a key in the state view. add the key-value pair to `touched` and append op to `ops`.
    fn rollback(&mut self); // rollback the state view to the state before the tx execution.
}

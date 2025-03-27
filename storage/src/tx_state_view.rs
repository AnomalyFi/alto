use std::collections::HashMap;
use crate::database::Database;
use std::error::Error;
use rocksdb::LogLevel::Error;
use alto_types::state::{State};
use crate::state_db::StateDb;

const ACCOUNT_KEY_TYPE: u8 = 0;
type UnitKey<'a> = alto_types::state::UnitKey<'a>; // 1st byte denotes the type of key. 0b for account key, 1b for others.

pub enum OpAction {
    Read, // key was read
    Create, // key was created
    Update, // key was updated
    Delete, // key got deleted
}

pub struct Op<'a>{
    pub action: OpAction,
    pub key: UnitKey<'a>,
    pub value: Vec<u8>,
}

pub trait TxStateViewTrait: State {
    fn init_cache(&mut self, cache: HashMap<UnitKey, Vec<u8>>); // initialize the cache with an already available hashmap of key-value pairs.
    fn get_from_cache(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_state(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the underlying storage. If the key is not in the storage, it will return an error.
}

pub struct TxStateView<'a> {
    pub cache: HashMap<UnitKey<'a>, Vec<u8>>, // key-value, state view cache before tx execution. This is not an exhaustive list of all state keys read/write during tx. If cache misses occur, the state view will read from the underlying storage.
    pub ops: Vec<Op<'a>>, // list of state ops applied.
    pub touched: HashMap<UnitKey<'a>, Vec<u8>>, // key-value pairs that were changed during tx execution. This is a subset of the cache.
    pub state_db: StateDb, // underlying state storage, to use when cache misses occur.
}

impl<'a> TxStateView<'a> {
    pub fn new(state_db: StateDb) -> Self {
        Self{
            cache: HashMap::new(),
            ops: Vec::new(),
            touched: HashMap::new(),
            state_db,
        }
    }
    // initialize the cache with an already available hashmap of key-value pairs.
    pub fn init_cache(&mut self, cache: HashMap<UnitKey, Vec<u8>>) {
        self.cache = cache;
    }
    pub fn get_from_cache(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.cache.get(&key).map_or(
            Ok(None),
            |v| Ok(Some(v.clone().into())))
    }

    pub fn get_from_state(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match key[0] {
            ACCOUNT_KEY_TYPE => {
                self.get_from_state(key)
            },
            _ => Err(format!("invalid state key {:?}", key[0]).into())
        }
    }

    pub fn get(&self, key: alto_types::state::UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>{
        todo!()
    }
    pub fn get_multi_key(&self, key: Vec<alto_types::state::UnitKey>) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
        todo!()
    }
    pub fn update(&mut self, key: alto_types::state::UnitKey, value: Vec<u8>) -> Result<(), Box<dyn Error>>{
        todo!()
    }
    pub fn delete(&mut self, key: alto_types::state::UnitKey) -> Result<(), Box<dyn Error>> {
        todo!()
    }
    pub fn commit(&mut self) -> Result<(), Box<dyn Error>> {
        todo!()
    }
    pub fn rollback(&mut self) -> Result<(), Box<dyn Error>> {
        todo!()
    }
}
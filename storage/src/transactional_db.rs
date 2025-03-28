use std::collections::HashMap;
use std::error::Error;
use bytes::Bytes;
use alto_types::address::Address;
use alto_types::state::State;
use crate::database::Database;

const ACCOUNT_KEY_TYPE: u8 = 0;

type Key<'a> = &'a[u8];

pub fn decode_unit_key(key: UnitKey) -> (u8, Address) {
    let key_type: u8 = key[0];
    let address_bytes: &[u8] = &key[1..];
    (key_type, address_bytes.into())
}

pub enum OpAction {
    Read, // key was read
    Create, // key was created
    Update, // key was updated
    Delete, // key got deleted
}

pub struct Op<'a>{
    pub action: crate::tx_state_view::OpAction,
    pub key: Key<'a>,
    pub value: Vec<u8>,
}

pub trait TransactionalDb : Database {
    fn init_cache(&mut self, cache: HashMap<Key, Vec<u8>>); // initialize the cache with an already available hashmap of key-value pairs.
    fn get_from_cache(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_db(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the underlying storage. If the key is not in the storage, it will return an error.
    fn commit(&mut self) -> Result<(), Box<dyn Error>>;
    fn rollback(&mut self) -> Result<(), Box<dyn Error>>;
}

pub struct InMemoryCachingTransactionalDb<'a> {
    pub cache: HashMap<Key<'a>, Vec<u8>>, // key-value, state view cache before tx execution. This is not an exhaustive list of all state keys read/write during tx. If cache misses occur, the state view will read from the underlying storage.
    //pub ops: Vec<crate::tx_state_view::Op<'a>>, // list of state ops applied.
    pub touched: HashMap<Key<'a>, Vec<u8>>, // key-value pairs that were changed during tx execution. This is a subset of the cache.
    pub db: Box<dyn Database>, // underlying state storage, to use when cache misses occur.
}

impl<'a> TransactionalDb for InMemoryCachingTransactionalDb<'a> {
    fn init_cache(&mut self, cache: HashMap<Key, Vec<u8>>) {
        self.cache = cache;
    }

    // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_cache(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.cache.get(&key).map_or(
            Ok(None),
            |v| Ok(Some(v.clone().into())))
    }

    // get a key from the underlying storage. If the key is not in the storage, it will return an error.
    fn get_from_db(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.db.get(key)
    }

    fn commit(&mut self) -> Result<(), Box<dyn Error>> {
        for (key, value) in self.touched.iter() {}
    }

    fn rollback(&mut self) -> Result<(), Box<dyn Error>> {
        self.touched.clear();
        Ok(())
    }
}

impl<'a> Database for InMemoryCachingTransactionalDb<'a> {
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn Error>> {
        self.cache.insert(key, value.clone().to_vec());
        self.touched.insert(key, value.to_vec());
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match self.get_from_cache(key) {
            Ok(Some(value)) => Ok(Some(value.into())),
            _ => self.db.get(key),
        }
    }

    // TODO: The below deletes in both cache and underlying db. Change later such that deletes must be committed.
    fn delete(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>> {
        self.cache.remove(key);
        self.db.delete(key)
    }
}

/*
pub trait : State {
    fn init_cache(&mut self, cache: HashMap<UnitKey, Vec<u8>>); // initialize the cache with an already available hashmap of key-value pairs.
    fn get_from_cache(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_state(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the underlying storage. If the key is not in the storage, it will return an error.
}

pub struct TxStateView<'a> {
    pub cache: HashMap<UnitKey<'a>, Vec<u8>>, // key-value, state view cache before tx execution. This is not an exhaustive list of all state keys read/write during tx. If cache misses occur, the state view will read from the underlying storage.
    pub ops: Vec<crate::tx_state_view::Op<'a>>, // list of state ops applied.
    pub touched: HashMap<UnitKey<'a>, Vec<u8>>, // key-value pairs that were changed during tx execution. This is a subset of the cache.
    pub state_db: StateDb, // underlying state storage, to use when cache misses occur.
}

impl<'a> crate::tx_state_view::TxStateView<'a> {
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
        let (key_type, address)
        match key[0] {
            ACCOUNT_KEY_TYPE => {
                self.get_from_state(key)
            },
            _ => Err(format!("invalid state key {:?}", key[0]).into())
        }
    }

    pub fn get(&self, key: UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>>{
    }

    pub fn get_multi_key(&self, key: UnitKey) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
        todo!()
    }
    pub fn update(&mut self, key: UnitKey, value: Vec<u8>) -> Result<(), Box<dyn Error>>{
        todo!()
    }
    pub fn delete(&mut self, key: UnitKey) -> Result<(), Box<dyn Error>> {
        todo!()
    }
    pub fn commit(&mut self) -> Result<(), Box<dyn Error>> {
        todo!()
    }
    pub fn rollback(&mut self) -> Result<(), Box<dyn Error>> {
        todo!()
    }

    pub fn process_get_action(&mut self, cmd_type: u8, key: &UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match cmd_type {
            ACCOUNT_KEY_TYPE => {
                self.state_db.get(key)
            }
            _ => Err(format!("invalid state key {:?}", key).into())
        }
    }

    pub fn process_put_action(&mut self, cmd_type: u8, key: &UnitKey) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match cmd_type {
            ACCOUNT_KEY_TYPE => {
                self.state_db.get(key)
            }
            _ => Err(format!("invalid state key {:?}", key).into())
        }
    }
}

 */

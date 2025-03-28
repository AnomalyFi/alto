use std::collections::HashMap;
use std::error::Error;
use bytes::Bytes;
use alto_types::address::Address;
use alto_types::state::State;
use crate::database::Database;

const ACCOUNT_KEY_TYPE: u8 = 0;

type Key<'a> = &'a[u8];

// pub enum OpAction {
//     Read, // key was read
//     Create, // key was created
//     Update, // key was updated
//     Delete, // key got deleted
// }

// pub struct Op<'a>{
//     pub action: crate::tx_state_view::OpAction,
//     pub key: Key<'a>,
//     pub value: Vec<u8>,
// }

pub trait TransactionalDb<'b> : Database {
    fn init_cache<'a: 'b>(&mut self, cache: HashMap<Key<'b>, Vec<u8>>); // initialize the cache with an already available hashmap of key-value pairs.
    fn get_from_cache<'a: 'b>(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_db(&mut self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the underlying storage. If the key is not in the storage, it will return an error.
    fn commit(&mut self) -> Result<(), Box<dyn Error>>;
    fn rollback(&mut self) -> Result<(), Box<dyn Error>>;
}

pub struct InMemoryCachingTransactionalDb<'a> {
    pub cache: HashMap<Key<'a>, Vec<u8>>, // key-value, state view cache before tx execution. This is not an exhaustive list of all state keys read/write during tx. If cache misses occur, the state view will read from the underlying storage.
    // pub ops: Vec<crate::tx_state_view::Op<'a>>, // list of state ops applied.
    pub touched: HashMap<Key<'a>, Vec<u8>>, // key-value pairs that were changed during tx execution. This is a subset of the cache.
    pub db: Box<dyn Database>, // underlying state storage, to use when cache misses occur.
}
impl<'a> InMemoryCachingTransactionalDb<'a> {
    pub fn new(db: Box<dyn Database>) -> InMemoryCachingTransactionalDb<'a> {
        Self{
            cache: HashMap::new(),
            touched: HashMap::new(),
            db,
        }
    }
    fn get_from_touched<'b>(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.touched.get(&key).map_or(
            Ok(None),
            |v| Ok(Some(v.clone().into())))
    }
}

impl<'b> TransactionalDb<'b> for InMemoryCachingTransactionalDb<'b> {
    fn init_cache<'a: 'b>(&mut self, cache: HashMap<Key, Vec<u8>>) {
        self.cache = cache;
    }

    // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_cache<'a: 'b>(&self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        // searches touched
        if let Some(v) = self.get_from_touched(&key)? {
            return Ok(Some(v.clone()));
        }
        // searches cache
        match self.cache.get(&key) {
            Some(cached_value) => {
                Ok(Some(cached_value.clone().into()))
            }
            // not found in either cache or db
            None => Ok(None),
        }
    }

    // get a key from the underlying storage. If the key is not in the storage, it will return an error.
    fn get_from_db<'a>(&mut self, key: Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.db.get(key)
    }

    fn commit<'a>(&mut self) -> Result<(), Box<dyn Error>> {
        for (key, value) in self.touched.iter() {
            self.cache.insert(key.clone(), value.clone());
            //TODO: what to do if an intermediary operation fails maybe use rocks db transact?
            // ex: what if we go through half of touched and it fails halfway? rare but possible.
            self.db.put(key.clone(), value)?;
        }
        self.touched.clear();
        Ok(())
    }

    fn rollback<'a>(&mut self) -> Result<(), Box<dyn Error>> {
        self.touched.clear();
        Ok(())
    }
}

impl<'b> Database for InMemoryCachingTransactionalDb<'b> {
    fn put<'a: 'b>(&mut self, key: &'a [u8], value: &[u8]) -> Result<(), Box<dyn Error>> {
        self.touched.insert(key, value.to_vec());
        Ok(())
    }

    fn get<'a: 'b>(&mut self, key: &'a [u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match self.get_from_cache(key) {
            Ok(Some(value)) => {
                Ok(Some(value.into()))
            },
            Ok(None) => {
                match self.db.get(key) {
                    Ok(Some(value)) => {
                        self.cache.insert(key, value.clone());
                        Ok(Some(value.into()))
                    }
                    Ok(None) => {
                        Ok(None)
                    },
                    Err(e) => {
                        Err(e)
                    }
                }
            },
            Err(e) => {
                Err(e)
            }
        }
    }

    // TODO: The below deletes in both cache and underlying db. Change later such that deletes must be committed.
    fn delete<'a>(&mut self, key: &'a [u8]) -> Result<(), Box<dyn Error>> {
        self.touched.remove(key);
        self.cache.remove(key);
        self.db.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use crate::hashmap_db::HashmapDatabase;
    use super::*;
    #[test]
    fn test_transactional_db_basic() {
        let mut db = HashmapDatabase::new();
        let mut tx_db = InMemoryCachingTransactionalDb::new(Box::new(db));
        let test_key = b"test_key".to_vec();
        let test_value = b"test_value".to_vec();
        tx_db.put(&test_key, &test_value).unwrap();
    }
}


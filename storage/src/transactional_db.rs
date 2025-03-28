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

pub trait TransactionalDb<'b> : Database<'b> {
    fn init_cache(&mut self, cache: HashMap<Key<'b>, Vec<u8>>); // initialize the cache with an already available hashmap of key-value pairs.
    fn get_from_cache(&self, key: Key<'b>) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_db(&mut self, key: Key<'b>) -> Result<Option<Vec<u8>>, Box<dyn Error>>; // get a key from the underlying storage. If the key is not in the storage, it will return an error.
    fn commit(&mut self) -> Result<(), Box<dyn Error>>;
    fn rollback(&mut self) -> Result<(), Box<dyn Error>>;
}

pub struct InMemoryCachingTransactionalDb<'a> {
    pub cache: HashMap<Key<'a>, Vec<u8>>, // key-value, state view cache before tx execution. This is not an exhaustive list of all state keys read/write during tx. If cache misses occur, the state view will read from the underlying storage.
    // pub ops: Vec<crate::tx_state_view::Op<'a>>, // list of state ops applied.
    pub touched: HashMap<Key<'a>, Vec<u8>>, // key-value pairs that were changed during tx execution. This is a subset of the cache.
    pub db: Box<dyn Database<'a>>, // underlying state storage, to use when cache misses occur.
}
impl<'a> InMemoryCachingTransactionalDb<'a> {
    pub fn new(db: Box<dyn Database<'a>>) -> InMemoryCachingTransactionalDb<'a> {
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
    fn init_cache(&mut self, cache: HashMap<Key<'b>, Vec<u8>>) {
        self.cache = cache;
    }

    // get a key from the cache. If the key is not in the cache, it will return an error.
    fn get_from_cache(&self, key: Key<'b>) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
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
    fn get_from_db(&mut self, key: Key<'b>) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.db.get(key)
    }

    fn commit(&mut self) -> Result<(), Box<dyn Error>> {
        for (key, value) in self.touched.iter() {
            self.cache.insert(key.clone(), value.clone());
            //TODO: what to do if an intermediary operation fails maybe use rocks db transact?
            // ex: what if we go through half of touched and it fails halfway? rare but possible.
            self.db.put(key.clone(), value)?;
        }
        self.touched.clear();
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), Box<dyn Error>> {
        self.touched.clear();
        Ok(())
    }
}

impl<'b> Database<'b> for InMemoryCachingTransactionalDb<'b> {
    fn put(&mut self, key: &'b [u8], value: &[u8]) -> Result<(), Box<dyn Error>> {
        self.touched.insert(key, value.to_vec());
        Ok(())
    }

    fn get(&mut self, key: &'b [u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
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
    fn delete(&mut self, key: &'b [u8]) -> Result<(), Box<dyn Error>> {
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
        let mut hash_db = HashmapDatabase::new();  // underlying store

        let test_key1 = b"test_key1".to_vec();
        let test_value1 = b"test_value1".to_vec();
        let test_key2 = b"test_key2".to_vec();
        let test_value2 = b"test_value2".to_vec();
        let test_key3 = b"test_key3".to_vec();
        let test_value3 = b"test_value3".to_vec();

        {
            let mut tx_db = InMemoryCachingTransactionalDb::new(Box::new(hash_db));

            // should add to cache but not underlying store db
            tx_db.put(&test_key1, &test_value1).unwrap();
            assert!(tx_db.get_from_db(&test_key1).unwrap().is_none());
            assert_eq!(tx_db.get_from_cache(&test_key1).unwrap().is_some(), true);
            assert_eq!(tx_db.get_from_cache(&test_key1).unwrap().unwrap(), test_value1);
            assert_eq!(tx_db.get(&test_key1).unwrap().unwrap(), test_value1);

            // commit the change. should be visible in underlying store.
            tx_db.commit().unwrap();
            assert!(tx_db.get_from_db(&test_key1).unwrap().is_some());
            assert_eq!(tx_db.get_from_db(&test_key1).unwrap().unwrap(), test_value1);
            assert_eq!(tx_db.get_from_cache(&test_key1).unwrap().unwrap(), test_value1);
            assert_eq!(tx_db.get(&test_key1).unwrap().unwrap(), test_value1);

            // add 2nd key-value. do not commit yet.
            tx_db.put(&test_key2, &test_value2).unwrap();
            assert!(tx_db.get_from_db(&test_key2).unwrap().is_none());
            assert_eq!(tx_db.get_from_cache(&test_key2).unwrap().is_some(), true);
            assert_eq!(tx_db.get_from_cache(&test_key2).unwrap().unwrap(), test_value2);
            assert_eq!(tx_db.get(&test_key2).unwrap().unwrap(), test_value2);

            // rollback and commit should make no changes to underlying.
            tx_db.rollback().unwrap();
            tx_db.commit().unwrap();
            assert!(tx_db.get_from_db(&test_key2).unwrap().is_none());
            assert!(tx_db.get_from_cache(&test_key2).unwrap().is_none());
            assert!(tx_db.get(&test_key2).unwrap().is_none());
        }
    }
}


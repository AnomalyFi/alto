use std::collections::HashMap;
use std::error::Error;

use crate::database::Database;
use std::sync::Arc;

pub type Key = [u8; 33];

// i. should track every operation, that a tx does.
// ii. should be able to rollback if a tx reverts.
// iii. should be able to rollback if a block forks.
// iv. should be able to commit to all the state changes once a block has been accepted.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpAction {
    // key is read
    Read, 
    // key is created
    Create, 
    // key is updated
    Update, 
    // key got deleted
    Delete, 
}

/// Op contains action performed and value stored over a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Op{
    // Action performed
    pub action: OpAction,
    // Resulting value after perfromed action.
    pub value: Vec<u8>,
}

/// Implements finalization to database out of TransactionalDb trait.
pub trait TransactionalDb<'a> {
    /// initialize the cache with an already available hashmap of key-value pairs.
    fn init_cache(&mut self, cache: & 'a mut HashMap<Key, Op>);
    /// get the value corresponding to the key. 
    /// use this method for querying state.
    fn get(&mut self, key: &Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>;
    /// insert a key-pair. could be a create or delete action. 
    /// underlying struct should handle the OpAction part.
    fn insert(&mut self, key: &Key, value: Vec<u8>) -> Result<(), Box<dyn Error>>;
    /// delete a key-pair
    fn delete(&mut self, key: &Key) -> Result<(), Box<dyn Error>>;
    /// get a key from cache. do not call this method directly, instead call get.
    fn get_from_cache(&self, key: &Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>;
    /// get a key from the underlying storage. do not call this method directly, instead call get. If the key is not in the storage, it will return an error.
    fn get_from_db(&mut self, key: &Key) -> Result<Option<Vec<u8>>, Box<dyn Error>>; 
    /// commit last tx changes within the cache
    fn commit_last_tx(&mut self) -> Result<(), Box<dyn Error>>;
    /// commit changes to the unfinalized map.
    fn commit(&mut self) -> Result<(), Box<dyn Error>>;
    /// rollback last tx changes within the cache.
    fn rollback_last_tx(&mut self) -> Result<(), Box<dyn Error>>;
    /// rollback entirely.
    fn rollback(&mut self) -> Result<(), Box<dyn Error>>;
}

pub struct InMemoryCachingTransactionalDb<'a> {
    /// cache is init'ed at the start.
    pub cache: &'a mut HashMap<Key, Op>,
    /// unfinalized changes from previous block(s).
    pub unfinalized: &'a mut HashMap<Key, Op>,
    /// set of all key value changes from last init. 
    pub touched: HashMap<Key, Op>, 
    /// set of all key value changes from last commit_last_tx
    pub touched_tx: HashMap<Key, Op>,
    /// underlying database.
    pub db: Arc<std::sync::Mutex<dyn Database + Send + Sync>>, 
}

impl<'a> InMemoryCachingTransactionalDb<'a> {
    pub fn new(cache: &'a mut HashMap<Key, Op>, unfinalized: &'a mut HashMap<Key, Op>, db: Arc<std::sync::Mutex<dyn Database + Send + Sync>>) -> Self {
        Self{
            cache: cache,
            unfinalized: unfinalized,
            touched: HashMap::new(),
            touched_tx: HashMap::new(),
            db,
        }
    }
}

impl<'a> TransactionalDb<'a> for InMemoryCachingTransactionalDb<'a> {
    fn init_cache(&mut self, cache: & 'a mut HashMap<Key, Op>) {
        self.cache = cache;
    }

    fn get(&mut self, key: &Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match self.touched_tx.get(key) {
            // the key is used in the current transaction.
            Some(op) => {
                // No need to update the op.action as create, insert or delete are superior to read.
                Ok(Some(op.value.clone()))
            }
            // key is not used in the current transaction.
            None => {
                match self.touched.get(key) {
                    // key is used in the current block.
                    Some(t_op) => {
                        let v = Op { 
                            action: OpAction::Read, 
                            value:t_op.value.clone() 
                        };
                        self.touched_tx.insert(*key, v);
                        Ok(Some(t_op.value.clone()))
                    }
                    // key is not used in the current block.
                    None => {
                        match self.unfinalized.get(key) {
                            // the key is used in the previous block(s). but the blocks did nt finalze yet.
                            Some(u_op) => {
                                let v = Op { 
                                    action: OpAction::Read, 
                                    value:u_op.value.clone() 
                                };
                                self.touched_tx.insert(*key, v);
                                return Ok(Some(u_op.value.clone()));
                            }
                            // the key is not used in the previous block(s).
                            None => {
                                // check if the key is in the cache.
                                // if it is, return the value.
                                // if it is not, check if the key is in the underlying db.
                                // if it is, return the value.
                                // if it is not, return an error.
                                if let Some(f_c) = self.get_from_cache(key)
                                .ok()
                                .flatten()
                                .or_else(|| self.db.lock().ok()?.get(key).ok().flatten())  {
                                    let v = Op { 
                                        action: OpAction::Read, 
                                        value: f_c.clone() 
                                    };
                                    self.touched_tx.insert(*key, v);
                                    return Ok(Some(f_c));
                                } else {
                                    return Err("Key does not exist.".into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn insert(&mut self, key: &Key, value: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let op = Op {
            action: OpAction::Update, // @todo change this to OpAction::Update or OpAction::Create? based on if the key-pair actually exists.
            value: value,
        };
        self.touched_tx.insert(*key, op);
        Ok(())
    }

    fn delete(&mut self, key: &Key) -> Result<(), Box<dyn Error>> {
        let op = Op{
            action: OpAction::Delete,
            value: vec![],
        };
        self.touched_tx.insert(*key, op);
        Ok(())
    }

    fn get_from_cache(&self, key: &Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.cache.get(key)
            .map(|op| Some(op.value.clone()))
            .ok_or_else(|| "Key not found in cache.".into())
    }

    fn get_from_db(&mut self, key: &Key) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        self.db.lock().unwrap().get(key)
            .map(|v| Some(v.clone()))?
            .ok_or_else(|| "Key not found in db.".into())
    }

    fn commit_last_tx(&mut self) -> Result<(), Box<dyn Error>> {
        merge_maps(&mut self.touched, &self.touched_tx);
        self.touched_tx.clear();
        Ok(())
    }

    fn commit(&mut self) -> Result<(), Box<dyn Error>> {
        merge_maps(&mut self.unfinalized, &self.touched);
        self.touched.clear();
        Ok(())
    }

    fn rollback_last_tx(&mut self) -> Result<(), Box<dyn Error>> {
        self.touched_tx.clear();
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), Box<dyn Error>> {
        self.touched.clear();
        Ok(())
    }
}

pub fn merge_maps<'a, 'b>(map1: & 'a mut HashMap<Key, Op>, map2: & 'b HashMap<Key, Op>) -> & 'a mut HashMap<Key, Op> {
    for (key, op) in map2.iter() {
        if let Some(existing_op) = map1.get(key) {
            // there is a key existing in both maps.
            if op.action == OpAction::Delete {
                map1.insert(*key, op.clone());
            
            }else if op.action == OpAction::Update {
                // if op.action is update, then update the map1. as update superseeds everything.
                map1.insert(*key, op.clone());
            }else if op.action == OpAction::Read && (existing_op.action == OpAction::Update || existing_op.action == OpAction::Create) {
                // reading on a delete will return a nill value with existing op set to delete. this should not be an issue.
                let new_op = Op {
                    action: existing_op.action,
                    value: op.value.clone(),
                };
                map1.insert(*key, new_op.clone());
            }
        } else {
            // there is a key not existing in the first map.
            map1.insert(*key, op.clone());
        }
    }
    map1
}

#[cfg(test)]
mod tests {
    use crate::hashmap_db::HashmapDatabase;
    use super::*;
    use std::sync::Mutex;
    #[test]
    fn test_it_works() {
        let mut cache: HashMap<Key, Op> = HashMap::new();
        let mut unfinalized: HashMap<Key, Op> = HashMap::new();
        let db = Arc::new(Mutex::new(HashmapDatabase::new()));
        let key1 = [1; 33];
        let value1 = [1; 33];
        let key2 = [2; 33];
        let value2 = [2; 33];
        db.lock().unwrap().put(&key1, &value1).unwrap();
        {
            let mut in_mem_db = InMemoryCachingTransactionalDb::new(&mut cache, &mut unfinalized, db.clone());

            // start a tx
            assert_eq!(in_mem_db.get(&key1).unwrap(), Some(value1.to_vec()));
            // end a tx
            assert_eq!(in_mem_db.touched_tx.len(), 1);
            assert_eq!(in_mem_db.touched.len(),0);
            assert_eq!(in_mem_db.cache.len(), 0);
            assert_eq!(in_mem_db.touched_tx.get(&key1).unwrap(), &Op{
                action: OpAction::Read,
                value: value1.to_vec(),
            });
            // commit the tx
            let _ = in_mem_db.commit_last_tx();
            assert_eq!(in_mem_db.touched_tx.len(), 0);
            assert_eq!(in_mem_db.touched.len(),1);
            assert_eq!(in_mem_db.cache.len(), 0);
            assert_eq!(in_mem_db.touched.get(&key1).unwrap(), &Op{
                action: OpAction::Read,
                value: value1.to_vec(),
            });
            // start a new tx
            in_mem_db.insert(&key2, value2.to_vec()).unwrap();
            assert_eq!(in_mem_db.touched_tx.len(), 1);
            assert_eq!(in_mem_db.touched.len(), 1);
            assert_eq!(in_mem_db.cache.len(), 0);
            assert_eq!(in_mem_db.touched_tx.get(&key2).unwrap(), &Op{
                action: OpAction::Update,
                value: value2.to_vec(),
            });
            assert_eq!(in_mem_db.touched.get(&key1).unwrap(), &Op{
                action: OpAction::Read,
                value: value1.to_vec(),
            });
            // end tx
            assert_eq!(in_mem_db.get(&key2).unwrap(), Some(value2.to_vec()));
            assert_eq!(in_mem_db.touched_tx.len(), 1);
            assert_eq!(in_mem_db.touched.len(), 1);
            assert_eq!(in_mem_db.cache.len(), 0);
            // commit tx
            let _ = in_mem_db.commit_last_tx().unwrap();
            assert_eq!(in_mem_db.touched_tx.len(), 0);
            assert_eq!(in_mem_db.touched.len(), 2);
            assert_eq!(in_mem_db.cache.len(), 0);
            // commit block
            let _ = in_mem_db.commit().unwrap();
            assert_eq!(in_mem_db.touched_tx.len(), 0);
            assert_eq!(in_mem_db.touched.len(), 0);
            assert_eq!(in_mem_db.cache.len(), 0);
            assert_eq!(in_mem_db.unfinalized.len(), 2);
        }
        // implement finalize out of db trait.
        assert_eq!(unfinalized.len(), 2);
        assert_eq!(cache.len(), 0);
        for (key, op) in unfinalized.iter() {
            if op.action == OpAction::Delete {
                db.lock().unwrap().delete(key).unwrap();
            } else if op.action == OpAction::Update {
                db.lock().unwrap().put(key, &op.value).unwrap();
            } 
        }
        merge_maps(&mut cache, &unfinalized);
        assert_eq!(cache.len(), 2);
        unfinalized.clear();
        assert_eq!(unfinalized.len(), 0);
        // try fetching key2 from db. this should pass.
        assert_eq!(db.lock().unwrap().get(&key2).unwrap(), Some(value2.to_vec()));
        // try fetching key1 from db. this should pass.
        assert_eq!(db.lock().unwrap().get(&key1).unwrap(), Some(value1.to_vec()));
        // new block
        {
            let mut in_mem_db = InMemoryCachingTransactionalDb::new(&mut cache, &mut unfinalized, db.clone());
            assert_eq!(db.lock().unwrap().get(&key1).unwrap(), Some(value1.to_vec()));
            assert_eq!(in_mem_db.get(&key1).unwrap(), Some(value1.to_vec()));
            assert_eq!(in_mem_db.get(&key2).unwrap(), Some(value2.to_vec()));
            // delete key1
            in_mem_db.delete(&key1).unwrap();
            assert_eq!(in_mem_db.touched_tx.len(), 2);
            assert_eq!(in_mem_db.touched.len(), 0);
            assert_eq!(in_mem_db.cache.len(), 2);
            assert_eq!(in_mem_db.unfinalized.len(), 0);
            assert_eq!(in_mem_db.touched_tx.get(&key1).unwrap(), &Op{
                action: OpAction::Delete,
                value: vec![],
            });
        }
    }
}


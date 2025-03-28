use std::error::Error;
use rocksdb::{DB, Options};
use commonware_codec::{Codec};
use crate::database::Database;
use std::path::Path;
use bytes::{BufMut};
use tempfile::TempDir;

const SAL_ROCKS_DB_PATH: &str = "rocksdb";

pub struct RocksDbDatabase {
    db: DB,
}

impl RocksDbDatabase {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Self::new_with_path(SAL_ROCKS_DB_PATH)
    }

    pub fn new_with_path(path: &str) -> Result<Self, Box<dyn Error>> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        let db_path = Path::new(path);
        let db = DB::open(&opts, &db_path)?;
        Ok(RocksDbDatabase { db })
    }

    pub fn new_tmp_db() -> Result<Self, Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("testdb");
        Self::new_with_path(db_path.to_str().unwrap())
    }

}

impl Database for RocksDbDatabase {
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn Error>> {
        self.db.put(key, value)?;
        Ok(())
    }

    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let result = self.db.get(key)?;
        Ok(result)
    }

    fn delete(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>> {
        self.db.delete(key)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_rocks_db_basic() {
        let mut db = RocksDbDatabase::new().expect("db could not be created");
        let key = b"key1";
        let value = b"value1";
        db.put(key, value).unwrap();
        let retrieved = db.get(key).unwrap().unwrap();
        assert_eq!(retrieved.as_slice(), value);
    }
}

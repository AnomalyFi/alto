use crate::database::Database;
use std::collections::HashMap;
use std::error::Error;

pub struct HashmapDatabase {
    data: HashMap<Vec<u8>, Vec<u8>>,
}

impl Default for HashmapDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl HashmapDatabase {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

impl Database for HashmapDatabase {
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.data.insert(key.into(), value.into());
        Ok(())
    }

    fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        self.data.get(key).map_or(Ok(None), |v| Ok(Some(v.clone())))
    }

    fn delete(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>> {
        self.data.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hashmap_db() {
        let mut db = HashmapDatabase::new();
        let key = b"key1";
        let value = b"value1";
        db.put(key, value).unwrap();
        let retrieved = db.get(key).unwrap().unwrap();
        assert_eq!(retrieved.as_slice(), value);
    }
}

use crate::address::Address;
use crate::state::State;
use crate::tx::{Unit, UnitType, UnitContext};

const MAX_MEMO_SIZE: usize = 256;

#[derive(Debug, Clone)]
pub struct Transfer {
    pub from_address: Address,
    pub to_address: Address,
    pub value: u64,
    pub memo: Vec<u8>,
}

#[derive(Debug)]
pub enum TransferError {
    DuplicateAddress,
    InvalidToAddress,
    InvalidFromAddress,
    InsufficientFunds,
    TooMuchFunds,
    InvalidMemoSize,
    StorageError,
}

impl Unit for Transfer {
    fn unit_type(&self) -> UnitType {
        UnitType::Transfer
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let memo_len = self.memo.len() as u64;
        bytes.extend_from_slice(self.from_address.as_slice());
        bytes.extend_from_slice(self.to_address.as_slice());
        bytes.extend(self.value.to_be_bytes());
        bytes.extend(memo_len.to_be_bytes());
        if memo_len > 0 {
            bytes.extend_from_slice(&self.memo);
        }
        
        bytes
    }

    // @todo introduce syntactic checks.
    fn decode(&mut self, bytes: &[u8]) {
        self.from_address = Address::from_bytes(&bytes[0..32]).unwrap();
        self.to_address = Address::from_bytes(&bytes[32..64]).unwrap();
        self.value = u64::from_be_bytes(bytes[64..72].try_into().unwrap());
        let memo_len = u64::from_be_bytes(bytes[72..80].try_into().unwrap());
        if memo_len > 0 {
            self.memo = bytes[80..(80 + memo_len as usize)].to_vec();
        }
    }

    fn apply(
        &self,
        context: &UnitContext,
        state: &mut Box<dyn State>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        todo!()
    }
}

impl Default for Transfer {
    fn default() -> Self {
        Self {
            from_address: Address::empty(),
            to_address: Address::empty(),
            value: 0,
            memo: vec![],
        }
    }
}
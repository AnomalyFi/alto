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
        todo!()
    }

    fn encode(&self) -> Vec<u8> {
        todo!()
    }

    fn decode(&mut self, bytes: &[u8]) {
        todo!()
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
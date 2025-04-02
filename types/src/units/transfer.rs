use crate::address::Address;
use crate::state_view::StateView;
use crate::tx::{Unit, UnitContext, UnitType};
use std::{any::Any, error::Error, fmt::Display};

const MAX_MEMO_SIZE: usize = 256;

#[derive(Debug, Clone)]
pub struct Transfer {
    pub to_address: Address,
    pub value: u64,
    pub memo: Vec<u8>,
}

impl Transfer {
    pub fn new() -> Transfer {
        Self {
            to_address: Address::empty(),
            value: 0,
            memo: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum TransferError {
    SenderAccountNotFound,
    InsufficientFunds,
    InvalidMemoSize,
    StorageError,
}

impl Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::SenderAccountNotFound => write!(f, "Sender account not found"),
            TransferError::InsufficientFunds => write!(f, "Insufficient funds"),
            TransferError::InvalidMemoSize => write!(f, "Invalid memo size"),
            TransferError::StorageError => write!(f, "Storage error"),
        }
    }
}

impl Error for TransferError {}

impl Unit for Transfer {
    fn unit_type(&self) -> UnitType {
        UnitType::Transfer
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let memo_len = self.memo.len() as u64;
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
        self.to_address = Address::from_bytes(&bytes[0..32]).unwrap();
        self.value = u64::from_be_bytes(bytes[32..40].try_into().unwrap());
        let memo_len = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
        if memo_len > 0 {
            self.memo = bytes[48..(48 + memo_len as usize)].to_vec();
        }
    }

    fn apply(
        &self,
        context: &UnitContext,
        state: &mut Box<&mut dyn StateView>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {

        if self.memo.len() > MAX_MEMO_SIZE {
            return Err(TransferError::InvalidMemoSize.into());
        }

        if let Some(bal) = state.get_balance(&context.sender) {

            if bal < self.value {
                return Err(TransferError::InsufficientFunds.into());
            }

            let receiver_bal = state.get_balance(&self.to_address).unwrap_or(0);

            if !state.set_balance(&context.sender, bal - self.value) || !state.set_balance(&self.to_address, receiver_bal + self.value){
                return Err(TransferError::StorageError.into());
            }
            
        } else {
            return Err(TransferError::SenderAccountNotFound.into());
        }
        
        Ok(None)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Default for Transfer {
    fn default() -> Self {
        Self {
            to_address: Address::empty(),
            value: 0,
            memo: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use more_asserts::assert_gt;
    use std::error::Error;

    #[test]
    fn test_encode_decode() -> Result<(), Box<dyn Error>> {
        let to_address = Address::create_random_address();
        let value = 5;
        let memo = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let origin_msg = Transfer {
            to_address,
            value,
            memo,
        };
        let encoded_bytes = origin_msg.encode();
        assert_gt!(encoded_bytes.len(), 0);
        let mut decoded_msg = Transfer::new();
        decoded_msg.decode(&encoded_bytes);
        assert_eq!(origin_msg.to_address, decoded_msg.to_address);
        assert_eq!(origin_msg.value, decoded_msg.value);
        assert_eq!(origin_msg.memo, decoded_msg.memo);
        Ok(())
    }
}

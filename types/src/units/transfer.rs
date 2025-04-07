use bytes::{Buf, BufMut};
use commonware_utils::SizedSerialize;

use crate::{address::Address, ADDRESSLEN};
use crate::state_view::StateView;
use std::ops::Add;
use std::{any::Any, error::Error, fmt::Display};

use super::{Unit, UnitContext, UnitType};

const MAX_MEMO_SIZE: usize = 256;

#[derive(Debug, Clone)]
pub struct Transfer {
    pub to: Address,
    pub value: u64,
    pub memo: Vec<u8>,
}

impl SizedSerialize for Transfer {
    const SERIALIZED_LEN: usize = ADDRESSLEN + size_of::<u64>() * 2;
}

impl Transfer {
    pub fn new(to: Address, value: u64, memo: Vec<u8>) -> Transfer {
        Self {
            to,
            value,
            memo,
        }
    }

    pub fn decode(mut bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        //  Value + MemoLen + AddressLen + <Memo>
        if bytes.len() < Self::SERIALIZED_LEN {
            return Err("Not enough data to decode transfer".into());
        }

        let to = Address::from_bytes(&bytes.copy_to_bytes(ADDRESSLEN))?;
        let value = bytes.get_u64();
        let memo_len  = bytes.get_u64() as usize;
        if bytes.remaining() != memo_len {
            return Err(format!("Incorrect memo length, wanted: {}, actual: {}", memo_len, bytes.remaining()).into());
        }
        let memo = bytes.copy_to_bytes(memo_len).to_vec();
        Ok( Self { to, value, memo })
    }

    // @todo introduce syntactic checks.
    pub fn decode_box(mut bytes: &[u8]) -> Result<Box<dyn Unit>, Box<dyn Error>> {
        let transfer = Self::decode(bytes)?;
        Ok(Box::new(transfer))
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

        bytes.extend_from_slice(self.to.as_slice());
        bytes.extend(self.value.to_be_bytes());
        bytes.put_u64(self.memo.len() as u64);
        bytes.extend_from_slice(&self.memo);
        bytes
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
            let receiver_bal = state.get_balance(&self.to).unwrap_or(0);

            if !state.set_balance(&context.sender, bal - self.value)
                || !state.set_balance(&self.to, receiver_bal + self.value)
            {
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
            to: Address::empty(),
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
        let to = Address::create_random_address();
        let value = 5;
        let memo = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let origin_msg = Transfer {
            to,
            value,
            memo,
        };
        let encoded_bytes = origin_msg.encode();
        assert_gt!(encoded_bytes.len(), 0);
        let decoded_msg = Transfer::decode(&encoded_bytes)?;
        assert_eq!(origin_msg.to, decoded_msg.to);
        assert_eq!(origin_msg.value, decoded_msg.value);
        assert_eq!(origin_msg.memo, decoded_msg.memo);
        Ok(())
    }
}
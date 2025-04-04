use bytes::{Buf, BufMut};

use crate::{
    address::Address,
    state_view::StateView, ADDRESSLEN,
};
use std::{any::Any, ops::Add};
use std::error::Error;

use super::{Unit, UnitContext, UnitType};

// @todo couple SequencerMsg with DA.
// and skip execution no-op.
#[derive(Clone, Debug)]
pub struct SequencerMsg {
    pub chain_id: u64,
    pub from: Address,
    pub data: Vec<u8>,
}

impl SequencerMsg {
    pub fn new(chain_id: u64, from: Address, data: Vec<u8>) -> SequencerMsg {
        Self {
            chain_id,
            data,
            from,
        }
    }

    // @todo introduce syntactic checks.
    pub fn decode(mut bytes: &[u8]) -> Result<SequencerMsg, Box<dyn Error>> {
        // ChainID + DataLen + AddressLen + <Data>
        let expected_size = size_of::<u64>()*2 + size_of::<Address>();
        if bytes.len() < expected_size {
            return Err(format!("Not enough data to decode sequencer message, wanted: >{}, actual: {}", expected_size, bytes.len()).into());
        }

        let chain_id = bytes.get_u64();
        let from= Address::from_bytes(&bytes.copy_to_bytes(ADDRESSLEN))?;
        let data_len = bytes.get_u64() as usize;
        if bytes.remaining() != data_len {
            return Err(format!("Incorrect data length, wanted: {}, actual: {}", data_len, bytes.remaining()).into());
        }
        let data = bytes.copy_to_bytes(data_len).to_vec();

        Ok(SequencerMsg { chain_id, data, from })
    }

    pub fn decode_box(mut bytes: &[u8]) -> Result<Box<dyn Unit>, Box<dyn Error>> {
        let msg = Self::decode(bytes)?;
        Ok(Box::new(msg))
    }
}

impl Unit for SequencerMsg {
    fn unit_type(&self) -> UnitType {
        UnitType::SequencerMsg
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        // chain id length is 8 bytes.n store chain id.
        bytes.put_u64(self.chain_id);
        // address length is 32. store address.
        bytes.extend_from_slice(self.from.as_slice());
        // store data length.
        bytes.put_u64(self.data.len() as u64);
        // store data.
        bytes.extend_from_slice(&self.data);

        bytes
    }

    fn apply(
        &self,
        _: &UnitContext,
        _: &mut Box<&mut dyn StateView>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        Ok(None)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Default for SequencerMsg {
    fn default() -> Self {
        Self {
            chain_id: 0,
            data: vec![],
            from: Address::empty(),
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
        let chain_id = 4502;
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let from = Address::create_random_address();
        let origin_msg = SequencerMsg {
            chain_id,
            data,
            from,
        };
        let encoded_bytes = origin_msg.encode();
        assert_gt!(encoded_bytes.len(), 0);
        let decoded_msg = SequencerMsg::decode(&encoded_bytes)?;
        assert_eq!(origin_msg.chain_id, decoded_msg.chain_id);
        assert_eq!(origin_msg.data.len(), decoded_msg.data.len());
        assert_eq!(origin_msg.data, decoded_msg.data);
        assert_eq!(origin_msg.from, decoded_msg.from);
        Ok(())
    }
}
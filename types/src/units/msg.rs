use crate::{
    address::Address,
    state_view::StateView,
    tx::{Unit, UnitContext, UnitType},
};
use std::any::Any;

// @todo couple SequencerMsg with DA.
// and skip execution no-op.
#[derive(Clone, Debug)]
pub struct SequencerMsg {
    pub chain_id: u64,
    pub data: Vec<u8>,
    pub from_address: Address,
}

impl SequencerMsg {
    pub fn new() -> SequencerMsg {
        Self {
            chain_id: 0,
            data: Vec::new(),
            from_address: Address::empty(),
        }
    }
}

impl Unit for SequencerMsg {
    fn unit_type(&self) -> UnitType {
        UnitType::SequencerMsg
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        // data length is 8 bytes.
        let data_len = self.data.len() as u64;
        // chain id length is 8 bytes.n store chain id.
        bytes.extend(&self.chain_id.to_be_bytes());
        // address length is 32. store address.
        bytes.extend_from_slice(self.from_address.as_slice());
        // store data length.
        bytes.extend(data_len.to_be_bytes());
        // store data.
        bytes.extend_from_slice(&self.data);

        bytes
    }

    // @todo introduce syntactic checks.
    fn decode(&mut self, bytes: &[u8]) {
        self.chain_id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        self.from_address = Address::from_bytes(&bytes[8..40]).unwrap();
        let data_len = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
        self.data = bytes[48..(48 + data_len as usize)].to_vec();
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
            from_address: Address::empty(),
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
        let from_address = Address::create_random_address();
        let origin_msg = SequencerMsg {
            chain_id,
            data,
            from_address,
        };
        let encoded_bytes = origin_msg.encode();
        assert_gt!(encoded_bytes.len(), 0);
        let mut decoded_msg = SequencerMsg::new();
        decoded_msg.decode(&encoded_bytes);
        assert_eq!(origin_msg.chain_id, decoded_msg.chain_id);
        assert_eq!(origin_msg.data.len(), decoded_msg.data.len());
        assert_eq!(origin_msg.data, decoded_msg.data);
        assert_eq!(origin_msg.from_address, decoded_msg.from_address);
        Ok(())
    }
}

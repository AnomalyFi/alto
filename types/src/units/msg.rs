use crate::{address::Address, tx::{Unit, UnitType, UnitContext}, state::State};

#[derive(Clone, Debug)]
pub struct SequencerMsg {
    pub chain_id: u64,
    pub data: Vec<u8>,
    pub from_address: Address,
    pub relayer_id: u64,
}

impl Unit for SequencerMsg {
    fn unit_type(&self) -> UnitType {
        UnitType::SequencerMsg
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes:Vec<u8> = Vec::new();
        // data length is 8 bytes.
        let data_len = self.data.len() as u64;
        // chain id length is 8 bytes.n store chain id.
        bytes.extend(&self.chain_id.to_be_bytes());
        // address length is 32. store address.
        bytes.extend_from_slice(self.from_address.as_slice());
        // relayer id length is 8 bytes. store relayer id.
        bytes.extend(self.relayer_id.to_be_bytes());
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
        self.relayer_id = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
        let data_len = u64::from_be_bytes(bytes[48..56].try_into().unwrap());
        self.data = bytes[56..(56 + data_len as usize)].to_vec();
    }

    fn apply(
        &self,
        context: &UnitContext,
        state: &mut Box<dyn State>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        todo!()
    }
}

impl Default for SequencerMsg {
    fn default() -> Self {
        Self {
            chain_id: 0,
            data: vec![],
            from_address: Address::empty(),
            relayer_id: 0,
        }
    }
}
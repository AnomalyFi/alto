use crate::{address::Address, tx::{Unit, UnitType, UnitContext}, state::State};

#[derive(Clone, Debug)]
pub struct SequencerMsg {
    pub chain_id: Vec<u8>,
    pub data: Vec<u8>,
    pub from_address: Address,
    pub relayer_id: u64,
}

impl Unit for SequencerMsg {
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

impl Default for SequencerMsg {
    fn default() -> Self {
        Self {
            chain_id: vec![],
            data: vec![],
            from_address: Address::empty(),
            relayer_id: 0,
        }
    }
}
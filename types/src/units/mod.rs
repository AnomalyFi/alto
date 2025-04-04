pub(crate) mod msg;
pub(crate) mod transfer;

use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::sync::OnceLock;

use bytes::{Buf, BufMut};
use msg::SequencerMsg;
use transfer::Transfer;

use crate::address::Address;
use crate::state_view::StateView;

// A registry mapping each UnitType to its decode function.
static UNIT_DECODERS: OnceLock<HashMap<UnitType, fn(&[u8]) -> Result<Box<dyn Unit>, Box<dyn Error>>>> = OnceLock::new();

fn init_registry() -> HashMap<UnitType, fn(&[u8]) -> Result<Box<dyn Unit>, Box<dyn Error>>> {
    let mut m: HashMap<UnitType, fn(&[u8]) -> Result<Box<dyn Unit>, Box<dyn Error>>> = HashMap::new();
    m.insert(UnitType::Transfer, Transfer::decode_box);
    m.insert(UnitType::SequencerMsg, SequencerMsg::decode_box);
    // register additional units as needed...
    m
}

pub fn decode_unit(unit_type: UnitType, data: &[u8]) -> Result<Box<dyn Unit>, Box<dyn Error>> {
    let registry = UNIT_DECODERS.get_or_init(init_registry);
    let Some(decode_fn) = registry.get(&unit_type) else {
        return Err(format!("unsupported unit type {:?}", unit_type).into())
    };

    decode_fn(data)
}

pub fn encode_units(units: &Vec<Box<dyn Unit>>) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.put_u64(units.len() as u64);
    for unit in units.iter() {
        // put unit length + unit type + unit data
        let unit_raw = unit.encode();
        raw.put_u8(unit.unit_type() as u8);
        raw.put_u64(unit_raw.len() as u64);
        raw.extend_from_slice(&unit_raw);
    }

    raw
}

pub fn decode_units<T: Buf>(mut raw: T) -> Result<Vec<Box<dyn Unit>>, Box<dyn Error>> {
    if raw.remaining() < size_of::<u64>() {
        return Err(format!("invalid raw units size: {}", raw.remaining()).into())
    }

    let num_units = raw.get_u64();
    let mut units = Vec::with_capacity(num_units as usize);
    for _ in 0..num_units {
        let unit_type = raw.get_u8();
        let raw_len = raw.get_u64();
        let unit_raw = raw.copy_to_bytes(raw_len as usize);
        let unit = decode_unit(unit_type.try_into()?, &unit_raw)?;
        units.push(unit);
    }

    Ok(units)
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum UnitType {
    Transfer = 1,
    SequencerMsg,
}

impl TryFrom<u8> for UnitType {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(UnitType::Transfer),
            2 => Ok(UnitType::SequencerMsg),
            _ => Err(format!("unknown unit type: {}", value)),
        }
    }
}

pub struct UnitContext {
    // timestamp of the tx.
    pub timestamp: u64,
    // chain id of the tx.
    pub chain_id: u64,
    // sender of the tx.
    pub sender: Address,
}

pub trait UnitClone {
    fn clone_box(&self) -> Box<dyn Unit>;
}

impl<T> UnitClone for T
where
    T: 'static + Unit + Clone,
{
    fn clone_box(&self) -> Box<dyn Unit> {
        Box::new(self.clone())
    }
}

// unit need to be simple and easy to be packed in the tx and executed by the vm.
pub trait Unit: UnitClone + Send + Sync + std::fmt::Debug {
    fn unit_type(&self) -> UnitType;
    fn encode(&self) -> Vec<u8>;

    fn apply(
        &self,
        context: &UnitContext,
        state: &mut Box<&mut dyn StateView>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>>;

    fn as_any(&self) -> &dyn Any;
}

impl Clone for Box<dyn Unit> {
    fn clone(&self) -> Box<dyn Unit> {
        self.clone_box()
    }
}

#[cfg(test)]
mod tests {
    use crate::address::Address;

    use super::{decode_units, encode_units, msg::SequencerMsg, transfer::Transfer, Unit};

    #[test]
    fn test_encode_decode() {
        let mut units: Vec<Box<dyn Unit>> = Vec::new();

        let transfer = Transfer::new(Address::empty(), 100, vec![0, 1, 2, 3]);
        let msg = SequencerMsg::new(0, Address::empty(), vec![3, 4, 5, 6]);

        units.push(Box::new(transfer));
        units.push(Box::new(msg));

        let units_raw = encode_units(&units);
        let decoded_untis = decode_units(units_raw.as_slice()).unwrap();

        assert_eq!(units.len(), decoded_untis.len())
    }
}
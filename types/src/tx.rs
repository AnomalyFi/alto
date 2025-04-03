use commonware_cryptography::sha256;
use commonware_cryptography::sha256::Digest;
use std::any::Any;

use crate::address::Address;
use crate::signed_tx::SignedTx;
use crate::state_view::StateView;
use crate::units;
use crate::wallet::Wallet;
use commonware_utils::SystemTimeExt;
use std::{error::Error, time::SystemTime};

#[derive(Debug)]
pub enum UnitType {
    Transfer,
    SequencerMsg,
}

impl TryFrom<u8> for UnitType {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(UnitType::Transfer),
            1 => Ok(UnitType::SequencerMsg),
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
    fn decode(&mut self, bytes: &[u8]);

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

#[derive(Clone, Debug)]
pub struct Tx {
    /// timestamp of the tx creation. set by the user.
    /// will be verified if the tx is in the valid window once received by validators.
    /// if the timestamp is not in the valid window, the tx will be rejected.
    /// if tx is in a valid window it is added to mempool.
    /// timestamp is used to prevent replay attacks. and counter infinite spam attacks as Tx does not have nonce.
    pub timestamp: u64,
    /// max fee is the maximum fee the user is willing to pay for the tx.
    pub max_fee: u64,
    /// priority fee is the fee the user is willing to pay for the tx to be included in the next block.
    pub priority_fee: u64,
    /// chain id is the id of the chain the tx is intended for.
    pub chain_id: u64,
    /// units are fundamental unit of a tx. similar to actions.
    pub units: Vec<Box<dyn Unit>>,

    /// id is the transaction id. It is the hash of digest.
    pub id: Digest,
    /// digest is encoded tx.
    pub digest: Vec<u8>,
    /// address of the tx sender. wrap this in a better way.
    pub actor: Address,
}

#[derive(Debug)]
pub struct TxResult {
    pub status: bool,
    pub error: Box<dyn Error>,
    pub output: Vec<Vec<u8>>,
    pub exec_logs: String,
}

pub trait TxMethods: Sized {
    /// new is used to create a new instance of Tx with given units and chain id.
    fn new(units: Vec<Box<dyn Unit>>, chain_id: u64) -> Self;
    /// set_fee is used to set the max fee and priority fee of the tx.
    fn set_fee(&mut self, max_fee: u64, priority_fee: u64);
    /// sign is used to sign the tx with the given wallet.
    fn sign(&mut self, wallet: Wallet) -> SignedTx;
    fn from(
        timestamp: u64,
        units: Vec<Box<dyn Unit>>,
        priority_fee: u64,
        max_fee: u64,
        chain_id: u64,
        actor: Address,
    ) -> Self;

    /// returns tx id.
    fn id(&mut self) -> Digest;
    /// returns digest of the tx.
    fn digest(&self) -> Vec<u8>;
    /// encodes the tx, writes to digest and returns the digest.
    /// ensure all fields are properly set before calling this function.
    fn encode(&mut self) -> Vec<u8>;

    fn decode(bytes: &[u8]) -> Result<Self, String>;

    fn set_actor(&mut self, actor: Address);

    fn actor(&self) -> Address;
}

impl Default for Tx {
    fn default() -> Self {
        Self {
            timestamp: 0,
            units: vec![],
            max_fee: 0,
            priority_fee: 0,
            chain_id: 19517,
            id: [0; 32].into(),
            digest: vec![],
            actor: Address::empty(),
        }
    }
}

impl TxMethods for Tx {
    fn new(units: Vec<Box<dyn Unit>>, chain_id: u64) -> Self {
        let mut tx = Self::default();
        tx.timestamp = SystemTime::now().epoch_millis();
        tx.units = units;
        tx.chain_id = chain_id;

        // do not encode and generate tx_id as Tx::new doesnot yet have priority fee and max fee.
        tx
    }

    fn set_fee(&mut self, max_fee: u64, priority_fee: u64) {
        self.max_fee = max_fee;
        self.priority_fee = priority_fee;
    }

    fn sign(&mut self, wallet: Wallet) -> SignedTx {
        SignedTx::sign(self.clone(), wallet)
    }

    fn from(
        timestamp: u64,
        units: Vec<Box<dyn Unit>>,
        priority_fee: u64,
        max_fee: u64,
        chain_id: u64,
        actor: Address,
    ) -> Self {
        let mut tx = Self::default();
        tx.timestamp = timestamp;
        tx.units = units;
        tx.max_fee = max_fee;
        tx.priority_fee = priority_fee;
        tx.chain_id = chain_id;
        tx.actor = actor;
        tx.encode();
        tx
    }

    fn id(&mut self) -> Digest {
        if self.digest.len() == 0 {
            self.encode();
        }
        self.id.clone()
    }

    fn digest(&self) -> Vec<u8> {
        self.digest.clone()
    }

    fn encode(&mut self) -> Vec<u8> {
        if self.digest.is_empty() {
            return self.digest.clone();
        }
        // pack tx timestamp.
        self.digest.extend(self.timestamp.to_be_bytes());
        // pack max fee
        self.digest.extend(self.max_fee.to_be_bytes());
        // pack priority fee
        self.digest.extend(self.priority_fee.to_be_bytes());
        // pack chain id
        self.digest.extend(self.chain_id.to_be_bytes());
        // pack # of units.
        self.digest.extend((self.units.len() as u64).to_be_bytes());
        // pack individual units
        self.units.iter().for_each(|unit| {
            let unit_bytes = unit.encode();
            // pack the unit type info.
            self.digest.extend((unit.unit_type() as u8).to_be_bytes());
            // pack len of inidividual unit.
            self.digest.extend((unit_bytes.len() as u64).to_be_bytes());
            // pack individual unit.
            self.digest.extend_from_slice(&unit_bytes);
        });

        // generate tx id.
        self.id = sha256::hash(&self.digest);

        // return encoded digest.
        self.digest.clone()
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() {
            return Err("Empty bytes".to_string());
        }
        let mut tx = Self::default();
        tx.digest = bytes.to_vec(); // @todo ??
        tx.timestamp = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        tx.max_fee = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        tx.priority_fee = u64::from_be_bytes(bytes[16..24].try_into().unwrap());
        tx.chain_id = u64::from_be_bytes(bytes[24..32].try_into().unwrap());
        let units = unpack_units(&bytes[32..]);
        if units.is_err() {
            return Err(format!("Failed to unpack units: {}", units.unwrap_err()));
        }
        tx.units = units?;
        // generate tx id.
        tx.id = sha256::hash(&tx.digest);
        // return transaction.
        Ok(tx)
    }

    fn set_actor(&mut self, actor: Address) {
        self.actor = actor;
    }

    fn actor(&self) -> Address {
        self.actor.clone()
    }
}

fn unpack_units(digest: &[u8]) -> Result<Vec<Box<dyn Unit>>, String> {
    let mut offset = 0;

    fn read_u8(input: &[u8], offset: &mut usize) -> Result<u8, String> {
        if input.len() < *offset + 1 {
            return Err("Unexpected end of input when reading u8".into());
        }
        let val = input[*offset];
        *offset += 1;
        Ok(val)
    }

    fn read_u64(input: &[u8], offset: &mut usize) -> Result<u64, String> {
        if input.len() < *offset + 8 {
            return Err("Unexpected end of input when reading u64".into());
        }
        let val = u64::from_be_bytes(input[*offset..*offset + 8].try_into().unwrap());
        *offset += 8;
        Ok(val)
    }

    fn read_bytes<'a>(
        input: &'a [u8],
        offset: &'a mut usize,
        len: usize,
    ) -> Result<&'a [u8], String> {
        if input.len() < *offset + len {
            return Err("Unexpected end of input when reading bytes".into());
        }
        let bytes = &input[*offset..*offset + len];
        *offset += len;
        Ok(bytes)
    }

    let unit_count = read_u64(digest, &mut offset)?;

    let mut units: Vec<Box<dyn Unit>> = Vec::with_capacity(unit_count as usize);

    for _ in 0..unit_count {
        let unit_type = read_u8(digest, &mut offset)?;
        let unit_len = read_u64(digest, &mut offset)?;
        let unit_bytes = read_bytes(digest, &mut offset, unit_len as usize)?.to_vec();
        let unit_type = UnitType::try_from(unit_type);
        if unit_type.is_err() {
            return Err(format!("Invalid unit type: {}", unit_type.unwrap_err()));
        }
        let unit_type = unit_type?;
        let unit: Box<dyn Unit> = match unit_type {
            UnitType::Transfer => {
                let mut transfer = units::transfer::Transfer::default();
                transfer.decode(&unit_bytes);
                Box::new(transfer)
            }
            UnitType::SequencerMsg => {
                let mut msg = units::msg::SequencerMsg::default();
                msg.decode(&unit_bytes);
                Box::new(msg)
            }
        };
        units.push(unit);
    }

    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curr_timestamp;
    use crate::units::transfer::Transfer;
    use more_asserts::assert_gt;
    use std::error::Error;

    #[test]
    fn test_encode_decode() -> Result<(), Box<dyn Error>> {
        let timestamp = curr_timestamp();
        let max_fee = 100;
        let priority_fee = 75;
        let chain_id = 45205;
        let transfer = Transfer::new();
        let units: Vec<Box<dyn Unit>> = vec![Box::new(transfer)];
        let digest: [u8; 32] = [0; 32];
        let id = Digest::from(digest.clone());
        // TODO: the .encode call on next line gave error and said origin_msg needed to be mut? but why?
        // shouldn't encode be able to encode without changing the msg?
        let mut origin_msg = Tx {
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            units: units.clone(),
            id,
            actor: Address::empty(),
            digest: digest.to_vec(),
        };
        let encoded_bytes = origin_msg.encode();
        assert_gt!(encoded_bytes.len(), 0);
        let decoded_msg = Tx::decode(&encoded_bytes)?;
        let origin_transfer = origin_msg.units[0]
            .as_ref()
            .as_any()
            .downcast_ref::<Transfer>()
            .expect("Failed to downcast to Transfer");

        let decode_transfer = decoded_msg.units[0]
            .as_ref()
            .as_any()
            .downcast_ref::<Transfer>()
            .expect("Failed to downcast to Transfer");

        assert_eq!(origin_msg.timestamp, decoded_msg.timestamp);
        assert_eq!(origin_msg.max_fee, decoded_msg.max_fee);
        assert_eq!(origin_msg.priority_fee, decoded_msg.priority_fee);
        assert_eq!(origin_msg.chain_id, decoded_msg.chain_id);
        assert_eq!(origin_msg.id, decoded_msg.id);
        assert_eq!(origin_msg.digest, decoded_msg.digest);

        // units
        assert_eq!(origin_transfer.to_address, decode_transfer.to_address);
        assert_eq!(origin_transfer.value, decode_transfer.value);
        assert_eq!(origin_transfer.memo, decode_transfer.memo);
        Ok(())
    }
}

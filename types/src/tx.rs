use bytes::{Buf, BufMut};
use commonware_cryptography::{hash, sha256, Hasher, Scheme};
use commonware_cryptography::sha256::Digest;
use std::any::Any;
use std::cell::OnceCell;
use std::error::Error;
use std::fmt::Debug;
use std::ops::Add;
use std::sync::OnceLock;

use crate::address::Address;
use crate::signed_tx::SignedTx;
use crate::state_view::StateView;
use crate::units::{self, decode_units, encode_units, transfer, Unit, UnitType};
use crate::wallet::Wallet;
use commonware_utils::{SizedSerialize, SystemTimeExt};
use std::time::SystemTime;
use commonware_cryptography::ed25519::PublicKey;
use crate::units::msg::SequencerMsg;
use crate::units::transfer::Transfer;

// TODO: add a commonware_cryptography::Hasher trait for Tx, and the digest should be labeled as H::Digest
#[derive(Clone)]
pub struct Tx<H: Hasher> {
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

    /// id is the transaction id. It is the hash of payload.
    pub id: H::Digest,
    /// payload is encoded tx.
    pub payload: OnceLock<Vec<u8>>,

    // TODO: add a payload referenced by OnceCell here possibly to avoid repeated serialization/deserialization
}

/// The minimal length for a tx
impl <H: Hasher> SizedSerialize for Tx<H> {
    // timestamp + max_fee + priority_fee + chain_id + len(units) + sizeof(Units)
    const SERIALIZED_LEN: usize = size_of::<u64>()*5;
}

impl<H: Hasher> Debug for Tx<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // todo! do any of these need to be hex encoded?
        f.debug_struct("SignedTx")
            .field("timestamp", &self.timestamp)
            .field("max_fee", &self.max_fee)
            .field("priority_fee", &self.priority_fee)
            .field("chain_id", &self.chain_id)
            .field("units", &self.units)
            .field("id", &self.id)
            .field("payload", &self.payload)
            .finish()
    }
}

impl<H: Hasher> Tx<H> {
    fn compute_digest(&self) -> H::Digest {
        if self.payload.get().is_none() {
            let _ = self.serialize();
        }

        let payload = self.payload.get().expect("payload nil");
        let mut hasher = H::new();
        hasher.update(&payload);
        hasher.finalize()
    }

    pub fn digest(&mut self) -> H::Digest {
        self.id
    }

    pub fn validate(&self) -> bool {
        todo!()
    }

    pub fn payload(&self) -> Vec<u8> {
        self.encode()
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.encode()
    }

    pub fn deserialize(raw: &[u8]) -> Result<Self, String> {
        Self::decode(raw)
    }

    // size of the payload
    pub fn size(&mut self) -> usize {
        self.payload().len()
    }

    pub fn random() -> Self {
        // create a tx
        let timestamp = SystemTime::now().epoch_millis();
        let max_fee = 100;
        let priority_fee = 75;
        let chain_id = 45205;
        let transfer = Transfer::new(Address::empty(), 100, vec![34,10,43]);
        let msg = SequencerMsg::new(10, Address::empty(), vec![1, 2, 3]);
        let units: Vec<Box<dyn Unit>> = vec![Box::new(transfer), Box::new(msg)];

        Tx::new(
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            Address::empty(),
            units.clone(),
        )
    }

    pub fn new(
        timestamp: u64,
        max_fee: u64,
        priority_fee: u64,
        chain_id: u64,
        sender: Address,
        units: Vec<Box<dyn Unit>>,
    ) -> Self {
        let mut hasher = H::new();
        hasher.update(&[0; 32]);

        let mut tx = Self {
            id: hasher.finalize(),
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            units,
            payload: OnceLock::new(),
        };
        tx.id = tx.compute_digest();

        tx
    }

    fn set_fee(&mut self, max_fee: u64, priority_fee: u64) {
        self.max_fee = max_fee;
        self.priority_fee = priority_fee;
    }

    fn sign(&mut self, wallet: Wallet) -> SignedTx<H> {
        SignedTx::sign(self.clone(), wallet)
    }

    fn new_from_params(
        timestamp: u64,
        units: Vec<Box<dyn Unit>>,
        priority_fee: u64,
        max_fee: u64,
        chain_id: u64,
        sender: Address,
    ) -> Self {
        let mut tx = Self::default();
        tx.timestamp = timestamp;
        tx.units = units;
        tx.max_fee = max_fee;
        tx.priority_fee = priority_fee;
        tx.chain_id = chain_id;
        tx.encode();
        tx
    }

    pub fn encode(&self) -> Vec<u8> {
        if let Some(payload) = self.payload.get() {
            // TODO: use ref counter instead of copying?
            return payload.to_vec();
        }
        let mut payload: Vec<u8> = Vec::new();
        // pack tx timestamp.
        payload.put_u64(self.timestamp);
        // pack max fee
        payload.put_u64(self.max_fee);
        // pack priority fee
        payload.put_u64(self.priority_fee);
        // pack chain id
        payload.put_u64(self.chain_id);
        // pack # of units.
        let units_raw = encode_units(&self.units);
        payload.extend_from_slice(&units_raw);

        // cache the payload
        self.payload.set(payload).expect("cannot set payload");
        self.payload.get().expect("unable to get payload").to_vec()
    }

    pub fn decode(mut bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < Self::SERIALIZED_LEN {
            return Err(format!("bytes length: {} below min: {}", bytes.len(), Self::SERIALIZED_LEN).into());
        }

        // Store the payload the compute digest
        let mut tx = Self::default();
        tx.payload = OnceLock::from(bytes.to_vec());
        let digest = tx.compute_digest();
        tx.id = digest;

        tx.timestamp = bytes.get_u64();
        tx.max_fee = bytes.get_u64();
        tx.priority_fee = bytes.get_u64();
        tx.chain_id = bytes.get_u64();

        let units = decode_units(bytes).map_err(|e| e.to_string())?;
        tx.units = units;

        Ok(tx)
    }
}

impl<H: Hasher> Default for Tx<H> {
    fn default() -> Self {
        let mut hasher = H::new();
        hasher.update(&[0; 32]);

        Self {
            timestamp: 0,
            units: vec![],
            max_fee: 0,
            priority_fee: 0,
            chain_id: 19517,
            id: hasher.finalize(),
            payload: OnceLock::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::msg::SequencerMsg;
    use crate::units::transfer::Transfer;
    use commonware_cryptography::Sha256;
    use more_asserts::assert_gt;
    use std::error::Error;
    use std::vec;

    #[test]
    fn test_encode_decode() -> Result<(), Box<dyn Error>> {
        let timestamp = SystemTime::now().epoch_millis();
        let max_fee = 100;
        let priority_fee = 75;
        let chain_id = 45205;
        let transfer = Transfer::new(Address::empty(), 100, vec![34,10,43]);
        let msg = SequencerMsg::new(10, Address::empty(), vec![1, 2, 3]);
        let units: Vec<Box<dyn Unit>> = vec![Box::new(transfer), Box::new(msg)];
        // TODO: the .encode call on next line gave error and said origin_msg needed to be mut? but why?
        // shouldn't encode be able to encode without changing the msg?
        let mut tx = Tx::<Sha256>::new(
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            Address::empty(),
            units.clone(),
        );
        let encoded_bytes = tx.encode();
        print!("encoded tx length: {}\n", encoded_bytes.len());
        let decoded_msg = Tx::<Sha256>::decode(&encoded_bytes)?;

        assert_eq!(decoded_msg.payload().len(), encoded_bytes.len());

        let origin_transfer = tx.units[0]
            .as_ref()
            .as_any()
            .downcast_ref::<Transfer>()
            .expect("Failed to downcast to Transfer");

        let decode_transfer = decoded_msg.units[0]
            .as_ref()
            .as_any()
            .downcast_ref::<Transfer>()
            .expect("Failed to downcast to Transfer");

        assert_eq!(tx.timestamp, decoded_msg.timestamp);
        assert_eq!(tx.max_fee, decoded_msg.max_fee);
        assert_eq!(tx.priority_fee, decoded_msg.priority_fee);
        assert_eq!(tx.chain_id, decoded_msg.chain_id);
        assert_eq!(tx.id, decoded_msg.id);
        assert_eq!(tx.payload, decoded_msg.payload);

        // units
        assert_eq!(origin_transfer.to, decode_transfer.to);
        assert_eq!(origin_transfer.value, decode_transfer.value);
        assert_eq!(origin_transfer.memo, decode_transfer.memo);
        Ok(())
    }

    #[test]
    fn test_insufficient_bytes() {
        let timestamp = SystemTime::now().epoch_millis();
        let max_fee = 100;
        let priority_fee = 75;
        let chain_id = 45205;
        let transfer = Transfer::new(Address::empty(), 100, vec![34,10,43]);
        let msg = SequencerMsg::new(10, Address::empty(), vec![1, 2, 3]);
        let units: Vec<Box<dyn Unit>> = vec![Box::new(transfer), Box::new(msg)];
        // TODO: the .encode call on next line gave error and said origin_msg needed to be mut? but why?
        // shouldn't encode be able to encode without changing the msg?
        let mut tx = Tx::<Sha256>::new(
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            Address::empty(),
            units.clone(),
        );
        let encoded_bytes = tx.encode();
        print!("encoded tx length: {}\n", encoded_bytes.len());
        let decode_result = Tx::<Sha256>::decode(&encoded_bytes[0..&encoded_bytes.len() - 10]);

        let err_str = decode_result.map_err(|e| e.to_string()).err().unwrap();
        print!("{}\n", err_str);
        assert!(err_str.contains("remaining bytes invalid to decode a unit"));
    }

    #[test]
    fn test_excessive_bytes() {
        let timestamp = SystemTime::now().epoch_millis();
        let max_fee = 100;
        let priority_fee = 75;
        let chain_id = 45205;
        let transfer = Transfer::new(Address::empty(), 100, vec![34,10,43]);
        let msg = SequencerMsg::new(10, Address::empty(), vec![1, 2, 3]);
        let units: Vec<Box<dyn Unit>> = vec![Box::new(transfer), Box::new(msg)];
        // TODO: the .encode call on next line gave error and said origin_msg needed to be mut? but why?
        // shouldn't encode be able to encode without changing the msg?
        let mut tx = Tx::<Sha256>::new(
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            Address::empty(),
            units.clone(),
        );
        let mut encoded_bytes = tx.encode();
        encoded_bytes.push(10);
        print!("encoded tx length: {}\n", encoded_bytes.len());
        let decode_result = Tx::<Sha256>::decode(&encoded_bytes);

        let err_str = decode_result.map_err(|e| e.to_string()).err().unwrap();
        print!("{}\n", err_str);
        assert!(err_str.contains("left residue after decoding all the units"));
    }


    #[test]
    fn test_below_serialize_len() {
        let timestamp = SystemTime::now().epoch_millis();
        let max_fee = 100;
        let priority_fee = 75;
        let chain_id = 45205;
        let transfer = Transfer::new(Address::empty(), 100, vec![34,10,43]);
        let msg = SequencerMsg::new(10, Address::empty(), vec![1, 2, 3]);
        let units: Vec<Box<dyn Unit>> = vec![Box::new(transfer), Box::new(msg)];
        // TODO: the .encode call on next line gave error and said origin_msg needed to be mut? but why?
        // shouldn't encode be able to encode without changing the msg?
        let mut tx = Tx::<Sha256>::new(
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            Address::empty(),
            units.clone(),
        );
        let mut encoded_bytes = tx.encode();
        encoded_bytes.push(10);
        print!("encoded tx length: {}\n", encoded_bytes.len());
        let decode_result = Tx::<Sha256>::decode(&encoded_bytes[0..Tx::<Sha256>::SERIALIZED_LEN-1]);

        let err_str = decode_result.map_err(|e| e.to_string()).err().unwrap();
        print!("{}\n", err_str);
        assert!(err_str.contains("below min"));
    }

}
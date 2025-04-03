use std::fmt::Debug;
use std::hash::Hash;

use crate::address::Address;
use crate::tx::{Tx};
use crate::wallet::{Wallet, WalletMethods};
use crate::{PublicKey, Signature, TX_NAMESPACE};
use commonware_cryptography::{Ed25519, Hasher, Scheme};
// this is sent by the user to the validators.
#[derive(Clone)]
pub struct SignedTx<H: Hasher> {
    pub tx: Tx<H>,

    pub digest: H::Digest,

    pub_key: PublicKey,
    address: Address,
    signature: Vec<u8>,
}

impl<H: Hasher> Debug for SignedTx<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    } 
}


impl<H: Hasher> SignedTx<H> {
    pub fn payload(&self) -> Vec<u8> {
        todo!()
    } 
    pub fn size(&self) -> usize {
        todo!()
    }

    pub fn serialize(&self) -> Vec<u8> {
        todo!()
    }

    pub fn deserialize(raw: &[u8]) -> Result<Self, String> {
        todo!()
    }

    pub fn validate(&self) -> bool {
        todo!()
    }

    pub fn random() -> Self {
        todo!()
    }



    // @todo either have all fields initialized or none.
    fn new(tx: Tx<H>, pub_key: PublicKey, signature: Vec<u8>) -> Self {
        let mut hasher = H::new();
        let digest = hasher.finalize();
        Self {
            tx,
            pub_key: pub_key.clone(),
            address: Address::from_pub_key(&pub_key),
            signature: signature.clone(),
            digest
        }
    }

    fn verify(&mut self) -> bool {
        let tx_data = self.tx.encode();
        let signature = Signature::try_from(self.signature.as_slice());
        if signature.is_err() {
            return false;
        }
        let signature = signature.unwrap();
        Ed25519::verify(Some(TX_NAMESPACE), &tx_data, &self.pub_key, &signature)
    }

    fn signature(&self) -> Vec<u8> {
        self.signature.clone()
    }

    fn public_key(&self) -> Vec<u8> {
        self.pub_key.to_vec()
    }

    fn address(&self) -> Address {
        self.address.clone()
    }

    // @todo add syntactic checks.
    pub fn encode(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();

        let raw_tx = self.tx.encode();
        let raw_tx_len = raw_tx.len() as u64;
        bytes.extend(raw_tx_len.to_be_bytes());
        bytes.extend_from_slice(&raw_tx);
        bytes.extend_from_slice(&self.pub_key);
        bytes.extend_from_slice(&self.signature);
        bytes
    }

    // @todo add syntactic checks and use methods consume.
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        // @todo this method seems untidy.

        let raw_tx_len = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let raw_tx = &bytes[8..8 + raw_tx_len as usize];
        let pub_key = &bytes[8 + raw_tx_len as usize..8 + raw_tx_len as usize + 32];
        let signature = &bytes[8 + raw_tx_len as usize + 32..];
        let public_key = PublicKey::try_from(pub_key);
        if public_key.is_err() {
            return Err(public_key.unwrap_err().to_string());
        }
        let public_key = public_key.unwrap();
        let tx = Tx::decode(raw_tx);
        if tx.is_err() {
            return Err(tx.unwrap_err());
        }

        let mut hasher = H::new();
        hasher.update(raw_tx);
        let digest = hasher.finalize();

        Ok(SignedTx {
            tx: tx.unwrap(),
            pub_key: public_key.clone(),
            address: Address::from_pub_key(&public_key),
            signature: signature.to_vec(),
            digest
        })
    }

    pub fn sign(mut tx: Tx<H>, mut wallet: Wallet) -> SignedTx<H> {
        let tx_data = tx.encode();

        let mut hasher = H::new();
        hasher.update(&tx_data);
        let digest = hasher.finalize();

        SignedTx {
            tx: tx.clone(),
            signature: wallet.sign(&tx_data),
            address: wallet.address(),
            pub_key: wallet.public_key(),
            digest
        }
    }
}

pub fn pack_signed_txs<H: Hasher>(signed_txs: Vec<SignedTx<H>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend((signed_txs.len() as u64).to_be_bytes());
    for signed_tx in signed_txs {
        // @todo improvise
        let mut signed_tx = signed_tx;
        let signed_tx_bytes = signed_tx.encode();
        bytes.extend((signed_tx_bytes.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&signed_tx_bytes);
    }
    bytes
}

pub fn unpack_signed_txs<H: Hasher>(bytes: Vec<u8>) -> Vec<SignedTx<H>> {
    let signed_txs_len = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
    let mut signed_txs = Vec::with_capacity(signed_txs_len as usize);
    let mut offset = 8;
    for _ in 0..signed_txs_len {
        let signed_tx_len = u64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap());
        offset += 8;
        let signed_tx_bytes = &bytes[offset..offset + signed_tx_len as usize];
        offset += signed_tx_len as usize;
        let signed_tx = SignedTx::decode(signed_tx_bytes);
        if signed_tx.is_err() {
            panic!("Failed to unpack signed tx: {}", signed_tx.unwrap_err());
        }
        signed_txs.push(signed_tx.unwrap());
    }
    signed_txs
}

#[cfg(test)]
mod tests {
    use std::default;
    use std::error::Error;
    use std::hash::Hash;

    use super::*;
    use crate::tx::Unit;
    use crate::units::transfer::Transfer;
    use crate::{create_test_keypair, curr_timestamp};
    use commonware_cryptography::sha256::{self, Digest};
    use commonware_cryptography::Sha256;
    use more_asserts::assert_gt;

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
        let (pk, sk) = create_test_keypair();
        // TODO: the .encode call on next line gave error and said origin_msg needed to be mut? but why?
        // shouldn't encode be able to encode without changing the msg?
        let tx = Tx::<Sha256> {
            timestamp,
            max_fee,
            priority_fee,
            chain_id,
            units: units.clone(),
            id,
            digest: digest.to_vec(),
            actor: Address::empty(),
        };
        let digest = sha256::hash(&[0; 32]);
        let mut origin_msg = SignedTx {
            tx,
            pub_key: pk,
            address: Address::create_random_address(),
            signature: vec![],
            digest
        };
        let encoded_bytes = origin_msg.encode();
        assert_gt!(encoded_bytes.len(), 0);
        let decoded_msg = SignedTx::<Sha256>::decode(&encoded_bytes)?;
        assert_eq!(origin_msg.pub_key, decoded_msg.pub_key);
        assert_eq!(origin_msg.address, decoded_msg.address);
        assert_eq!(origin_msg.signature, decoded_msg.signature);
        // @todo make helper to compare fields in tx and units. same issue when testing in tx.rs file.
        Ok(())
    }
}
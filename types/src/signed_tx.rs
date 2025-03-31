use crate::address::Address;
use crate::{PublicKey, TX_NAMESPACE, Signature};
use crate::wallet::{Wallet, WalletMethods};
use crate::tx::{Tx, TxMethods};
use commonware_cryptography::{Ed25519, Scheme};
// this is sent by the user to the validators.
#[derive(Clone, Debug)]
pub struct SignedTx {
    pub tx: Tx,

    pub_key: PublicKey,
    address: Address,
    signature: Vec<u8>,
}

// function names are self explanatory.
pub trait SignedTxChars:Sized {
    fn new(tx: Tx, pub_key: PublicKey, signature: Vec<u8>) -> Self;
    // fn sign(&mut self, wallet: Wallet) -> SignedTx;
    fn verify(&mut self) -> bool;
    fn signature(&self) -> Vec<u8>;
    fn public_key(&self) -> Vec<u8>;
    fn address(&self) -> Address;
    fn encode(&mut self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self,String>;
}

impl SignedTxChars for SignedTx {
    // @todo either have all fields initialized or none.
    fn new(tx: Tx, pub_key: PublicKey, signature: Vec<u8>) -> Self {
        Self {
            tx,
            pub_key: pub_key.clone(),
            address: Address::from_pub_key(&pub_key),
            signature: signature.clone(),
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
    fn encode(&mut self) -> Vec<u8> {
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
    fn decode(bytes: &[u8]) -> Result<Self, String> { // @todo this method seems untidy.

        let raw_tx_len = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let raw_tx = &bytes[8..8+raw_tx_len as usize];
        let pub_key = &bytes[8+raw_tx_len as usize..8+raw_tx_len as usize+32];
        let signature = &bytes[8+raw_tx_len as usize+32..];
        let public_key = PublicKey::try_from(pub_key);
        if public_key.is_err() {
            return Err(public_key.unwrap_err().to_string());
        }
        let public_key = public_key.unwrap();
        let tx = Tx::decode(raw_tx);
        if tx.is_err(){
           return Err(tx.unwrap_err());
        }

        Ok(SignedTx { tx: tx.unwrap(), pub_key: public_key.clone(), address: Address::from_pub_key(&public_key), signature: signature.to_vec() })
    }
}

impl SignedTx {
    pub fn sign(mut tx: Tx, mut wallet: Wallet) -> SignedTx {
        let tx_data = tx.encode();
        SignedTx {
            tx: tx.clone(),
            signature: wallet.sign(&tx_data),
            address: wallet.address(),
            pub_key: wallet.public_key(),
        }
    }
}

pub fn pack_signed_txs(signed_txs: Vec<SignedTx>) -> Vec<u8> {
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

pub fn unpack_signed_txs(bytes: Vec<u8>) -> Vec<SignedTx> {
    let signed_txs_len = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
    let mut signed_txs = Vec::with_capacity(signed_txs_len as usize);
    let mut offset = 8;
    for i in 0..signed_txs_len {
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
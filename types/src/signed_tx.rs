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
pub trait SignedTxChars {
    fn new(tx: Tx, pub_key: PublicKey, signature: Vec<u8>) -> Self;
    // fn sign(&mut self, wallet: Wallet) -> SignedTx;
    fn verify(&mut self) -> bool;
    fn signature(&self) -> Vec<u8>;
    fn public_key(&self) -> Vec<u8>;
    fn address(&self) -> Address;
    fn encode(&self) -> Vec<u8>;
    fn decode(&self, bytes: &[u8]) -> Self;
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
        self.pub_key.as_ref().to_vec()
    }

    fn address(&self) -> Address {
        self.address.clone()
    }

    fn encode(&self) -> Vec<u8> {
        todo!()
    }

    fn decode(&self, bytes: &[u8]) -> Self {
        todo!()
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
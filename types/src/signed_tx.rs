use crate::Address;
use crate::wallet::{AuthTypes, Wallet};
use crate::tx::{Tx, Unit};

// this is sent by the user to the validators.
pub struct SignedTx<'a> {
    pub tx: Tx<'a>,
    pub auth_type: AuthTypes,


    pub_key: Vec<u8>,
    address: Address,
    signature: Vec<u8>,
}


pub trait SignedTxChars {
    fn new(tx: Tx, auth_type: AuthTypes) -> Self;
    fn sign(&self, wallet: Wallet) -> SignedTx;
    fn verify(&self) -> bool;
    fn signature(&self) -> Vec<u8>;
    fn public_key(&self) -> Vec<u8>;
    fn address(&self) -> Address;
    fn encode(&self) -> Vec<u8>;
    fn decode(&self, bytes: &[u8]) -> Self;
}

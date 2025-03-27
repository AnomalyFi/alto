use crate::Address;
pub enum AuthTypes {
    ED25519,
}


// auth should have a method to verify signatures.
// also batch signature verification.
pub trait Auth {
    fn public_key(&self) -> Vec<u8>; // return the public key of the signer.
    fn address(&self) -> Address; // return the account address of the signer.
    fn verify(&self, data: &[u8], signature: &[u8]) -> bool; // verify a signature.
    fn batch_verify(&self, data: &[u8], signatures: Vec<&[u8]>) -> bool; // batch verify signatures. returns error if batch verification fails.
}


pub struct Wallet {
    auth: AuthTypes, // auth type
    p_key: Vec<u8>, // private key
    pub_key: Vec<u8>, // public key
    address: Address, // account address
}


// wallet generation, management, and signing should be functions of the wallet.
pub trait ImplWallet {
    fn generate(auth_type: AuthTypes) -> Self; // create a new wallet of given auth type.
    fn load(&self, auth_type: AuthTypes, p_key: &[u8]) -> Self; // load a new wallet from private key.
    fn sign(&self, data: &[u8]) -> Vec<u8>; // sign data with the private key.
    fn verify(&self,data: &[u8], signature: &[u8]) -> bool; // verify a signature with the public key.
    fn address(&self) -> Address; // return the account address.
    fn public_key(&self) -> Vec<u8>; // return the public key.
    fn auth_type(&self) -> AuthTypes; // return the auth type.
    fn private_key(&self) -> Vec<u8>; // returns the private key.
    fn store_private_key(&self); // store the private key.
    fn new_address(&mut self); // generate a new address.
}

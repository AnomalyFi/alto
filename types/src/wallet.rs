use crate::address::Address;
use crate::{PrivateKey, PublicKey,Signature, TX_NAMESPACE};
use commonware_cryptography::ed25519::Ed25519;
use commonware_cryptography::Scheme;
use rand::{CryptoRng, Rng};
use std::fmt::Error;
use std::path;

#[derive(Clone, Debug)]
pub enum AuthTypes {
    ED25519,
}

/// auth should have a method to verify signatures.
/// also batch signature verification.
pub trait Auth {
    // returns the public key of the signer.
    fn public_key(&self) -> PublicKey;
    // returns the account address of the signer.
    fn address(&self) -> Address;
    // verifys the signature.
    fn verify(&self, data: &[u8], signature: &[u8]) -> bool; 
    // batch verify signatures. returns false if batch verification fails.
    fn batch_verify(&self, data: &[u8], signatures: Vec<&[u8]>) -> bool;
}

/// Wallet is the module used by the user to sign transactions. Wallet uses Ed25519 signature scheme.
pub struct Wallet {
    // Private key
    priv_key: PrivateKey,
    // Public key
    pub_key: PublicKey,
    // Account Address, is derived from the public key.
    address: Address,
    // Signer
    signer: Ed25519, 
}

// wallet generation, management, and signing should be functions of the wallet.
pub trait WalletMethods {
    // create a new wallet using the given randomness.
    fn generate<R: CryptoRng + Rng>(r: &mut R) -> Self; 
    // load signer from bytes rep of a private key and initialize the wallet.
    fn load(&self, priv_key: &[u8]) -> Self;
    // sign the given arbitary data with the private key of the wallet.
    fn sign(&mut self, data: &[u8]) -> Vec<u8>;
    // verify the signature of the given data with the public key of the wallet.
    fn verify(&self,data: &[u8], signature: &[u8]) -> Result<bool, commonware_cryptography::Error>;
    // return corresponding wallet's address.
    fn address(&self) -> Address;
    // return corresponding wallet's public key.
    fn public_key(&self) -> PublicKey;
    // return corresponding wallet's private key. 
    fn private_key(&self) -> Vec<u8>; 
    // store the private key at the given path.
    fn store_private_key(&self, path: &str) -> Result<(), Error>;
    // @todo remove this?
    fn init_address(&mut self); 
}

impl WalletMethods for Wallet {
    fn generate<R: CryptoRng + Rng>(r: &mut R) -> Self {
        let signer = Ed25519::new(r);
        let pub_key = signer.public_key();
        let address = Address::from_pub_key(&pub_key);
        Self { 
            priv_key: signer.private_key(), 
            pub_key: signer.public_key(), 
            address: address, 
            signer: signer,
        }
    }

    fn load(&self, priv_key: &[u8]) -> Self {
        let private_key = PrivateKey::try_from(priv_key).expect("Invalid private key");
        let signer = <Ed25519 as Scheme>::from(private_key).unwrap();
        Self {
            priv_key: signer.private_key(),
            pub_key: signer.public_key(),
            address: Address::from_pub_key(&signer.public_key()),
            signer: signer,
        }
    }

    fn sign(&mut self, data: &[u8]) -> Vec<u8> {
        self.signer.sign(Some(TX_NAMESPACE), data).as_ref().to_vec()
    }

    fn verify(&self, data: &[u8], signature: &[u8]) -> Result<bool, commonware_cryptography::Error> {
        let signature = Signature::try_from(signature);
        if signature.is_err() {
            return Err(signature.unwrap_err());
        }
        let signature = signature.unwrap();
        let pub_key = self.signer.public_key();
        Ok(Ed25519::verify(Some(TX_NAMESPACE), data, &pub_key, &signature))
    }

    fn address(&self) -> Address {
        self.address.clone()
    }

    fn public_key(&self) -> PublicKey {
        self.pub_key.clone()
    }

    fn private_key(&self) -> Vec<u8> {
        self.priv_key.as_ref().to_vec()
    }

    fn store_private_key(&self, path: &str) -> Result<(), Error> {
        todo!()
    }

    fn init_address(&mut self) {
        todo!()
    }
}